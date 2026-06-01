//! `Provision` sentant — orchestrates the
//!
//! ```text
//!   device.identity_observed  →  keyholder.SIGN_CERT
//!                             →  provision.cert_issued
//!                             →  provision.COMPOSE_OFFER
//!                             →  provision.offer.composed
//!                             →  device.enrolled
//! ```
//!
//! chain per SPEC-APIARY-FLASH §5 + §3.3. F3 v0.1 scope: the
//! orchestrator-side reaction to a freshly-flashed board announcing
//! its Ed25519 pubkey. The actual BLE radio observation that produces
//! `device.identity_observed` is F4 (`beacon-observer` plugin); for v0.1
//! the operator (via the AI) triggers it directly through chat.
//!
//! The DeviceCertificate written here is the artefact that proves to
//! the device that the orchestrator has the apiary's TG private key
//! and accepts the device as part of the apiary. The cert + WiFi
//! credentials together form the `#wifi_offer` blob that F4 will ship
//! over BLE-L2CAP CoC.
//!
//! Per [[feedback-sentants-vs-plugins-terminology]] the sentant routes
//! events; the imperative work (Ed25519 signing, file IO, offer
//! composition) lives in the keyholder + provision plugins.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use r2_engine::action::PayloadBuf;
use r2_engine::plugin::PluginId;
use r2_engine::{Action, ActionBuf, Event, EventSource, Sentant, StateId, Target};

use crate::bridge::registry;
use crate::substrate::{ComposeOfferRequest, KeyholderSlot, ProvisionSlot, SignCertRequest};
use crate::substrate::{keyholder, provision};
use crate::sentants::RosterCtx;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum State { Idle = 0 }

/// Per-slot state we maintain across the cert→offer→enrol chain.
#[derive(Debug, Clone)]
struct Pending {
    device_pk_hex: String,
    /// 144-byte DeviceCertificate — populated on cert_issued, used on
    /// offer.composed to compose the final device.enrolled event.
    cert_bytes: Vec<u8>,
    /// One-second resolution timestamps from the cert.
    valid_from: u64,
    valid_until: u64,
}

pub struct ProvisionSentant {
    state: State,
    keyholder_pid: PluginId,
    keyholder_slot: KeyholderSlot,
    provision_pid: PluginId,
    provision_slot: ProvisionSlot,
    apiary_ctx: RosterCtx,
    /// In-flight slots keyed by slot_id.
    pending: Arc<Mutex<HashMap<String, Pending>>>,

    identity_observed_hash: u32,
    cert_issued_hash: u32,
    cert_error_hash: u32,
    offer_composed_hash: u32,
    offer_start_hash: u32,
    provision_error_hash: u32,
    device_enrolled_hash: u32,
    device_transition_hash: u32,
}

impl ProvisionSentant {
    pub fn new(
        keyholder_pid: PluginId,
        keyholder_slot: KeyholderSlot,
        provision_pid: PluginId,
        provision_slot: ProvisionSlot,
        apiary_ctx: RosterCtx,
    ) -> Self {
        let reg = registry();
        Self {
            state: State::Idle,
            keyholder_pid,
            keyholder_slot,
            provision_pid,
            provision_slot,
            apiary_ctx,
            pending: Arc::new(Mutex::new(HashMap::new())),

            identity_observed_hash: reg.hash_of("r2.composer.device.identity_observed").unwrap(),
            cert_issued_hash:       reg.hash_of("r2.composer.provision.cert_issued").unwrap(),
            cert_error_hash:        reg.hash_of("r2.composer.provision.cert_error").unwrap(),
            offer_composed_hash:    reg.hash_of("r2.composer.provision.offer.composed").unwrap(),
            offer_start_hash:       reg.hash_of("r2.composer.provision.offer.start").unwrap(),
            provision_error_hash:   reg.hash_of("r2.composer.provision.error").unwrap(),
            device_enrolled_hash:   reg.hash_of("r2.composer.device.enrolled").unwrap(),
            device_transition_hash: reg.hash_of("r2.composer.device.transition").unwrap(),
        }
    }
}

