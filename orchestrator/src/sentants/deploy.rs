//! `Deploy` sentant — orchestrates first-install + OTA flows.
//!
//! F2 scope (this file): handles `r2.composer.deploy.first_install.start`
//! events. Reads the slot row from `roster.toml`, resolves the carrier's
//! `[usb]` table from `catalogue/boards/<host>/board.toml`, populates the
//! flasher's FlashParams slot, dispatches `CMD_START` to the flasher
//! plugin. Re-broadcasts the plugin's progress/done/error events so the
//! webapp's build console picks them up.
//!
//! On `first_install.done`, also re-broadcasts an internal-shaped
//! event the Roster sentant listens for to transition the slot's
//! state (placeholder → built → flashed_pending_pk).
//!
//! Future chunks (F5 OTA, F6 decommission) add `deploy.batch.*` handling
//! here; same pattern, different plugin.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use r2_engine::action::PayloadBuf;
use r2_engine::plugin::PluginId;
use r2_engine::{Action, ActionBuf, Event, EventSource, Sentant, StateId, Target};

use crate::bridge::registry;
use crate::plugins::{flasher, FlashParams, FlashRegion, FlasherSlot};
use crate::roster;
use crate::sentants::RosterCtx;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum State { Idle = 0, Flashing = 1 }

pub struct DeploySentant {
    state: State,
    flasher_plugin_id: PluginId,
    flasher_slot: FlasherSlot,
    /// Repo root — used to resolve `catalogue/boards/<host>/board.toml`.
    repo_root: Arc<Mutex<PathBuf>>,
    /// Shared with Roster — used to resolve the slot row from roster.toml.
    apiary_ctx: RosterCtx,

    first_install_start_hash: u32,
    first_install_progress_hash: u32,
    first_install_done_hash: u32,
    first_install_error_hash: u32,
}

impl DeploySentant {
    pub fn new(
        flasher_plugin_id: PluginId,
        flasher_slot: FlasherSlot,
        repo_root: PathBuf,
        apiary_ctx: RosterCtx,
    ) -> Self {
        let reg = registry();
        Self {
            state: State::Idle,
            flasher_plugin_id,
            flasher_slot,
            repo_root: Arc::new(Mutex::new(repo_root)),
            apiary_ctx,
            first_install_start_hash:    reg.hash_of("r2.composer.deploy.first_install.start").unwrap(),
            first_install_progress_hash: reg.hash_of("r2.composer.deploy.first_install.progress").unwrap(),
            first_install_done_hash:     reg.hash_of("r2.composer.deploy.first_install.done").unwrap(),
            first_install_error_hash:    reg.hash_of("r2.composer.deploy.first_install.error").unwrap(),
        }
    }
}

impl Sentant for DeploySentant {
    fn handle_event(&mut self, event: &Event, actions: &mut ActionBuf) {
        // deploy.first_install.start  →  resolve + dispatch
        if event.hash == self.first_install_start_hash {
            self.handle_start(event.payload, actions);
            return;
        }

        // Plugin-sourced progress / done / error → re-broadcast so the
        // WS layer's outbound queue picks them up. Same guard pattern
        // as Builder + Author.
        let is_plugin_source = matches!(event.source, EventSource::Plugin(_));
        if !is_plugin_source { return; }

        if event.hash == self.first_install_progress_hash {
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.first_install_progress_hash,
                payload: PayloadBuf::from_slice(event.payload),
            });
        } else if event.hash == self.first_install_done_hash {
            self.state = State::Idle;
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.first_install_done_hash,
                payload: PayloadBuf::from_slice(event.payload),
            });
            // Also drive the slot through built → flashed_pending_pk
            // by reading the original start payload's slot_id from the
            // current done payload (the flasher embedded the port; we
            // need slot_id too — for v0.1 we infer via the active
            // apiary's most-recent placeholder/built slot whose host
            // matches the port. Cleaner: thread slot_id through the
            // flasher pipeline. Tracked as a follow-up.
            self.apply_post_flash_transition(event.payload);
        } else if event.hash == self.first_install_error_hash {
            self.state = State::Idle;
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.first_install_error_hash,
                payload: PayloadBuf::from_slice(event.payload),
            });
        }
    }

    fn state(&self) -> StateId { self.state as StateId }

    fn class_hash(&self) -> u32 {
        r2_fnv::fnv1a_32(b"ai.reality2.composer.sentant.deploy")
    }

    fn name(&self) -> &str { "Deploy" }

    fn subscriptions(&self) -> &[u32] {
        use std::sync::OnceLock;
        static SUBS: OnceLock<&'static [u32]> = OnceLock::new();
        SUBS.get_or_init(|| {
            let reg = registry();
            let subs = vec![
                reg.hash_of("r2.composer.deploy.first_install.start").unwrap(),
                reg.hash_of("r2.composer.deploy.first_install.progress").unwrap(),
                reg.hash_of("r2.composer.deploy.first_install.done").unwrap(),
                reg.hash_of("r2.composer.deploy.first_install.error").unwrap(),
            ];
            Box::leak(subs.into_boxed_slice())
        })
    }
}

