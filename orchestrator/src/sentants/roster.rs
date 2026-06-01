//! `Roster` sentant — per-device state machine for the active apiary.
//!
//! F1 scope (this file): handles `r2.composer.device.slot.create` +
//! `r2.composer.device.list` for the placeholder lifecycle phase.
//! Subsequent flash-workflow chunks (F2 USB first-install, F3 BLE
//! provisioning, F4 beacon-observer, F5 OTA, F6 decommission) add the
//! built / flashed_pending_pk / enrolled / reachable / etc. transitions
//! by extending this sentant's match arms.
//!
//! Per [[feedback-sentants-vs-plugins-terminology]] the sentant is a
//! thin FSM router; the imperative `roster.toml` IO lives in the
//! `roster` module.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use r2_engine::action::PayloadBuf;
use r2_engine::{Action, ActionBuf, Event, EventSource, Sentant, StateId, Target};

use crate::bridge::registry;
use crate::roster::{self, DeviceRow, Roster};

/// Shared roster context — the active apiary's directory path. Mutex
/// so the orchestrator's runtime apiary-open/close path (future) can
/// swap this without restarting the sentant. None means no apiary is
/// active; events are sluffed.
pub type RosterCtx = Arc<Mutex<Option<PathBuf>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum State {
    Idle = 0,
}

pub struct RosterSentant {
    state: State,
    ctx: RosterCtx,
    slot_create_hash: u32,
    list_hash: u32,
    entry_hash: u32,
    transition_hash: u32,
    first_install_done_hash: u32,
    device_enrolled_hash: u32,
}

impl RosterSentant {
    pub fn new(ctx: RosterCtx) -> Self {
        let reg = registry();
        Self {
            state: State::Idle,
            ctx,
            slot_create_hash: reg.hash_of("r2.composer.device.slot.create").unwrap(),
            list_hash:        reg.hash_of("r2.composer.device.list").unwrap(),
            entry_hash:       reg.hash_of("r2.composer.device.entry").unwrap(),
            transition_hash:  reg.hash_of("r2.composer.device.transition").unwrap(),
            first_install_done_hash: reg.hash_of("r2.composer.deploy.first_install.done").unwrap(),
            device_enrolled_hash:    reg.hash_of("r2.composer.device.enrolled").unwrap(),
        }
    }

    fn with_apiary<F>(&self, f: F)
    where
        F: FnOnce(&PathBuf),
    {
        let lock = self.ctx.lock().unwrap();
        if let Some(p) = lock.as_ref() {
            f(p);
        } else {
            tracing::warn!("roster: event received but no apiary open — sluffing");
        }
    }
}

impl Sentant for RosterSentant {
    fn handle_event(&mut self, event: &Event, actions: &mut ActionBuf) {
        // device.slot.create — operator (via AI) declares a new slot.
        if event.hash == self.slot_create_hash {
            self.handle_slot_create(event.payload, actions);
            return;
        }
        // device.list — webapp asks for the current roster snapshot.
        if event.hash == self.list_hash {
            self.handle_list(actions);
            return;
        }
        // deploy.first_install.done from the flasher plugin — drive the
        // slot through placeholder → built → flashed_pending_pk per
        // SPEC-APIARY-FLASH §2.2. Only act on Plugin-sourced events to
        // avoid re-handling Deploy sentant's own re-broadcast.
        if event.hash == self.first_install_done_hash
            && matches!(event.source, EventSource::Plugin(_))
        {
            self.handle_first_install_done(event.payload, actions);
            return;
        }
        // device.enrolled from ProvisionSentant — transition slot to
        // enrolled and stamp device_pk + cert_status. Sentant-sourced
        // (the Provision sentant synthesised it), so accept any source.
        if event.hash == self.device_enrolled_hash {
            self.handle_device_enrolled(event.payload, actions);
            return;
        }
    }

    fn state(&self) -> StateId { self.state as StateId }

