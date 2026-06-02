//! `Provision` sentant — orchestrates the F4 / F4b chain:
//!
//! ```text
//!   beacon-observer  →  device.beacon_observed (provisioning=true)
//!   Provision         ─dispatch→  provision-handshake (PluginCall CMD_START)
//!   provision-handshake  →  device.identity_observed{device_pk, cert_hex, …}
//!   Provision         ─slot-disambiguation→  device.enrolled{slot_id, device_pk, cert_hex}
//!   Roster            ─state-machine→  flashed_pending_pk → enrolled
//! ```
//!
//! Per R2-PROVISION + R2-BLE §6.2. F4b retired F3's hand-rolled
//! 144-byte cert chain (keyholder.SIGN_CERT + provision.COMPOSE_OFFER);
//! the real `DeviceCertificate` is minted by `TrustGroup::process_join_request`
//! inside the provision-handshake substrate using R2-TRUST's canonical
//! 147-byte format.
//!
//! Per [[feedback-sentants-vs-plugins-terminology]] the sentant routes
//! events; the imperative work (BLE radio, signing, cert minting) lives
//! in the substrate components.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use r2_engine::action::PayloadBuf;
use r2_engine::plugin::PluginId;
use r2_engine::{Action, ActionBuf, Event, EventSource, Sentant, StateId, Target};