impl DeploySentant {
    /// Process deploy.first_install.start. Resolves slot → carrier →
    /// usb config, builds FlashParams, dispatches PluginCall.
    ///
    /// Expected payload (per SPEC-APIARY-FLASH §8.1):
    ///   { slot_id, port, carrier?, role?, artefact_path? }
    /// We accept either `slot_id` + `port` (look the rest up in
    /// roster.toml + board.toml) OR a full payload from a power-user.
    fn handle_start(&mut self, payload: &[u8], actions: &mut ActionBuf) {
        let v: serde_json::Value =
            serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);
        let slot_id = v.get("slot_id").and_then(|x| x.as_str()).unwrap_or("");
        let port    = v.get("port").and_then(|x| x.as_str()).unwrap_or("");
        let artefact = v.get("artefact_path").and_then(|x| x.as_str()).map(PathBuf::from);

        if slot_id.is_empty() || port.is_empty() {
            self.emit_error(actions, slot_id, port,
                "first_install.start missing slot_id or port");
            return;
        }

        // Resolve the slot's host from roster.toml.
        let host: String = {
            let ctx = self.apiary_ctx.lock().unwrap();
            let Some(apiary_dir) = ctx.as_ref() else {
                self.emit_error(actions, slot_id, port,
                    "no apiary open — first_install requires an active apiary");
                return;
            };
            let roster = roster::load(apiary_dir);
            match roster.devices.iter().find(|d| d.slot_id == slot_id) {
                Some(row) => row.host.clone(),
                None => {
                    self.emit_error(actions, slot_id, port,
                        &format!("E_SLOT_NOT_FOUND: {slot_id}"));
                    return;
                }
            }
        };

        // Resolve the carrier's [usb] config from board.toml.
        let repo_root = self.repo_root.lock().unwrap().clone();
        let board_toml = repo_root.join("catalogue/boards").join(&host).join("board.toml");
        let usb_cfg = match read_usb_config(&board_toml) {
            Ok(cfg) => cfg,
            Err(e) => {
                self.emit_error(actions, slot_id, port,
                    &format!("E_USB_CONFIG: {} ({e})", board_toml.display()));
                return;
            }
        };

        // Build FlashParams. v0.1 carries a single firmware.bin region
        // at ota_0 IF the operator gave us an artefact path. Future
        // chunks add bootloader.bin + partition-table.bin once the
        // build flow produces all three (SPEC-APIARY-COMPOSE §6.3
        // amendment per SPEC-APIARY-FLASH §4.3).
        let mut regions: Vec<FlashRegion> = Vec::new();
        if let Some(art) = artefact.as_ref() {
            regions.push(FlashRegion {
                offset: usb_cfg.ota_0_offset.clone(),
                path: art.clone(),
            });
        }

        let params = FlashParams {
            slot_id: slot_id.to_string(),
            port: port.to_string(),
            chip_arg: usb_cfg.esptool_chip_arg.clone(),
            baud: 460800,
            regions,
            firmware_sha256: None,
        };
        *self.flasher_slot.lock().unwrap() = Some(params);