    fn class_hash(&self) -> u32 {
        r2_fnv::fnv1a_32(b"ai.reality2.composer.sentant.roster")
    }

    fn name(&self) -> &str { "Roster" }

    fn subscriptions(&self) -> &[u32] {
        use std::sync::OnceLock;
        static SUBS: OnceLock<&'static [u32]> = OnceLock::new();
        SUBS.get_or_init(|| {
            let reg = registry();
            let subs = vec![
                reg.hash_of("r2.composer.device.slot.create").unwrap(),
                reg.hash_of("r2.composer.device.list").unwrap(),
                reg.hash_of("r2.composer.deploy.first_install.done").unwrap(),
                reg.hash_of("r2.composer.device.enrolled").unwrap(),
            ];
            Box::leak(subs.into_boxed_slice())
        })
    }
}

impl RosterSentant {
    fn handle_slot_create(&self, payload: &[u8], actions: &mut ActionBuf) {
        let v: serde_json::Value =
            serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);
        let role     = v.get("role").and_then(|x| x.as_str()).unwrap_or("");
        let ensemble = v.get("ensemble").and_then(|x| x.as_str()).unwrap_or("");
        let host     = v.get("host").and_then(|x| x.as_str()).unwrap_or("");
        let alias    = v.get("name_alias").and_then(|x| x.as_str()).unwrap_or("");

        if role.is_empty() || ensemble.is_empty() || host.is_empty() {
            tracing::warn!(
                "roster: device.slot.create with missing fields (role={role:?} ensemble={ensemble:?} host={host:?})"
            );
            return;
        }

        let mut new_row: Option<DeviceRow> = None;
        self.with_apiary(|apiary_dir| {
            let mut roster = roster::load(apiary_dir);
            let now = roster::now_iso8601();
            let row = roster::new_placeholder(role, ensemble, host, alias, &now);
            new_row = Some(row.clone());
            roster.devices.push(row);
            if let Err(e) = roster::save(apiary_dir, &roster) {
                tracing::error!("roster: save failed: {e}");
            }
        });

