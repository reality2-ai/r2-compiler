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
//! ## F5 — OTA batch push
//!
//! `deploy.batch.start` drives the `ota_push` substrate (wire-v1 TCP
//! push, R2-UPDATE §3.1.2.2). Per SPEC-APIARY-FLASH §6.3 pushes run
//! **sequentially** by default — the sentant holds a queue and starts
//! the next device only when the current one's `deploy.device.done` /
//! `.error` arrives, then emits `deploy.batch.done` when the queue
//! drains. The §6.1 reachability gate refuses any slot whose roster
//! row is not `state: "reachable"` (immediate `deploy.device.error`
//! with `error_kind: "unreachable"`).
//!
//! Parallel pushes (§6.3 OPTIONAL) are NOT yet supported — the single
//! `ota_push` plugin instance serialises pushes, so F5 always runs
//! sequentially regardless of the payload's `parallel` flag.
//!
//! F6 (decommission) adds `device.revoke`/`retire`/`purge` handling.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use r2_engine::action::PayloadBuf;
use r2_engine::plugin::PluginId;
use r2_engine::{Action, ActionBuf, Event, EventSource, Sentant, StateId, Target};

use crate::bridge::registry;
use crate::composer::{flasher, FlashParams, FlashRegion, FlasherSlot};
use crate::roster;
use crate::sentants::RosterCtx;
use crate::substrate::ota_push::{self, OtaPushParams, OtaPushSlot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum State { Idle = 0, Flashing = 1, Pushing = 2 }

/// In-flight OTA batch — drives the sequential push queue.
struct BatchState {
    batch_id: String,
    artefact_path: PathBuf,
    log_path: Option<PathBuf>,
    /// Remaining (slot_id, target_ip, port) to push, in order.
    queue: VecDeque<(String, String, u16)>,
    ok_count: u32,
    error_count: u32,
}

pub struct DeploySentant {
    state: State,
    flasher_plugin_id: PluginId,
    flasher_slot: FlasherSlot,
    ota_push_plugin_id: PluginId,
    ota_push_slot: OtaPushSlot,
    /// Repo root — used to resolve `catalogue/boards/<host>/board.toml`.
    repo_root: Arc<Mutex<PathBuf>>,
    /// Shared with Roster — used to resolve the slot row from roster.toml.
    apiary_ctx: RosterCtx,
    /// Active OTA batch, if any.
    batch: Option<BatchState>,

    first_install_start_hash: u32,
    first_install_progress_hash: u32,
    first_install_done_hash: u32,
    first_install_error_hash: u32,

    batch_start_hash: u32,
    batch_done_hash: u32,
    device_progress_hash: u32,
    device_done_hash: u32,
    device_error_hash: u32,
}

impl DeploySentant {
    pub fn new(
        flasher_plugin_id: PluginId,
        flasher_slot: FlasherSlot,
        ota_push_plugin_id: PluginId,
        ota_push_slot: OtaPushSlot,
        repo_root: PathBuf,
        apiary_ctx: RosterCtx,
    ) -> Self {
        let reg = registry();
        Self {
            state: State::Idle,
            flasher_plugin_id,
            flasher_slot,
            ota_push_plugin_id,
            ota_push_slot,
            repo_root: Arc::new(Mutex::new(repo_root)),
            apiary_ctx,
            batch: None,
            first_install_start_hash:    reg.hash_of("r2.composer.deploy.first_install.start").unwrap(),
            first_install_progress_hash: reg.hash_of("r2.composer.deploy.first_install.progress").unwrap(),
            first_install_done_hash:     reg.hash_of("r2.composer.deploy.first_install.done").unwrap(),
            first_install_error_hash:    reg.hash_of("r2.composer.deploy.first_install.error").unwrap(),
            batch_start_hash:    reg.hash_of("r2.composer.deploy.batch.start").unwrap(),
            batch_done_hash:     reg.hash_of("r2.composer.deploy.batch.done").unwrap(),
            device_progress_hash: reg.hash_of("r2.composer.deploy.device.progress").unwrap(),
            device_done_hash:     reg.hash_of("r2.composer.deploy.device.done").unwrap(),
            device_error_hash:    reg.hash_of("r2.composer.deploy.device.error").unwrap(),
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

        // deploy.batch.start  →  reachability-gate + sequential push
        if event.hash == self.batch_start_hash {
            self.handle_batch_start(event.payload, actions);
            return;
        }

        // Plugin-sourced progress / done / error → re-broadcast so the
        // WS layer's outbound queue picks them up. Same guard pattern
        // as Builder + Author.
        let is_plugin_source = matches!(event.source, EventSource::Plugin(_));
        if !is_plugin_source { return; }

        // ── OTA per-device events from the ota_push plugin ──
        if event.hash == self.device_progress_hash {
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.device_progress_hash,
                payload: PayloadBuf::from_slice(event.payload),
            });
            return;
        }
        if event.hash == self.device_done_hash {
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.device_done_hash,
                payload: PayloadBuf::from_slice(event.payload),
            });
            if let Some(b) = self.batch.as_mut() { b.ok_count += 1; }
            self.advance_batch(actions);
            return;
        }
        if event.hash == self.device_error_hash {
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.device_error_hash,
                payload: PayloadBuf::from_slice(event.payload),
            });
            if let Some(b) = self.batch.as_mut() { b.error_count += 1; }
            self.advance_batch(actions);
            return;
        }

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
                reg.hash_of("r2.composer.deploy.batch.start").unwrap(),
                reg.hash_of("r2.composer.deploy.device.progress").unwrap(),
                reg.hash_of("r2.composer.deploy.device.done").unwrap(),
                reg.hash_of("r2.composer.deploy.device.error").unwrap(),
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

    /// Process `deploy.batch.start`. Per SPEC-APIARY-FLASH §6.1/§6.3:
    /// reachability-gate each target, then push the survivors
    /// sequentially.
    ///
    /// Accepted payload (F5 — extends §8.1's `{target_id, slot_ids[],
    /// parallel?}` with the fields v0.1 needs to actually push):
    ///   {
    ///     batch_id?: string,
    ///     artefact_path: string,           // abs path to firmware.bin
    ///     targets: [{ slot_id, ip, port? }], // per-device IP (see note
    ///                                      // on OtaPushParams.target_ip);
    ///                                      // port defaults to 21043
    ///     parallel?: bool                  // accepted but ignored (F5
    ///                                      // is always sequential)
    ///   }
    fn handle_batch_start(&mut self, payload: &[u8], actions: &mut ActionBuf) {
        let v: serde_json::Value =
            serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);

        let batch_id = v.get("batch_id")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(synth_batch_id);

        let artefact_path = match v.get("artefact_path").and_then(|x| x.as_str()) {
            Some(p) => PathBuf::from(p),
            None => {
                // No artefact → nothing to push. Surface one batch.done
                // with everything counted as an error so the operator
                // sees the failure rather than silence.
                let n = v.get("targets").and_then(|t| t.as_array()).map(|a| a.len()).unwrap_or(0) as u32;
                self.emit_batch_done(actions, &batch_id, 0, n.max(1));
                return;
            }
        };

        // Resolve the apiary's deploy_log.jsonl for the §6.5 audit trail.
        let log_path = {
            let ctx = self.apiary_ctx.lock().unwrap();
            ctx.as_ref().map(|dir| dir.join("devices/deploy_log.jsonl"))
        };

        // Snapshot the roster once for the reachability gate.
        let roster = {
            let ctx = self.apiary_ctx.lock().unwrap();
            ctx.as_ref().map(|dir| roster::load(dir))
        };

        let targets = v.get("targets").and_then(|t| t.as_array()).cloned().unwrap_or_default();

        let mut queue: VecDeque<(String, String, u16)> = VecDeque::new();
        let mut error_count: u32 = 0;

        for t in &targets {
            let slot_id = t.get("slot_id").and_then(|x| x.as_str()).unwrap_or("");
            let ip = t.get("ip").and_then(|x| x.as_str()).unwrap_or("");
            let port = t.get("port").and_then(|x| x.as_u64()).unwrap_or(21043) as u16;
            if slot_id.is_empty() {
                continue;
            }

            // §6.1 reachability gate: only `state: "reachable"` rows are
            // pushable. A missing roster row, a non-reachable state, or
            // an absent IP all refuse with E_OTA_UNREACHABLE.
            let reachable = roster.as_ref()
                .and_then(|r| r.devices.iter().find(|d| d.slot_id == slot_id))
                .map(|row| row.state == "reachable")
                .unwrap_or(false);

            if !reachable || ip.is_empty() {
                let reason = if ip.is_empty() {
                    "E_OTA_UNREACHABLE: no IP known for slot"
                } else {
                    "E_OTA_UNREACHABLE: device state is not reachable"
                };
                self.emit_device_error(actions, &batch_id, slot_id, "unreachable", reason);
                error_count += 1;
                continue;
            }

            queue.push_back((slot_id.to_string(), ip.to_string(), port));
        }

        if queue.is_empty() {
            // Everything was gated out — close the batch immediately.
            self.emit_batch_done(actions, &batch_id, 0, error_count);
            return;
        }

        self.batch = Some(BatchState {
            batch_id,
            artefact_path,
            log_path,
            queue,
            ok_count: 0,
            error_count,
        });
        self.advance_batch(actions);
    }

    /// Pop the next queued target and fire its push, or — if the queue
    /// is drained — emit `deploy.batch.done` and return to Idle.
    fn advance_batch(&mut self, actions: &mut ActionBuf) {
        let Some(batch) = self.batch.as_mut() else {
            self.state = State::Idle;
            return;
        };

        let Some((slot_id, ip, port)) = batch.queue.pop_front() else {
            // Done — snapshot counts, clear, emit.
            let (batch_id, ok, err) =
                (batch.batch_id.clone(), batch.ok_count, batch.error_count);
            self.batch = None;
            self.state = State::Idle;
            self.emit_batch_done(actions, &batch_id, ok, err);
            return;
        };

        let params = OtaPushParams {
            slot_id,
            batch_id: batch.batch_id.clone(),
            target_ip: ip,
            port,
            firmware_path: batch.artefact_path.clone(),
            log_path: batch.log_path.clone(),
        };
        *self.ota_push_slot.lock().unwrap() = Some(params);
        self.state = State::Pushing;
        actions.push(Action::PluginCall {
            plugin_id: self.ota_push_plugin_id,
            command: ota_push::CMD_START,
            data: PayloadBuf::empty(),
        });
    }

    fn emit_device_error(
        &self, actions: &mut ActionBuf, batch_id: &str, slot_id: &str,
        error_kind: &str, message: &str,
    ) {
        let payload = serde_json::to_vec(&serde_json::json!({
            "batch_id": batch_id,
            "slot_id": slot_id,
            "error_kind": error_kind,
            "message": message,
        })).unwrap_or_default();
        actions.push(Action::Send {
            target: Target::Broadcast,
            event_hash: self.device_error_hash,
            payload: PayloadBuf::from_slice(&payload),
        });
    }

    fn emit_batch_done(&self, actions: &mut ActionBuf, batch_id: &str, ok_count: u32, error_count: u32) {
        let payload = serde_json::to_vec(&serde_json::json!({
            "batch_id": batch_id,
            "ok_count": ok_count,
            "error_count": error_count,
        })).unwrap_or_default();
        actions.push(Action::Send {
            target: Target::Broadcast,
            event_hash: self.batch_done_hash,
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

/// Synthesise a short batch id when the payload omits one. Derived from
/// the wall clock (low 24 bits of the nanosecond count) — enough to
/// distinguish concurrent operator batches in the audit log.
fn synth_batch_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("b{:06x}", (nanos as u64) & 0x00FF_FFFF)
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
        let (s, flasher_slot, _ota) = make_sentant_full(apiary_dir, repo_root);
        (s, flasher_slot)
    }

    fn make_sentant_full(apiary_dir: PathBuf, repo_root: PathBuf)
        -> (DeploySentant, FlasherSlot, OtaPushSlot)
    {
        let slot: FlasherSlot = Arc::new(Mutex::new(None));
        let ota_slot: OtaPushSlot = Arc::new(Mutex::new(None));
        let apiary_ctx: RosterCtx = Arc::new(Mutex::new(Some(apiary_dir)));
        let sentant = DeploySentant::new(
            99, slot.clone(), 77, ota_slot.clone(), repo_root, apiary_ctx,
        );
        (sentant, slot, ota_slot)
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

    // ── F5: OTA batch push ──────────────────────────────────────────

    /// Seed the roster with one row at the given `state`, return its slot_id.
    fn seed_row(apiary: &std::path::Path, host: &str, state: &str) -> String {
        let mut roster_data = roster::Roster::default();
        let mut row = roster::new_placeholder(
            "sensor", "rocker-sensor", host, "kitchen", "2026-06-01T00:00:00Z");
        row.state = state.to_string();
        let slot_id = row.slot_id.clone();
        roster_data.devices.push(row);
        roster::save(apiary, &roster_data).unwrap();
        slot_id
    }

    #[test]
    fn batch_start_pushes_reachable_target() {
        let repo = tempfile::tempdir().unwrap();
        let apiary = tempfile::tempdir().unwrap();
        let slot_id = seed_row(apiary.path(), "esp32-s3-test", "reachable");

        let (mut s, _f, ota_slot) =
            make_sentant_full(apiary.path().to_path_buf(), repo.path().to_path_buf());
        let mut actions = ActionBuf::new();
        let payload = format!(
            r#"{{"batch_id":"bX","artefact_path":"/tmp/fw.bin","targets":[{{"slot_id":"{slot_id}","ip":"192.168.1.42"}}]}}"#);
        s.handle_event(&ev(s.batch_start_hash, payload.as_bytes(), EventSource::Local(0)), &mut actions);

        let collected: Vec<_> = actions.drain().collect();
        // Exactly one PluginCall (the push for the reachable target).
        assert_eq!(collected.len(), 1, "{collected:?}");
        match &collected[0] {
            Action::PluginCall { plugin_id, command, .. } => {
                assert_eq!(*plugin_id, 77);
                assert_eq!(*command, ota_push::CMD_START);
            }
            other => panic!("expected PluginCall, got {other:?}"),
        }
        assert_eq!(s.state, State::Pushing);
        // Params slot filled with the right target.
        let params = ota_slot.lock().unwrap().clone().expect("params");
        assert_eq!(params.target_ip, "192.168.1.42");
        assert_eq!(params.port, 21043);
        assert_eq!(params.slot_id, slot_id);
        assert_eq!(params.batch_id, "bX");
    }

    #[test]
    fn batch_start_gates_unreachable_target() {
        let repo = tempfile::tempdir().unwrap();
        let apiary = tempfile::tempdir().unwrap();
        let slot_id = seed_row(apiary.path(), "esp32-s3-test", "unreachable");

        let (mut s, _f, _ota) =
            make_sentant_full(apiary.path().to_path_buf(), repo.path().to_path_buf());
        let mut actions = ActionBuf::new();
        let payload = format!(
            r#"{{"batch_id":"bX","artefact_path":"/tmp/fw.bin","targets":[{{"slot_id":"{slot_id}","ip":"192.168.1.42"}}]}}"#);
        s.handle_event(&ev(s.batch_start_hash, payload.as_bytes(), EventSource::Local(0)), &mut actions);

        let collected: Vec<_> = actions.drain().collect();
        // No PluginCall — one device.error (gated) + one batch.done.
        assert!(collected.iter().all(|a| !matches!(a, Action::PluginCall { .. })),
            "must not push to an unreachable device");
        let err = collected.iter().find(|a| matches!(a, Action::Send { event_hash, .. } if *event_hash == s.device_error_hash)).expect("device.error");
        if let Action::Send { payload, .. } = err {
            let v: serde_json::Value = serde_json::from_slice(payload.as_slice()).unwrap();
            assert_eq!(v["error_kind"], "unreachable");
        }
        let done = collected.iter().find(|a| matches!(a, Action::Send { event_hash, .. } if *event_hash == s.batch_done_hash)).expect("batch.done");
        if let Action::Send { payload, .. } = done {
            let v: serde_json::Value = serde_json::from_slice(payload.as_slice()).unwrap();
            assert_eq!(v["ok_count"], 0);
            assert_eq!(v["error_count"], 1);
        }
        assert_eq!(s.state, State::Idle);
    }

    #[test]
    fn device_done_advances_then_completes_batch() {
        let repo = tempfile::tempdir().unwrap();
        let apiary = tempfile::tempdir().unwrap();
        let slot_id = seed_row(apiary.path(), "esp32-s3-test", "reachable");

        let (mut s, _f, _ota) =
            make_sentant_full(apiary.path().to_path_buf(), repo.path().to_path_buf());

        // Start a single-target batch → one push in flight.
        let mut actions = ActionBuf::new();
        let payload = format!(
            r#"{{"batch_id":"bX","artefact_path":"/tmp/fw.bin","targets":[{{"slot_id":"{slot_id}","ip":"10.0.0.5"}}]}}"#);
        s.handle_event(&ev(s.batch_start_hash, payload.as_bytes(), EventSource::Local(0)), &mut actions);
        let _ = actions.drain().count();
        assert_eq!(s.state, State::Pushing);

        // Plugin reports the device done → batch should complete.
        let mut actions = ActionBuf::new();
        let done_payload = format!(r#"{{"batch_id":"bX","slot_id":"{slot_id}","artefact_sha256":"ab","duration_ms":10}}"#);
        s.handle_event(&ev(s.device_done_hash, done_payload.as_bytes(), EventSource::Plugin(0)), &mut actions);

        let collected: Vec<_> = actions.drain().collect();
        // Re-broadcast of device.done + a batch.done with ok=1.
        assert!(collected.iter().any(|a| matches!(a, Action::Send { event_hash, .. } if *event_hash == s.device_done_hash)));
        let done = collected.iter().find(|a| matches!(a, Action::Send { event_hash, .. } if *event_hash == s.batch_done_hash)).expect("batch.done");
        if let Action::Send { payload, .. } = done {
            let v: serde_json::Value = serde_json::from_slice(payload.as_slice()).unwrap();
            assert_eq!(v["ok_count"], 1);
            assert_eq!(v["error_count"], 0);
        }
        assert_eq!(s.state, State::Idle);
        assert!(s.batch.is_none());
    }

    /// Full end-to-end through the **real** `EventBus`: composes the
    /// Deploy sentant + ota_push plugin exactly like `hive.rs`, injects
    /// `deploy.batch.start` the way the WS bridge does, pumps the bus,
    /// and asserts the whole event chain comes back out on the outbound
    /// queue — with a real TCP push to a fake wire-v1 device.
    #[test]
    fn e2e_batch_push_through_real_bus() {
        use r2_engine::queue::QueuedEvent;
        use r2_engine::EventBus;
        use sha2::Digest;
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::time::{Duration, Instant};

        // 1. Fake wire-v1 device: read until the client's FIN, then reply
        //    [status 0x00][len u16 LE]["OK"]. Record how many bytes it read.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let bytes_read = Arc::new(Mutex::new(0usize));
        let br = bytes_read.clone();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut all = Vec::new();
                let _ = stream.read_to_end(&mut all);
                *br.lock().unwrap() = all.len();
                let _ = stream.write_all(&[0x00, 0x02, 0x00, b'O', b'K']);
                let _ = stream.flush();
            }
        });

        // 2. Apiary with one reachable roster row + a firmware artefact.
        let repo = tempfile::tempdir().unwrap();
        let apiary = tempfile::tempdir().unwrap();
        let slot_id = seed_row(apiary.path(), "esp32-s3-xiao", "reachable");
        let fw_bytes = vec![0x5Au8; 40_000];
        let fw_path = apiary.path().join("firmware.bin");
        std::fs::write(&fw_path, &fw_bytes).unwrap();

        // 3. Compose the real bus the way hive.rs does.
        let ota_slot: OtaPushSlot = Arc::new(Mutex::new(None));
        let flasher_slot: FlasherSlot = Arc::new(Mutex::new(None));
        let apiary_ctx: RosterCtx = Arc::new(Mutex::new(Some(apiary.path().to_path_buf())));
        let mut bus = EventBus::new();
        let ota_pid = bus.register_plugin(Box::new(ota_push::OtaPushPlugin::new(0, ota_slot.clone())));
        bus.register_sentant(Box::new(DeploySentant::new(
            0, flasher_slot, ota_pid, ota_slot, repo.path().to_path_buf(), apiary_ctx,
        )));
        bus.init_all();

        // 4. Inject deploy.batch.start (source 0xFF — exactly how the WS
        //    bridge tags inbound events). Target our fake device's port.
        let reg = registry();
        let done_hash = reg.hash_of("r2.composer.deploy.batch.done").unwrap();
        let dev_done_hash = reg.hash_of("r2.composer.deploy.device.done").unwrap();
        let dev_prog_hash = reg.hash_of("r2.composer.deploy.device.progress").unwrap();
        let payload = serde_json::to_vec(&serde_json::json!({
            "batch_id": "e2e",
            "artefact_path": fw_path.to_string_lossy(),
            "targets": [{ "slot_id": slot_id, "ip": "127.0.0.1", "port": port }],
        })).unwrap();
        bus.enqueue(QueuedEvent::new(
            reg.hash_of("r2.composer.deploy.batch.start").unwrap(),
            0xFF, false, 0, &payload,
        ));

        // 5. Pump: poll plugins → tick → collect outbound, until batch.done.
        let mut outbound: Vec<(u32, Vec<u8>)> = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            bus.poll_plugins();
            bus.tick();
            for q in bus.drain_outbound() {
                outbound.push((q.hash, q.payload().to_vec()));
            }
            if outbound.iter().any(|(h, _)| *h == done_hash) { break; }
            std::thread::sleep(Duration::from_millis(15));
        }

        // 6a. The fake device received the full framed request:
        //     [0x01][size u32 LE][sha 32][body] = 37 + firmware bytes.
        let got = *bytes_read.lock().unwrap();
        assert_eq!(got, 37 + fw_bytes.len(), "device read full wire-v1 request");

        // 6b. device.progress streamed, device.done carries the batch id.
        assert!(outbound.iter().any(|(h, _)| *h == dev_prog_hash),
            "expected ≥1 deploy.device.progress on the wire");
        let (_, dd) = outbound.iter().find(|(h, _)| *h == dev_done_hash)
            .expect("deploy.device.done on the wire");
        let v: serde_json::Value = serde_json::from_slice(dd).unwrap();
        assert_eq!(v["batch_id"], "e2e");
        assert_eq!(v["slot_id"], slot_id);
        let expected_sha = hex::encode(sha2::Sha256::digest(&fw_bytes));
        assert_eq!(v["artefact_sha256"], expected_sha);

        // 6c. batch.done with ok=1, error=0.
        let (_, bd) = outbound.iter().find(|(h, _)| *h == done_hash)
            .expect("deploy.batch.done on the wire");
        let v: serde_json::Value = serde_json::from_slice(bd).unwrap();
        assert_eq!(v["ok_count"], 1);
        assert_eq!(v["error_count"], 0);
    }
}
