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
use r2_engine::{Action, ActionBuf, Event, Sentant, StateId, Target};

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
}