        // Emit the new row as a device.entry so the webapp picks it up
        // AND a device.transition tagged with the new state. Both flow
        // through bus → bridge → /r2.
        if let Some(row) = new_row {
            let entry_payload = serde_json::to_vec(&row).unwrap_or_default();
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.entry_hash,
                payload: PayloadBuf::from_slice(&entry_payload),
            });

            let transition_payload = serde_json::to_vec(&serde_json::json!({
                "slot_id": row.slot_id,
                "from": "",
                "to": "placeholder",
                "detail": "slot.created",
            })).unwrap_or_default();
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.transition_hash,
                payload: PayloadBuf::from_slice(&transition_payload),
            });
        }
    }

    fn handle_list(&self, actions: &mut ActionBuf) {
        self.with_apiary(|apiary_dir| {
            let roster = roster::load(apiary_dir);
            for row in &roster.devices {
                let payload = serde_json::to_vec(row).unwrap_or_default();
                actions.push(Action::Send {
                    target: Target::Broadcast,
                    event_hash: self.entry_hash,
                    payload: PayloadBuf::from_slice(&payload),
                });
            }
        });
    }

    /// Drive the slot through `placeholder → built → flashed_pending_pk`
    /// per SPEC-APIARY-FLASH §2.2. Two transitions in sequence (the
    /// state table forbids skipping `built`). Emits a `device.transition`
    /// for each so the webapp's slot-state chip updates.
    ///
    /// v0.1: the `built` state is synthetic — we accept the operator's
    /// `artefact_path` as evidence that an artefact exists. Once the
    /// compile flow lands (SPEC-APIARY-COMPOSE §6 fan-out), the
    /// `placeholder → built` transition will fire on `target.build.done`
    /// instead, and this handler will only do the `built →
    /// flashed_pending_pk` step.
    fn handle_first_install_done(&self, payload: &[u8], actions: &mut ActionBuf) {
        let v: serde_json::Value =
            serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);
        let slot_id = v.get("slot_id").and_then(|x| x.as_str()).unwrap_or("");
        let port    = v.get("port").and_then(|x| x.as_str()).unwrap_or("");
        if slot_id.is_empty() {
            tracing::warn!("roster: first_install.done without slot_id — cannot transition");
            return;
        }

        let mut new_state = String::new();
        let mut transitions: Vec<(String, String, String)> = Vec::new();   // (from, to, detail)

        self.with_apiary(|apiary_dir| {
            let mut r = roster::load(apiary_dir);
            let Some(row) = roster::find_mut(&mut r, slot_id) else {
                tracing::warn!("roster: first_install.done for unknown slot_id {slot_id}");
                return;
            };
            let now = roster::now_iso8601();

            // placeholder → built (synthetic for v0.1)
            if row.state == "placeholder" {
                let from = row.state.clone();
                if let Err(e) = roster::apply_transition(
                    row, "built", "built",
                    &format!("operator-supplied artefact via first_install.done (port {port})"),
                    &now,
                ) {
                    tracing::warn!("roster: {e}");
                    return;
                }
                transitions.push((from, "built".to_string(),
                    "operator-supplied artefact".to_string()));
            }

            // built → flashed_pending_pk
            if row.state == "built" {
                let from = row.state.clone();
                if let Err(e) = roster::apply_transition(
                    row, "flashed_pending_pk", "flashed_usb",
                    &format!("flashed via USB on {port}"),
                    &now,
                ) {
                    tracing::warn!("roster: {e}");
                    return;
                }
                transitions.push((from, "flashed_pending_pk".to_string(),
                    format!("flashed via USB on {port}")));
            }

            new_state = row.state.clone();
            if let Err(e) = roster::save(apiary_dir, &r) {
                tracing::error!("roster: save failed: {e}");
            }
        });

        // Emit one device.transition per applied step. The webapp
        // updates the slot state chip incrementally — the operator
        // sees placeholder → built → flashed_pending_pk in sequence.
        for (from, to, detail) in transitions {
            let payload = serde_json::to_vec(&serde_json::json!({
                "slot_id": slot_id,
                "from": from,
                "to": to,
                "detail": detail,
            })).unwrap_or_default();
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.transition_hash,
                payload: PayloadBuf::from_slice(&payload),
            });
        }

        // Also emit a device.entry so the webapp's deviceSlots Map
        // updates with any other field changes (last_seen, history,
        // etc.) that came along.
        if !new_state.is_empty() {
            self.with_apiary(|apiary_dir| {
                let r = roster::load(apiary_dir);
                if let Some(row) = r.devices.iter().find(|d| d.slot_id == slot_id) {
                    let payload = serde_json::to_vec(row).unwrap_or_default();
                    actions.push(Action::Send {
                        target: Target::Broadcast,
                        event_hash: self.entry_hash,
                        payload: PayloadBuf::from_slice(&payload),
                    });
                }
            });
        }
    }

    /// Transition slot `flashed_pending_pk → enrolled` on device.enrolled
    /// from the Provision sentant. Stamps `device_pk` + `cert_status` +
    /// `provision_state` per SPEC-APIARY-FLASH §2.2.
    fn handle_device_enrolled(&self, payload: &[u8], actions: &mut ActionBuf) {
        let v: serde_json::Value =
            serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);
        let slot_id   = v.get("slot_id").and_then(|x| x.as_str()).unwrap_or("");
        let device_pk = v.get("device_pk").and_then(|x| x.as_str()).unwrap_or("");
        if slot_id.is_empty() || device_pk.is_empty() {
            tracing::warn!("roster: device.enrolled missing slot_id or device_pk");
            return;
        }

        let mut applied_transition: Option<(String, String, String)> = None;
        self.with_apiary(|apiary_dir| {
            let mut r = roster::load(apiary_dir);
            let Some(row) = roster::find_mut(&mut r, slot_id) else {
                tracing::warn!("roster: device.enrolled for unknown slot_id {slot_id}");
                return;
            };
            if row.state != "flashed_pending_pk" {
                tracing::warn!(
                    "roster: device.enrolled for slot in state {:?} — expected flashed_pending_pk; ignoring",
                    row.state
                );
                return;
            }
            let now = roster::now_iso8601();
            let from = row.state.clone();
            if let Err(e) = roster::apply_transition(
                row, "enrolled", "enrolled_via_ble",
                &format!("cert issued for {device_pk}"),
                &now,
            ) {
                tracing::warn!("roster: enrolled transition failed: {e}");
                return;
            }
            row.device_pk = Some(device_pk.to_string());
            row.cert_status = "valid".to_string();
            row.provision_state = "valid".to_string();
            applied_transition = Some((from, "enrolled".to_string(),
                format!("cert issued for {device_pk}")));
            if let Err(e) = roster::save(apiary_dir, &r) {
                tracing::error!("roster: save failed: {e}");
            }
        });

        if let Some((from, to, detail)) = applied_transition {
            let payload = serde_json::to_vec(&serde_json::json!({
                "slot_id": slot_id,
                "from": from,
                "to": to,
                "detail": detail,
            })).unwrap_or_default();
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.transition_hash,
                payload: PayloadBuf::from_slice(&payload),
            });

            self.with_apiary(|apiary_dir| {
                let r = roster::load(apiary_dir);
                if let Some(row) = r.devices.iter().find(|d| d.slot_id == slot_id) {
                    let payload = serde_json::to_vec(row).unwrap_or_default();
                    actions.push(Action::Send {
                        target: Target::Broadcast,
                        event_hash: self.entry_hash,
                        payload: PayloadBuf::from_slice(&payload),
                    });
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use r2_engine::EventSource;

    fn ev(hash: u32, payload: &[u8]) -> Event<'_> {
        Event { hash, payload, source: EventSource::Local(0), msg_id: 0 }
    }

    fn ctx_for(dir: &std::path::Path) -> RosterCtx {
        Arc::new(Mutex::new(Some(dir.to_path_buf())))
    }

    #[test]
    fn slot_create_appends_row_and_emits_two_actions() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = RosterSentant::new(ctx_for(dir.path()));
        let mut actions = ActionBuf::new();
        let payload = br#"{"role":"sensor","ensemble":"rocker-sensor","host":"esp32-s3-xiao","name_alias":"kitchen"}"#;
        s.handle_event(&ev(s.slot_create_hash, payload), &mut actions);

        // Two emissions: device.entry + device.transition.
        let collected: Vec<_> = actions.drain().collect();
        assert_eq!(collected.len(), 2);
        match &collected[0] {
            Action::Send { event_hash, .. } => assert_eq!(*event_hash, s.entry_hash),
            _ => panic!("expected Send"),
        }
        match &collected[1] {
            Action::Send { event_hash, .. } => assert_eq!(*event_hash, s.transition_hash),
            _ => panic!("expected Send"),
        }

        // Row landed on disk in `placeholder` state.
        let roster = roster::load(dir.path());
        assert_eq!(roster.devices.len(), 1);
        assert_eq!(roster.devices[0].state, "placeholder");
        assert_eq!(roster.devices[0].name_alias, "kitchen");
    }

    #[test]
    fn list_emits_one_entry_per_row() {
        let dir = tempfile::tempdir().unwrap();
        // Pre-populate two slots.
        let mut roster = Roster::default();
        roster.devices.push(roster::new_placeholder("sensor", "rocker-sensor", "esp32-s3-xiao", "kitchen", "2026-06-01T00:00:00Z"));
        roster.devices.push(roster::new_placeholder("sensor", "rocker-sensor", "esp32-c6-dfr1117", "lounge", "2026-06-01T00:00:01Z"));
        roster::save(dir.path(), &roster).unwrap();

        let mut s = RosterSentant::new(ctx_for(dir.path()));
        let mut actions = ActionBuf::new();
        s.handle_event(&ev(s.list_hash, b"{}"), &mut actions);

        let collected: Vec<_> = actions.drain().collect();
        assert_eq!(collected.len(), 2);
        for a in &collected {
            match a {
                Action::Send { event_hash, .. } => assert_eq!(*event_hash, s.entry_hash),
                _ => panic!("expected Send"),
            }
        }
    }

    #[test]
    fn missing_fields_silently_sluffs() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = RosterSentant::new(ctx_for(dir.path()));
        let mut actions = ActionBuf::new();
        let payload = br#"{"role":"sensor"}"#;
        s.handle_event(&ev(s.slot_create_hash, payload), &mut actions);
        assert!(actions.is_empty());
        // No row added.
        assert_eq!(roster::load(dir.path()).devices.len(), 0);
    }

    #[test]
    fn no_apiary_open_sluffs() {
        let no_ctx: RosterCtx = Arc::new(Mutex::new(None));
        let mut s = RosterSentant::new(no_ctx);
        let mut actions = ActionBuf::new();
        let payload = br#"{"role":"sensor","ensemble":"rocker-sensor","host":"esp32-s3-xiao"}"#;
        s.handle_event(&ev(s.slot_create_hash, payload), &mut actions);
        assert!(actions.is_empty(), "no-apiary state must not emit");
    }

    #[test]
    fn first_install_done_drives_placeholder_to_flashed_pending_pk() {
        let dir = tempfile::tempdir().unwrap();
        // Seed a placeholder slot.
        let mut r = Roster::default();
        let row = roster::new_placeholder(
            "sensor", "rocker-sensor", "esp32-s3-xiao", "kitchen", "2026-06-01T00:00:00Z");
        let slot_id = row.slot_id.clone();
        r.devices.push(row);
        roster::save(dir.path(), &r).unwrap();

        let mut s = RosterSentant::new(ctx_for(dir.path()));
        let mut actions = ActionBuf::new();
        let payload = format!(
            r#"{{"slot_id":"{slot_id}","port":"/dev/ttyACM0","exit_code":0}}"#);
        s.handle_event(
            &Event {
                hash: s.first_install_done_hash,
                payload: payload.as_bytes(),
                source: EventSource::Plugin(0),
                msg_id: 0,
            },
            &mut actions,
        );

        // Two transition events + one entry refresh emitted.
        let collected: Vec<_> = actions.drain().collect();
        assert!(collected.len() >= 2, "expected ≥2 actions, got {}", collected.len());

        // Roster on disk: state = flashed_pending_pk, history has both transitions.
        let r = roster::load(dir.path());
        let row = r.devices.iter().find(|d| d.slot_id == slot_id).expect("row");
        assert_eq!(row.state, "flashed_pending_pk");
        // history: initial slot.created + built + flashed_usb = 3 entries.
        assert!(row.history.len() >= 3, "history should grow on each transition; got {}", row.history.len());
        let last = row.history.last().unwrap();
        assert_eq!(last.from, "built");
        assert_eq!(last.to, "flashed_pending_pk");
        assert!(last.detail.contains("/dev/ttyACM0"));
    }

    #[test]
    fn device_enrolled_drives_flashed_pending_to_enrolled() {
        // Seed roster with a slot in flashed_pending_pk.
        let dir = tempfile::tempdir().unwrap();
        let mut r = Roster::default();
        let mut row = roster::new_placeholder(
            "sensor", "rocker-sensor", "esp32-s3-xiao", "kitchen", "2026-06-01T00:00:00Z");
        row.state = "flashed_pending_pk".to_string();
        let slot_id = row.slot_id.clone();
        r.devices.push(row);
        roster::save(dir.path(), &r).unwrap();

        let mut s = RosterSentant::new(ctx_for(dir.path()));
        let mut actions = ActionBuf::new();
        let device_pk = "ab".repeat(32);
        let payload = format!(
            r#"{{"slot_id":"{slot_id}","device_pk":"{device_pk}","cert_hex":"{}","network":"lab","ssid":"Lab","offer_hex":""}}"#,
            "11".repeat(144),
        );
        s.handle_event(
            &Event {
                hash: s.device_enrolled_hash,
                payload: payload.as_bytes(),
                source: EventSource::Local(0),
                msg_id: 0,
            },
            &mut actions,
        );

        // Row mutated.
        let r = roster::load(dir.path());
        let row = r.devices.iter().find(|d| d.slot_id == slot_id).expect("row");
        assert_eq!(row.state, "enrolled");
        assert_eq!(row.device_pk.as_deref(), Some(device_pk.as_str()));
        assert_eq!(row.cert_status, "valid");
        assert_eq!(row.provision_state, "valid");

        // device.transition + device.entry emitted.
        let collected: Vec<_> = actions.drain().collect();
        let has_transition = collected.iter().any(|a| matches!(a,
            Action::Send { event_hash, .. } if *event_hash == s.transition_hash));
        let has_entry = collected.iter().any(|a| matches!(a,
            Action::Send { event_hash, .. } if *event_hash == s.entry_hash));
        assert!(has_transition, "expected device.transition");
        assert!(has_entry, "expected device.entry");
    }

    #[test]
    fn device_enrolled_for_wrong_state_sluffs() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = Roster::default();
        let row = roster::new_placeholder(
            "sensor", "rocker-sensor", "esp32-s3-xiao", "k", "2026-06-01T00:00:00Z");
        let slot_id = row.slot_id.clone();
        // Stays in `placeholder`, not flashed_pending_pk.
        r.devices.push(row);
        roster::save(dir.path(), &r).unwrap();

        let mut s = RosterSentant::new(ctx_for(dir.path()));
        let mut actions = ActionBuf::new();
        let payload = format!(
            r#"{{"slot_id":"{slot_id}","device_pk":"{}","cert_hex":""}}"#,
            "ab".repeat(32),
        );
        s.handle_event(
            &Event {
                hash: s.device_enrolled_hash,
                payload: payload.as_bytes(),
                source: EventSource::Local(0),
                msg_id: 0,
            },
            &mut actions,
        );
        assert!(actions.is_empty(), "must not transition from placeholder");
        let r = roster::load(dir.path());
        assert_eq!(r.devices[0].state, "placeholder");
    }

    #[test]
    fn first_install_done_from_local_source_ignored() {
        // The Deploy sentant re-broadcasts first_install.done with
        // EventSource::Sentant. Roster must NOT also handle it
        // (would double-mutate the row).
        let dir = tempfile::tempdir().unwrap();
        let mut r = Roster::default();
        let row = roster::new_placeholder(
            "sensor", "rocker-sensor", "esp32-s3-xiao", "kitchen", "2026-06-01T00:00:00Z");
        let slot_id = row.slot_id.clone();
        r.devices.push(row);
        roster::save(dir.path(), &r).unwrap();

        let mut s = RosterSentant::new(ctx_for(dir.path()));
        let mut actions = ActionBuf::new();
        let payload = format!(r#"{{"slot_id":"{slot_id}","port":"/dev/x"}}"#);
        s.handle_event(
            &Event {
                hash: s.first_install_done_hash,
                payload: payload.as_bytes(),
                source: EventSource::Local(0),  // NOT a Plugin source
                msg_id: 0,
            },
            &mut actions,
        );
        assert!(actions.is_empty(), "local-source first_install.done must not trigger Roster");
        // Row state unchanged.
        let r = roster::load(dir.path());
        assert_eq!(r.devices[0].state, "placeholder");
    }
}