use crate::bridge::registry;
use crate::roster;
use crate::sentants::RosterCtx;
use crate::substrate::{
    provision_handshake, HandshakeRequest, ProvisionHandshakeSlot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum State { Idle = 0 }

pub struct ProvisionSentant {
    state: State,
    handshake_pid: PluginId,
    handshake_slot: ProvisionHandshakeSlot,
    apiary_ctx: RosterCtx,
    /// BLE addresses we've already dispatched a handshake for. Avoids
    /// re-triggering on every scan window; same provisioning device
    /// emits the same beacon repeatedly until it switches to
    /// `provisioning=false` post-enrolment.
    in_flight: Arc<Mutex<HashMap<String, InFlight>>>,

    beacon_observed_hash:   u32,
    identity_observed_hash: u32,
    handshake_error_hash:   u32,
    device_enrolled_hash:   u32,
    device_transition_hash: u32,
    entry_hash:             u32,
}

#[derive(Debug, Clone)]
struct InFlight {
    ble_addr: String,
    /// Slot we picked (or None if we couldn't disambiguate at dispatch).
    slot_id_hint: Option<String>,
}

impl ProvisionSentant {
    pub fn new(
        handshake_pid: PluginId,
        handshake_slot: ProvisionHandshakeSlot,
        apiary_ctx: RosterCtx,
    ) -> Self {
        let reg = registry();
        Self {
            state: State::Idle,
            handshake_pid,
            handshake_slot,
            apiary_ctx,
            in_flight: Arc::new(Mutex::new(HashMap::new())),

            beacon_observed_hash:   reg.hash_of("r2.composer.device.beacon_observed").unwrap(),
            identity_observed_hash: reg.hash_of("r2.composer.device.identity_observed").unwrap(),
            handshake_error_hash:   reg.hash_of("r2.composer.provision.handshake.error").unwrap(),
            device_enrolled_hash:   reg.hash_of("r2.composer.device.enrolled").unwrap(),
            device_transition_hash: reg.hash_of("r2.composer.device.transition").unwrap(),
            entry_hash:             reg.hash_of("r2.composer.device.entry").unwrap(),
        }
    }
}

impl Sentant for ProvisionSentant {
    fn handle_event(&mut self, event: &Event, actions: &mut ActionBuf) {
        // beacon_observed from substrate/beacon-observer — only Plugin
        // source, only provisioning=true, only unseen addresses.
        if event.hash == self.beacon_observed_hash
            && matches!(event.source, EventSource::Plugin(_))
        {
            self.handle_beacon(event.payload, actions);
            return;
        }
        // identity_observed from substrate/provision-handshake.
        if event.hash == self.identity_observed_hash
            && matches!(event.source, EventSource::Plugin(_))
        {
            self.handle_identity(event.payload, actions);
            return;
        }
        // handshake.error — drop the in-flight entry so the next scan
        // window can re-attempt.
        if event.hash == self.handshake_error_hash
            && matches!(event.source, EventSource::Plugin(_))
        {
            self.handle_handshake_error(event.payload, actions);
            return;
        }
    }

    fn state(&self) -> StateId { self.state as StateId }

    fn class_hash(&self) -> u32 {
        r2_fnv::fnv1a_32(b"ai.reality2.composer.sentant.provision")
    }

    fn name(&self) -> &str { "Provision" }

    fn subscriptions(&self) -> &[u32] {
        use std::sync::OnceLock;
        static SUBS: OnceLock<&'static [u32]> = OnceLock::new();
        SUBS.get_or_init(|| {
            let reg = registry();
            let subs = vec![
                reg.hash_of("r2.composer.device.beacon_observed").unwrap(),
                reg.hash_of("r2.composer.device.identity_observed").unwrap(),
                reg.hash_of("r2.composer.provision.handshake.error").unwrap(),
            ];
            Box::leak(subs.into_boxed_slice())
        })
    }
}

impl ProvisionSentant {
    fn handle_beacon(&mut self, payload: &[u8], actions: &mut ActionBuf) {
        let v: serde_json::Value = serde_json::from_slice(payload)
            .unwrap_or(serde_json::Value::Null);
        let ble_addr = v.get("ble_addr").and_then(|x| x.as_str()).unwrap_or("");
        let provisioning = v.get("provisioning").and_then(|x| x.as_bool()).unwrap_or(false);
        if ble_addr.is_empty() || !provisioning { return; }
        // De-dup: don't re-dispatch a handshake for an address already
        // in flight.
        {
            let in_flight = self.in_flight.lock().unwrap();
            if in_flight.contains_key(ble_addr) { return; }
        }
        // Slot disambiguation — find the unique flashed_pending_pk row.
        let slot_id_hint = self.find_unique_pending_slot();
        if slot_id_hint.is_none() {
            tracing::info!(
                "provision: beacon from {ble_addr} (provisioning=true) but no \
                 unique flashed_pending_pk slot to bind it to — skipping. \
                 (Flash a board via USB first; that creates the slot.)"
            );
            return;
        }
        let device_name = slot_id_hint.as_ref()
            .map(|s| format!("device-{}", &s[..s.len().min(8)]))
            .unwrap_or_else(|| format!("device-{}", &ble_addr[..ble_addr.len().min(8)]));
        // Register as in-flight + fill the handshake substrate's slot.
        self.in_flight.lock().unwrap().insert(
            ble_addr.to_string(),
            InFlight { ble_addr: ble_addr.to_string(), slot_id_hint: slot_id_hint.clone() },
        );
        *self.handshake_slot.lock().unwrap() = Some(HandshakeRequest {
            ble_addr: ble_addr.to_string(),
            device_name,
            slot_id_hint,
        });
        // Dispatch the L2CAP handshake.
        actions.push(Action::PluginCall {
            plugin_id: self.handshake_pid,
            command: provision_handshake::CMD_START,
            data: PayloadBuf::empty(),
        });
    }

    fn handle_identity(&mut self, payload: &[u8], actions: &mut ActionBuf) {
        let v: serde_json::Value = serde_json::from_slice(payload)
            .unwrap_or(serde_json::Value::Null);
        let ble_addr  = v.get("ble_addr").and_then(|x| x.as_str()).unwrap_or("");
        let device_pk = v.get("device_pk").and_then(|x| x.as_str()).unwrap_or("");
        let cert_hex  = v.get("cert_hex").and_then(|x| x.as_str()).unwrap_or("");
        let slot_id_hint = v.get("slot_id_hint").and_then(|x| x.as_str());
        if device_pk.is_empty() || cert_hex.is_empty() {
            tracing::warn!("provision: identity_observed missing device_pk or cert_hex");
            return;
        }
        // Clear the in-flight marker.
        if !ble_addr.is_empty() {
            self.in_flight.lock().unwrap().remove(ble_addr);
        }

        // Decode + persist the cert to disk.
        let cert_bytes = match hex::decode(cert_hex) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("provision: bad cert_hex: {e}");
                return;
            }
        };
        if let Err(e) = self.write_device_cert(device_pk, &cert_bytes) {
            tracing::error!("provision: cert file write failed: {e}");
            // Continue anyway — the slot transition is still useful;
            // operator can re-sync the cert from r2-trust state.
        }

        // Slot disambiguation. Prefer the hint from the handshake (we
        // set it at dispatch); fall back to the unique-flashed-pending-pk
        // heuristic if the hint was None or stale.
        let slot_id = match slot_id_hint {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => match self.find_unique_pending_slot() {
                Some(s) => s,
                None => {
                    tracing::warn!(
                        "provision: device_pk={}.. enrolled via handshake but no \
                         flashed_pending_pk slot to assign — write a slot or \
                         operator must reflash with --slot-id explicit",
                        &device_pk[..device_pk.len().min(8)]
                    );
                    return;
                }
            }
        };

        // Emit the high-level device.enrolled event the Roster sentant
        // (already in place from F3) consumes to do the state transition.
        let enrolled = serde_json::to_vec(&serde_json::json!({
            "slot_id":   slot_id,
            "device_pk": device_pk,
            "cert_hex":  cert_hex,
            "ble_addr":  ble_addr,
        })).unwrap_or_default();
        actions.push(Action::Send {
            target: Target::Broadcast,
            event_hash: self.device_enrolled_hash,
            payload: PayloadBuf::from_slice(&enrolled),
        });
    }

    fn handle_handshake_error(&mut self, payload: &[u8], actions: &mut ActionBuf) {
        let v: serde_json::Value = serde_json::from_slice(payload)
            .unwrap_or(serde_json::Value::Null);
        let ble_addr = v.get("ble_addr").and_then(|x| x.as_str()).unwrap_or("");
        let message = v.get("message").and_then(|x| x.as_str()).unwrap_or("");
        let code    = v.get("code").and_then(|x| x.as_u64()).unwrap_or(0);
        tracing::warn!(
            "provision: handshake error from {ble_addr}: code={code:#04X} {message}"
        );
        if !ble_addr.is_empty() {
            self.in_flight.lock().unwrap().remove(ble_addr);
        }
        // Re-broadcast for the webapp's build console.
        actions.push(Action::Send {
            target: Target::Broadcast,
            event_hash: self.handshake_error_hash,
            payload: PayloadBuf::from_slice(payload),
        });
        // Avoid unused-field warnings until F4c stack-visualisation
        // integration uses these.
        let _ = (self.device_transition_hash, self.entry_hash);
    }

    /// Find the unique slot in `flashed_pending_pk` state. Returns
    /// `Some(slot_id)` only if there's exactly one such slot — otherwise
    /// None (caller must ask the operator to disambiguate).
    fn find_unique_pending_slot(&self) -> Option<String> {
        let ctx = self.apiary_ctx.lock().unwrap();
        let apiary_dir = ctx.as_ref()?;
        let r = roster::load(apiary_dir);
        let mut hits = r.devices.iter()
            .filter(|d| d.state == "flashed_pending_pk");
        let first = hits.next()?;
        if hits.next().is_some() { return None; } // ambiguous
        Some(first.slot_id.clone())
    }

    fn write_device_cert(&self, device_pk_hex: &str, cert_bytes: &[u8]) -> std::io::Result<()> {
        let apiary_dir = {
            let ctx = self.apiary_ctx.lock().unwrap();
            ctx.as_ref().cloned()
        };
        let apiary = match apiary_dir {
            Some(p) => p,
            None => return Ok(()),  // nothing we can do; not fatal
        };
        let cert_dir: PathBuf = apiary.join("devices/certs");
        std::fs::create_dir_all(&cert_dir)?;
        let cert_path = cert_dir.join(format!("{device_pk_hex}.bin"));
        std::fs::write(&cert_path, cert_bytes)?;
        tracing::info!(
            "provision: wrote {} ({} bytes — R2-TRUST DeviceCertificate)",
            cert_path.display(), cert_bytes.len()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roster::Roster;

    fn ctx_for(dir: &std::path::Path) -> RosterCtx {
        Arc::new(Mutex::new(Some(dir.to_path_buf())))
    }

    fn ev_plugin(hash: u32, payload: &[u8]) -> Event<'_> {
        Event { hash, payload, source: EventSource::Plugin(0), msg_id: 0 }
    }

    fn make(dir: &std::path::Path) -> ProvisionSentant {
        let slot: ProvisionHandshakeSlot = Arc::new(Mutex::new(None));
        ProvisionSentant::new(1, slot, ctx_for(dir))
    }

    #[test]
    fn beacon_provisioning_dispatches_handshake() {
        let dir = tempfile::tempdir().unwrap();
        // Seed a flashed_pending_pk slot so disambiguation succeeds.
        let mut r = Roster::default();
        let mut row = roster::new_placeholder(
            "sensor", "rocker-sensor", "esp32-s3-xiao", "kitchen", "2026-06-02T00:00:00Z");
        row.state = "flashed_pending_pk".to_string();
        let slot_id = row.slot_id.clone();
        r.devices.push(row);
        roster::save(dir.path(), &r).unwrap();

        let mut s = make(dir.path());
        let mut actions = ActionBuf::new();
        let payload = br#"{"ble_addr":"AA:BB:CC:DD:EE:FF","provisioning":true,"rssi":-60}"#;
        s.handle_event(&ev_plugin(s.beacon_observed_hash, payload), &mut actions);

        // Substrate slot should now hold the handshake request.
        let req = s.handshake_slot.lock().unwrap();
        let req = req.as_ref().expect("handshake slot filled");
        assert_eq!(req.ble_addr, "AA:BB:CC:DD:EE:FF");
        assert_eq!(req.slot_id_hint.as_deref(), Some(slot_id.as_str()));

        let collected: Vec<_> = actions.drain().collect();
        assert!(collected.iter().any(|a| matches!(a,
            Action::PluginCall { plugin_id, command, .. }
              if *plugin_id == 1 && *command == provision_handshake::CMD_START
        )), "expected PluginCall(handshake, CMD_START)");
    }

    #[test]
    fn beacon_already_in_flight_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = Roster::default();
        let mut row = roster::new_placeholder(
            "sensor", "rocker-sensor", "esp32-s3-xiao", "kitchen", "2026-06-02T00:00:00Z");
        row.state = "flashed_pending_pk".to_string();
        r.devices.push(row);
        roster::save(dir.path(), &r).unwrap();
        let mut s = make(dir.path());

        let mut actions = ActionBuf::new();
        let payload = br#"{"ble_addr":"AA:BB:CC:DD:EE:FF","provisioning":true}"#;
        s.handle_event(&ev_plugin(s.beacon_observed_hash, payload), &mut actions);
        let first: Vec<_> = actions.drain().collect();
        assert!(!first.is_empty(), "first observation should dispatch");
        // Clear the substrate slot so we can detect a second fill.
        let _ = s.handshake_slot.lock().unwrap().take();

        let mut actions2 = ActionBuf::new();
        s.handle_event(&ev_plugin(s.beacon_observed_hash, payload), &mut actions2);
        let second: Vec<_> = actions2.drain().collect();
        assert!(second.is_empty(), "in-flight beacon should NOT re-dispatch");
        assert!(s.handshake_slot.lock().unwrap().is_none(),
                "handshake slot should be empty on the second observation");
    }

    #[test]
    fn beacon_no_pending_slot_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        // Empty roster.
        let mut s = make(dir.path());
        let mut actions = ActionBuf::new();
        let payload = br#"{"ble_addr":"AA:BB:CC:DD:EE:FF","provisioning":true}"#;
        s.handle_event(&ev_plugin(s.beacon_observed_hash, payload), &mut actions);
        let collected: Vec<_> = actions.drain().collect();
        assert!(collected.is_empty(), "no flashed_pending_pk slot ⇒ no dispatch");
    }

    #[test]
    fn beacon_non_provisioning_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = make(dir.path());
        let mut actions = ActionBuf::new();
        let payload = br#"{"ble_addr":"AA:BB:CC:DD:EE:FF","provisioning":false}"#;
        s.handle_event(&ev_plugin(s.beacon_observed_hash, payload), &mut actions);
        assert!(actions.is_empty());
    }

    #[test]
    fn identity_writes_cert_and_emits_enrolled() {
        let dir = tempfile::tempdir().unwrap();
        // Seed a flashed_pending_pk slot.
        let mut r = Roster::default();
        let mut row = roster::new_placeholder(
            "sensor", "rocker-sensor", "esp32-s3-xiao", "kitchen", "2026-06-02T00:00:00Z");
        row.state = "flashed_pending_pk".to_string();
        let slot_id = row.slot_id.clone();
        r.devices.push(row);
        roster::save(dir.path(), &r).unwrap();

        let mut s = make(dir.path());
        let device_pk_hex = "ab".repeat(32);
        let cert_hex = "11".repeat(147); // R2-TRUST canonical length
        let payload = serde_json::to_vec(&serde_json::json!({
            "ble_addr": "AA:BB:CC:DD:EE:FF",
            "device_pk": device_pk_hex,
            "cert_hex":  cert_hex,
            "slot_id_hint": slot_id,
        })).unwrap();
        let mut actions = ActionBuf::new();
        s.handle_event(&ev_plugin(s.identity_observed_hash, &payload), &mut actions);

        // Cert file was written, R2-TRUST format (147 bytes).
        let cert_path = dir.path().join("devices/certs").join(format!("{device_pk_hex}.bin"));
        assert!(cert_path.exists(), "cert file missing");
        assert_eq!(std::fs::read(&cert_path).unwrap().len(), 147);

        // device.enrolled emitted.
        let collected: Vec<_> = actions.drain().collect();
        let enrolled_emitted = collected.iter().any(|a| matches!(a,
            Action::Send { event_hash, .. } if *event_hash == s.device_enrolled_hash
        ));
        assert!(enrolled_emitted, "expected device.enrolled action");
    }
}