        self.state = State::Flashing;
        actions.push(Action::PluginCall {
            plugin_id: self.flasher_plugin_id,
            command: flasher::CMD_START,
            data: PayloadBuf::empty(),
        });
    }

    fn emit_error(&self, actions: &mut ActionBuf, slot_id: &str, port: &str, message: &str) {
        let payload = serde_json::to_vec(&serde_json::json!({
            "slot_id": slot_id,
            "port": port,
            "phase": "resolving",
            "message": message,
        })).unwrap_or_default();
        actions.push(Action::Send {
            target: Target::Broadcast,
            event_hash: self.first_install_error_hash,
            payload: PayloadBuf::from_slice(&payload),
        });
    }

    /// Apply the post-flash state transition. v0.1 finds the matching
    /// placeholder/built slot by port-side heuristics; future versions
    /// will receive the slot_id back through the flasher payload (it's
    /// already in FlashParams; just needs to be plumbed through).
    fn apply_post_flash_transition(&self, _done_payload: &[u8]) {
        // F2 v0.1 stub. The Roster sentant subscribes to first_install.done
        // separately (when wired in F2b) and handles the transition.
        // Leaving this as a no-op for now means the slot's state stays
        // at `placeholder` post-flash — operator sees the flash succeed
        // but the slot card doesn't reflect it yet. Tracked as a known
        // gap; closing it requires threading slot_id through the
        // FlashParams → flasher Done → here pipeline.
    }
}

/// Resolved subset of a board's `[usb]` table — just the fields F2 needs.
struct UsbConfig {
    esptool_chip_arg: String,
    ota_0_offset: String,
}