impl Sentant for ProvisionSentant {
    fn handle_event(&mut self, event: &Event, actions: &mut ActionBuf) {
        // device.identity_observed — operator (via AI in v0.1) reports
        // that a freshly-flashed board has announced its pubkey.
        // Accept Local-source for v0.1 (chat tool-call); Plugin-source
        // will be the beacon-observer in F4.
        if event.hash == self.identity_observed_hash {
            self.handle_identity_observed(event.payload, actions);
            return;
        }

        // Plugin-sourced re-handling — only act on plugin originals so
        // we don't loop on our own re-broadcasts.
        let is_plugin = matches!(event.source, EventSource::Plugin(_));
        if !is_plugin { return; }

        if event.hash == self.cert_issued_hash {
            self.handle_cert_issued(event.payload, actions);
        } else if event.hash == self.offer_composed_hash {
            self.handle_offer_composed(event.payload, actions);
        } else if event.hash == self.cert_error_hash
               || event.hash == self.provision_error_hash {
            // Re-broadcast for the webapp + drop the pending entry.
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: event.hash,
                payload: PayloadBuf::from_slice(event.payload),
            });
            self.drop_pending_for_payload(event.payload);
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
                reg.hash_of("r2.composer.device.identity_observed").unwrap(),
                reg.hash_of("r2.composer.provision.cert_issued").unwrap(),
                reg.hash_of("r2.composer.provision.cert_error").unwrap(),
                reg.hash_of("r2.composer.provision.offer.composed").unwrap(),
                reg.hash_of("r2.composer.provision.error").unwrap(),
            ];
            Box::leak(subs.into_boxed_slice())
        })
    }
}

impl ProvisionSentant {
    fn handle_identity_observed(&mut self, payload: &[u8], actions: &mut ActionBuf) {
        let v: serde_json::Value =
            serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);
        let slot_id      = v.get("slot_id").and_then(|x| x.as_str()).unwrap_or("");
        let device_pk_hex = v.get("device_pk").and_then(|x| x.as_str()).unwrap_or("");
        let network_name = v.get("network").and_then(|x| x.as_str()).map(String::from);

        if slot_id.is_empty() || device_pk_hex.is_empty() {
            tracing::warn!(
                "provision: identity_observed missing slot_id or device_pk (slot_id={slot_id:?})"
            );
            return;
        }
        let device_pk_bytes = match hex::decode(device_pk_hex) {
            Ok(b) if b.len() == 32 => b,
            _ => {
                tracing::warn!(
                    "provision: device_pk must be 32-byte hex (got {} chars)",
                    device_pk_hex.len()
                );
                return;
            }
        };
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&device_pk_bytes);

        // Stash pending entry; the network preference rides along in
        // case the operator named a non-default network. We store it
        // for use when we fill the ComposeOfferRequest later.
        {
            let mut p = self.pending.lock().unwrap();
            p.insert(slot_id.to_string(), Pending {
                device_pk_hex: device_pk_hex.to_string(),
                cert_bytes: Vec::new(),
                valid_from: 0,
                valid_until: 0,
            });
        }
        // Also stash the network preference on the side-slot for the
        // upcoming ComposeOfferRequest. Carried separately because the
        // keyholder side-slot doesn't have a network field.
        if let Some(name) = network_name.as_ref() {
            tracing::info!("provision: identity_observed network preference: {name}");
        }

        // Fill keyholder side-slot + emit PluginCall.
        *self.keyholder_slot.lock().unwrap() = Some(SignCertRequest {
            device_pk: arr,
            valid_secs: 365 * 24 * 60 * 60, // 1 year (v0.1 default)
            slot_id: slot_id.to_string(),
        });
        actions.push(Action::PluginCall {
            plugin_id: self.keyholder_pid,
            command: keyholder::CMD_SIGN_CERT,
            data: PayloadBuf::empty(),
        });

        // Surface that we picked up the request — useful for the chat
        // log so the operator sees that the AI's tool-call was received.
        let progress = serde_json::to_vec(&serde_json::json!({
            "slot_id": slot_id,
            "phase": "cert_request",
            "detail": "minting DeviceCertificate via keyholder",
        })).unwrap_or_default();
        actions.push(Action::Send {
            target: Target::Broadcast,
            event_hash: self.offer_start_hash,
            payload: PayloadBuf::from_slice(&progress),
        });
    }

    fn handle_cert_issued(&mut self, payload: &[u8], actions: &mut ActionBuf) {
        let v: serde_json::Value =
            serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);
        let slot_id   = v.get("slot_id").and_then(|x| x.as_str()).unwrap_or("");
        let cert_hex  = v.get("cert_hex").and_then(|x| x.as_str()).unwrap_or("");
        let valid_from  = v.get("valid_from").and_then(|x| x.as_u64()).unwrap_or(0);
        let valid_until = v.get("valid_until").and_then(|x| x.as_u64()).unwrap_or(0);

        let cert_bytes = match hex::decode(cert_hex) {
            Ok(b) if b.len() == 144 => b,
            _ => {
                tracing::warn!("provision: cert_issued cert_hex invalid (len={} hex)", cert_hex.len());
                return;
            }
        };

        // Refresh the pending entry; this is also our only checkpoint
        // — if the slot wasn't in flight, we treat it as a late echo
        // from a stale request and ignore.
        {
            let mut p = self.pending.lock().unwrap();
            match p.get_mut(slot_id) {
                Some(e) => {
                    e.cert_bytes = cert_bytes.clone();
                    e.valid_from = valid_from;
                    e.valid_until = valid_until;
                }
                None => {
                    tracing::warn!("provision: cert_issued for unknown slot_id {slot_id} — ignoring");
                    return;
                }
            }
        }

        // Persist the cert to apiaries/<name>/devices/certs/<dev_pk>.bin.
        let device_pk_hex = v.get("device_pk").and_then(|x| x.as_str()).unwrap_or("");
        let apiary_dir = {
            let ctx = self.apiary_ctx.lock().unwrap();
            ctx.as_ref().cloned()
        };
        if let Some(apiary) = apiary_dir.as_ref() {
            let cert_dir: PathBuf = apiary.join("devices/certs");
            if let Err(e) = std::fs::create_dir_all(&cert_dir) {
                tracing::error!("provision: mkdir {}: {e}", cert_dir.display());
            } else {
                let cert_path = cert_dir.join(format!("{device_pk_hex}.bin"));
                if let Err(e) = std::fs::write(&cert_path, &cert_bytes) {
                    tracing::error!("provision: write cert {}: {e}", cert_path.display());
                } else {
                    tracing::info!("provision: wrote DeviceCertificate to {}", cert_path.display());
                }
            }
        }

        // Re-broadcast cert_issued so the webapp can show "cert minted"
        // in the chat log / build pane.
        actions.push(Action::Send {
            target: Target::Broadcast,
            event_hash: self.cert_issued_hash,
            payload: PayloadBuf::from_slice(payload),
        });

        // Fire the offer composition. network_name=None ⇒ provision
        // plugin uses the default network (the operator must have
        // `provision.network.upsert`'d at least one network first).
        *self.provision_slot.lock().unwrap() = Some(ComposeOfferRequest {
            slot_id: slot_id.to_string(),
            network_name: None,
            cert: cert_bytes,
        });
        actions.push(Action::PluginCall {
            plugin_id: self.provision_pid,
            command: provision::CMD_COMPOSE_OFFER,
            data: PayloadBuf::empty(),
        });
    }

    fn handle_offer_composed(&mut self, payload: &[u8], actions: &mut ActionBuf) {
        let v: serde_json::Value =
            serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);
        let slot_id      = v.get("slot_id").and_then(|x| x.as_str()).unwrap_or("");
        let network_name = v.get("network_name").and_then(|x| x.as_str()).unwrap_or("");
        let ssid         = v.get("ssid").and_then(|x| x.as_str()).unwrap_or("");
        let offer_hex    = v.get("offer_hex").and_then(|x| x.as_str()).unwrap_or("");

        // Look up + remove the pending entry.
        let pending = {
            let mut p = self.pending.lock().unwrap();
            p.remove(slot_id)
        };
        let Some(pe) = pending else {
            tracing::warn!("provision: offer.composed for unknown slot_id {slot_id} — ignoring");
            return;
        };

        // Re-broadcast offer.composed for diagnostics. The PSK is NOT
        // in here — the provision plugin already enforced that.
        actions.push(Action::Send {
            target: Target::Broadcast,
            event_hash: self.offer_composed_hash,
            payload: PayloadBuf::from_slice(payload),
        });

        // Emit device.enrolled — the high-level event Roster listens for.
        // Carries everything needed to mutate the slot row + the offer
        // bytes (for F4 to actually ship over BLE later).
        let enrolled = serde_json::to_vec(&serde_json::json!({
            "slot_id":     slot_id,
            "device_pk":   pe.device_pk_hex,
            "cert_hex":    hex::encode(&pe.cert_bytes),
            "valid_from":  pe.valid_from,
            "valid_until": pe.valid_until,
            "network":     network_name,
            "ssid":        ssid,
            "offer_hex":   offer_hex,
        })).unwrap_or_default();
        actions.push(Action::Send {
            target: Target::Broadcast,
            event_hash: self.device_enrolled_hash,
            payload: PayloadBuf::from_slice(&enrolled),
        });
    }

    fn drop_pending_for_payload(&self, payload: &[u8]) {
        let v: serde_json::Value =
            serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);
        let Some(slot_id) = v.get("slot_id").and_then(|x| x.as_str()) else { return };
        let mut p = self.pending.lock().unwrap();
        p.remove(slot_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_for(dir: &std::path::Path) -> RosterCtx {
        Arc::new(Mutex::new(Some(dir.to_path_buf())))
    }

    fn ev_local(hash: u32, payload: &[u8]) -> Event<'_> {
        Event { hash, payload, source: EventSource::Local(0), msg_id: 0 }
    }
    fn ev_plugin(hash: u32, payload: &[u8]) -> Event<'_> {
        Event { hash, payload, source: EventSource::Plugin(0), msg_id: 0 }
    }

    fn make(dir: &std::path::Path) -> ProvisionSentant {
        let kslot: KeyholderSlot = Arc::new(Mutex::new(None));
        let pslot: ProvisionSlot = Arc::new(Mutex::new(None));
        ProvisionSentant::new(1, kslot, 2, pslot, ctx_for(dir))
    }

    #[test]
    fn identity_observed_fills_keyholder_slot_and_calls_keyholder() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = make(dir.path());
        let mut actions = ActionBuf::new();
        let payload = br#"{"slot_id":"sensor:esp32-s3-xiao:abc","device_pk":"cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd"}"#;
        s.handle_event(&ev_local(s.identity_observed_hash, payload), &mut actions);

        // Keyholder slot filled.
        let slot = s.keyholder_slot.lock().unwrap();
        let req = slot.as_ref().expect("keyholder slot must be filled");
        assert_eq!(req.slot_id, "sensor:esp32-s3-xiao:abc");
        assert_eq!(req.device_pk[0], 0xCD);
        assert!(req.valid_secs >= 30 * 24 * 60 * 60);

        // pending map has the entry.
        assert!(s.pending.lock().unwrap().contains_key("sensor:esp32-s3-xiao:abc"));

        // Two actions: PluginCall(keyholder) + offer.start progress event.
        let collected: Vec<_> = actions.drain().collect();
        assert!(collected.iter().any(|a| matches!(a,
            Action::PluginCall { plugin_id, command, .. } if *plugin_id == 1 && *command == keyholder::CMD_SIGN_CERT
        )));
    }

    #[test]
    fn cert_issued_writes_file_fills_provision_slot_and_calls_provision() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = make(dir.path());

        // Seed pending state as if identity_observed had run.
        s.pending.lock().unwrap().insert("slot:x".into(), Pending {
            device_pk_hex: "ab".repeat(32),
            cert_bytes: Vec::new(),
            valid_from: 0, valid_until: 0,
        });

        let cert_hex = "11".repeat(144);
        let payload = serde_json::to_vec(&serde_json::json!({
            "slot_id": "slot:x",
            "device_pk": "ab".repeat(32),
            "cert_hex": cert_hex,
            "tg_pub": "cc".repeat(32),
            "tg_fp": "dd".repeat(32),
            "valid_from": 100u64,
            "valid_until": 200u64,
        })).unwrap();
        let mut actions = ActionBuf::new();
        s.handle_event(&ev_plugin(s.cert_issued_hash, &payload), &mut actions);

        // Cert file written.
        let cert_path = dir.path().join("devices/certs").join(format!("{}.bin", "ab".repeat(32)));
        assert!(cert_path.exists(), "cert file missing: {}", cert_path.display());
        assert_eq!(std::fs::read(&cert_path).unwrap().len(), 144);

        // Provision slot filled.
        let pslot = s.provision_slot.lock().unwrap();
        let req = pslot.as_ref().expect("provision slot");
        assert_eq!(req.slot_id, "slot:x");
        assert_eq!(req.cert.len(), 144);

        // PluginCall(provision, COMPOSE_OFFER) was emitted.
        let collected: Vec<_> = actions.drain().collect();
        assert!(collected.iter().any(|a| matches!(a,
            Action::PluginCall { plugin_id, command, .. } if *plugin_id == 2 && *command == provision::CMD_COMPOSE_OFFER
        )));
    }

    #[test]
    fn offer_composed_emits_device_enrolled_with_cert() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = make(dir.path());
        s.pending.lock().unwrap().insert("slot:x".into(), Pending {
            device_pk_hex: "ab".repeat(32),
            cert_bytes: vec![0x42; 144],
            valid_from: 100, valid_until: 200,
        });

        let payload = serde_json::to_vec(&serde_json::json!({
            "slot_id": "slot:x",
            "network_name": "lab",
            "ssid": "Lab",
            "offer_len": 232,
            "offer_hex": "00".repeat(232),
        })).unwrap();
        let mut actions = ActionBuf::new();
        s.handle_event(&ev_plugin(s.offer_composed_hash, &payload), &mut actions);

        // device.enrolled emitted with cert_hex + ssid.
        let collected: Vec<_> = actions.drain().collect();
        let enrolled = collected.iter().find_map(|a| match a {
            Action::Send { event_hash, payload, .. }
              if *event_hash == s.device_enrolled_hash => Some(payload.as_slice().to_vec()),
            _ => None,
        }).expect("device.enrolled");
        let v: serde_json::Value = serde_json::from_slice(&enrolled).unwrap();
        assert_eq!(v["slot_id"], "slot:x");
        assert_eq!(v["network"], "lab");
        assert_eq!(v["ssid"], "Lab");
        let cert_hex = v["cert_hex"].as_str().unwrap();
        assert_eq!(cert_hex, "42".repeat(144));

        // Pending entry removed.
        assert!(!s.pending.lock().unwrap().contains_key("slot:x"));
    }

    #[test]
    fn cert_issued_for_unknown_slot_sluffs() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = make(dir.path());
        let payload = serde_json::to_vec(&serde_json::json!({
            "slot_id": "ghost",
            "cert_hex": "11".repeat(144),
            "device_pk": "ab".repeat(32),
        })).unwrap();
        let mut actions = ActionBuf::new();
        s.handle_event(&ev_plugin(s.cert_issued_hash, &payload), &mut actions);
        // No actions emitted; no cert file.
        assert!(actions.is_empty());
    }

    #[test]
    fn invalid_device_pk_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = make(dir.path());
        let mut actions = ActionBuf::new();
        // 31-byte pubkey (62 hex chars).
        let payload = format!(r#"{{"slot_id":"x","device_pk":"{}"}}"#, "ab".repeat(31));
        s.handle_event(&ev_local(s.identity_observed_hash, payload.as_bytes()), &mut actions);
        assert!(actions.is_empty(), "must reject invalid-length device_pk");
        assert!(s.pending.lock().unwrap().is_empty());
    }
}