fn read_usb_config(board_toml: &std::path::Path) -> Result<UsbConfig, String> {
    let raw = std::fs::read_to_string(board_toml).map_err(|e| format!("read: {e}"))?;
    let v: toml::Value = raw.parse().map_err(|e: toml::de::Error| format!("parse: {e}"))?;
    let usb = v.get("usb").ok_or("missing [usb] table")?;
    let esptool_chip_arg = usb.get("esptool_chip_arg")
        .and_then(|x| x.as_str())
        .ok_or("[usb].esptool_chip_arg missing or not a string")?
        .to_string();
    // flash_offsets is an inline table — pull ota_0.
    let ota_0_offset = usb.get("flash_offsets")
        .and_then(|fo| fo.get("ota_0"))
        .and_then(|x| x.as_integer())
        .map(|i| format!("0x{:x}", i))
        .ok_or("[usb].flash_offsets.ota_0 missing or not an integer")?;
    Ok(UsbConfig { esptool_chip_arg, ota_0_offset })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn ev(hash: u32, payload: &[u8], source: EventSource) -> Event<'_> {
        Event { hash, payload, source, msg_id: 0 }
    }

    fn make_sentant(apiary_dir: PathBuf, repo_root: PathBuf) -> (DeploySentant, FlasherSlot) {
        let slot: FlasherSlot = Arc::new(Mutex::new(None));
        let apiary_ctx: RosterCtx = Arc::new(Mutex::new(Some(apiary_dir)));
        let sentant = DeploySentant::new(99, slot.clone(), repo_root, apiary_ctx);
        (sentant, slot)
    }

    fn write_minimal_board(repo_root: &std::path::Path, slug: &str) {
        let dir = repo_root.join("catalogue/boards").join(slug);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("board.toml"), r#"
[board]
name = "minimal"
arch = "esp32"
chip = "esp32s3"
carrier = "minimal"
version = "0.0.1"
description = "test"

[usb]
vid = 0x303a
pid = 0x1001
chip_id_probe = "esp32s3"
esptool_chip_arg = "esp32s3"
identify_strategy = "vid_pid_then_chip_id"
reset_strategy = "usb_serial_jtag"
flash_offsets = { bootloader = 0x0, partition_table = 0x8000, ota_0 = 0x20000 }
"#).unwrap();
    }

    #[test]
    fn start_resolves_slot_and_fills_flasher_slot() {
        let repo = tempfile::tempdir().unwrap();
        let apiary = tempfile::tempdir().unwrap();
        write_minimal_board(repo.path(), "esp32-s3-test");

        // Seed the roster with one placeholder slot.
        let mut roster_data = roster::Roster::default();
        let row = roster::new_placeholder(
            "sensor", "rocker-sensor", "esp32-s3-test", "kitchen", "2026-06-01T00:00:00Z");
        let slot_id = row.slot_id.clone();
        roster_data.devices.push(row);
        roster::save(apiary.path(), &roster_data).unwrap();

        let (mut s, flasher_slot) = make_sentant(apiary.path().to_path_buf(), repo.path().to_path_buf());
        let mut actions = ActionBuf::new();
        let payload = format!(r#"{{"slot_id":"{slot_id}","port":"/dev/ttyACM0","artefact_path":"/tmp/firmware.bin"}}"#);
        s.handle_event(&ev(s.first_install_start_hash, payload.as_bytes(), EventSource::Local(0)), &mut actions);

        // One PluginCall action emitted.
        let collected: Vec<_> = actions.drain().collect();
        assert_eq!(collected.len(), 1);
        match &collected[0] {
            Action::PluginCall { plugin_id, command, .. } => {
                assert_eq!(*plugin_id, 99);
                assert_eq!(*command, flasher::CMD_START);
            }
            other => panic!("expected PluginCall, got {other:?}"),
        }
        assert_eq!(s.state, State::Flashing);

        // FlasherSlot was filled correctly.
        let params = flasher_slot.lock().unwrap().clone().expect("params");
        assert_eq!(params.port, "/dev/ttyACM0");
        assert_eq!(params.chip_arg, "esp32s3");
        assert_eq!(params.regions.len(), 1);
        assert_eq!(params.regions[0].offset, "0x20000");
        assert_eq!(params.regions[0].path.display().to_string(), "/tmp/firmware.bin");
    }

    #[test]
    fn missing_slot_id_emits_error() {
        let repo = tempfile::tempdir().unwrap();
        let apiary = tempfile::tempdir().unwrap();
        let (mut s, _) = make_sentant(apiary.path().to_path_buf(), repo.path().to_path_buf());
        let mut actions = ActionBuf::new();
        s.handle_event(&ev(s.first_install_start_hash, b"{}", EventSource::Local(0)), &mut actions);

        let collected: Vec<_> = actions.drain().collect();
        assert_eq!(collected.len(), 1);
        match &collected[0] {
            Action::Send { event_hash, .. } => assert_eq!(*event_hash, s.first_install_error_hash),
            _ => panic!(),
        }
    }

    #[test]
    fn unknown_slot_emits_error() {
        let repo = tempfile::tempdir().unwrap();
        let apiary = tempfile::tempdir().unwrap();
        let (mut s, _) = make_sentant(apiary.path().to_path_buf(), repo.path().to_path_buf());
        let mut actions = ActionBuf::new();
        let payload = br#"{"slot_id":"nonexistent","port":"/dev/ttyACM0"}"#;
        s.handle_event(&ev(s.first_install_start_hash, payload, EventSource::Local(0)), &mut actions);

        let collected: Vec<_> = actions.drain().collect();
        assert_eq!(collected.len(), 1);
        match &collected[0] {
            Action::Send { event_hash, payload, .. } => {
                assert_eq!(*event_hash, s.first_install_error_hash);
                let v: serde_json::Value = serde_json::from_slice(payload.as_slice()).unwrap();
                assert!(v["message"].as_str().unwrap().contains("E_SLOT_NOT_FOUND"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn plugin_sourced_progress_rebroadcasts() {
        let repo = tempfile::tempdir().unwrap();
        let apiary = tempfile::tempdir().unwrap();
        let (mut s, _) = make_sentant(apiary.path().to_path_buf(), repo.path().to_path_buf());
        let mut actions = ActionBuf::new();
        s.handle_event(
            &ev(s.first_install_progress_hash, br#"{"phase":"writing","line":"x"}"#, EventSource::Plugin(0)),
            &mut actions,
        );
        let collected: Vec<_> = actions.drain().collect();
        assert_eq!(collected.len(), 1);
        match &collected[0] {
            Action::Send { event_hash, target, .. } => {
                assert_eq!(*event_hash, s.first_install_progress_hash);
                assert!(matches!(target, Target::Broadcast));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn locally_sourced_progress_does_not_rebroadcast() {
        let repo = tempfile::tempdir().unwrap();
        let apiary = tempfile::tempdir().unwrap();
        let (mut s, _) = make_sentant(apiary.path().to_path_buf(), repo.path().to_path_buf());
        let mut actions = ActionBuf::new();
        s.handle_event(
            &ev(s.first_install_progress_hash, b"{}", EventSource::Local(0)),
            &mut actions,
        );
        assert!(actions.is_empty(), "must not re-broadcast local emissions");
    }
}
