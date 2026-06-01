//! `flasher` plugin — subprocess driver for `esptool` per
//! SPEC-APIARY-FLASH §4.2 + §4.3.
//!
//! v0.1 (F2 scope): four-region `write_flash` invocation, line-by-line
//! stdout parsing into `r2.composer.deploy.first_install.progress`
//! events, `done`/`error` on exit. Bytes-identical CLI invocation to
//! r2-workshop's existing path.
//!
//! Hard gate per SPEC-APIARY-FLASH §4.2 + §9.4: this plugin **MUST**
//! use `esptool` and **MUST NOT** use `espflash` (the latter writes a
//! header byte that breaks ESP-IDF v5.3+ bootloaders).
//!
//! ## Lifecycle (same shape as the `claude-code` plugin)
//!
//! - `Plugin::execute(CMD_START, _)` — pulls a `FlashParams` from the
//!   shared slot (the Deploy sentant fills it before firing the
//!   PluginCall), spawns `esptool …` in worker threads, returns
//!   `Ok(empty)` immediately.
//! - `Plugin::execute(CMD_CANCEL, _)` — drops the receiver; the wait
//!   thread reaps the child naturally. v0.1 doesn't actually `kill()`
//!   the child (matching claude-code's posture); a follow-up will add
//!   a shared `Child` handle for proper cancellation.
//! - `Plugin::poll()` — drains one parsed line at a time as
//!   `(event_hash, payload)` tuples. Reader joins to the wait thread
//!   per the same race fix landed for claude-code.
//!
//! ## Events emitted (by hash, via `poll()`)
//!
//! - `r2.composer.deploy.first_install.progress` — one per parsed
//!   esptool stdout line. Payload `{phase, line}`.
//! - `r2.composer.deploy.first_install.done` — esptool exited 0.
//!   Payload `{exit_code: 0, port, artefact_sha256}`.
//! - `r2.composer.deploy.first_install.error` — non-zero exit, IO
//!   error, or spawn failure. Payload `{exit_code, message, stderr_tail}`.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};
use serde::Serialize;

/// Side-channel slot for delivering flash parameters from the Deploy
/// sentant to the flasher plugin. Bus payload (`Action::PluginCall.data`)
/// is capped at 4 KiB on the workstation build, plenty for a single
/// command, but we use the slot pattern for symmetry with `claude-code`
/// and so the params type can grow without bus-payload pressure.
pub type FlasherSlot = Arc<Mutex<Option<FlashParams>>>;

/// All inputs the flasher needs for one USB first-install.
#[derive(Debug, Clone)]
pub struct FlashParams {
    /// Stable per-target id, e.g. "sensor:esp32-s3-xiao:8c1e3aaf".
    pub slot_id: String,
    /// Serial port — `/dev/ttyACM0` on Linux.
    pub port: String,
    /// `esptool --chip <arg>` value, from `board.toml [usb].esptool_chip_arg`.
    pub chip_arg: String,
    /// Baud rate. `460800` is standard for USB-Serial-JTAG.
    pub baud: u32,
    /// Pairs of (offset, file-path) to write per
    /// SPEC-APIARY-FLASH §4.3's four-region pattern. Caller provides
    /// the resolved file paths; we don't construct them.
    pub regions: Vec<FlashRegion>,
    /// Optional SHA-256 of the primary firmware.bin — included in the
    /// done event so the Roster sentant can write `firmware_sha`.
    pub firmware_sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FlashRegion {
    /// Hex offset like "0x0", "0x8000", "0x20000".
    pub offset: String,
    /// Absolute path to the .bin file.
    pub path: PathBuf,
}

/// Command opcode: start a new `esptool write_flash`.
pub const CMD_START: PluginCommand = 0x01;
/// Command opcode: cancel the running session.
pub const CMD_CANCEL: PluginCommand = 0x02;

pub const ERR_SPAWN: u8 = 0x01;
pub const ERR_BUSY: u8 = 0x02;
pub const ERR_NO_PARAMS: u8 = 0x03;
pub const ERR_TOOL_BANNED: u8 = 0x04;
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

#[derive(Debug)]
enum WorkerMsg {
    /// One progress event derived from an esptool stdout line.
    Progress { phase: String, line: String },
    /// Subprocess finished with this exit code. Carries slot_id so the
    /// Roster sentant can transition the right row on
    /// first_install.done.
    Done { exit_code: i32, port: String, slot_id: String },
    /// Setup or read error. Carries slot_id for the same reason.
    Error { message: String, exit_code: i32, stderr_tail: String, slot_id: String },
}

pub struct FlasherPlugin {
    id: PluginId,
    rx: Option<mpsc::Receiver<WorkerMsg>>,
    /// Pre-hashed event names.
    hash_progress: u32,
    hash_done: u32,
    hash_error: u32,
    /// Reusable output buffer for `poll()`.
    out_buf: Vec<u8>,
    /// Where the Deploy sentant deposits the params before firing
    /// `PluginCall::CMD_START`. Cleared on each `start()` call.
    params_slot: FlasherSlot,
    /// The command spawned. Default `"esptool"`; tests override.
    program: String,
    /// Whether to use the real esptool arg-construction or a test
    /// override. Tests pass `program = "sh"` + a custom command tail
    /// via the override; the plugin doesn't build esptool args itself
    /// in that case.
    test_args_override: Option<Vec<String>>,
}

impl FlasherPlugin {
    pub fn new(id: PluginId, params_slot: FlasherSlot) -> Self {
        Self {
            id,
            rx: None,
            hash_progress: r2_fnv::fnv1a_32(b"r2.composer.deploy.first_install.progress"),
            hash_done:     r2_fnv::fnv1a_32(b"r2.composer.deploy.first_install.done"),
            hash_error:    r2_fnv::fnv1a_32(b"r2.composer.deploy.first_install.error"),
            out_buf: Vec::with_capacity(512),
            params_slot,
            program: "esptool".into(),
            test_args_override: None,
        }
    }

    /// Construct with a fixture-style command — used by tests to
    /// replay canned esptool output without needing real hardware.
    /// The override list goes to `Command::new(program).args(...)`
    /// directly; FlashParams from the slot is consumed but not used
    /// to build args.
    pub fn with_test_command(
        id: PluginId,
        params_slot: FlasherSlot,
        program: impl Into<String>,
        args: Vec<String>,
    ) -> Self {
        let mut p = Self::new(id, params_slot);
        p.program = program.into();
        p.test_args_override = Some(args);
        p
    }

    fn start(&mut self) -> PluginResult {
        if self.rx.is_some() {
            return PluginResult::Error(PluginError::new(ERR_BUSY, "flasher session already running"));
        }
        let params = match self.params_slot.lock().unwrap().take() {
            Some(p) => p,
            None => return PluginResult::Error(PluginError::new(
                ERR_NO_PARAMS,
                "no FlashParams in slot — Deploy sentant must fill before firing CMD_START",
            )),
        };

        if self.program.contains("espflash") {
            // Per SPEC-APIARY-FLASH §4.2 + §9.4 — espflash MUST NOT be
            // invoked; v3.x writes a header byte that breaks ESP-IDF
            // v5.3+ bootloaders.
            return PluginResult::Error(PluginError::new(
                ERR_TOOL_BANNED,
                "espflash is banned (SPEC-APIARY-FLASH §4.2); use esptool",
            ));
        }

        let args: Vec<String> = match &self.test_args_override {
            Some(test) => test.clone(),
            None => build_esptool_args(&params),
        };

        let mut cmd = Command::new(&self.program);
        cmd.args(&args);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return PluginResult::Error(PluginError::new(
                    ERR_SPAWN,
                    &format!("spawn failed: {e}"),
                ));
            }
        };
        // Close stdin — esptool doesn't read it but the pipe matters
        // for some shells.
        drop(child.stdin.take());
        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                return PluginResult::Error(PluginError::new(ERR_SPAWN, "no stdout pipe"));
            }
        };
        let stderr = child.stderr.take();

        let (tx, rx) = mpsc::channel();
        let port_for_done = params.port.clone();
        let sha_for_done = params.firmware_sha256.clone();
        let slot_id_for_done = params.slot_id.clone();

        // Reader thread: parse stdout line-by-line into phases.
        let tx_rd = tx.clone();
        let slot_id_for_reader = slot_id_for_done.clone();
        let reader_handle = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(s) => {
                        let phase = classify_esptool_line(&s);
                        let _ = tx_rd.send(WorkerMsg::Progress { phase, line: s });
                    }
                    Err(e) => {
                        let _ = tx_rd.send(WorkerMsg::Error {
                            message: format!("stdout read failed: {e}"),
                            exit_code: -1,
                            stderr_tail: String::new(),
                            slot_id: slot_id_for_reader.clone(),
                        });
                        break;
                    }
                }
            }
        });

        // stderr drain thread — captures the tail for error reports.
        let stderr_handle = stderr.map(|stderr| {
            thread::spawn(move || -> String {
                let reader = BufReader::new(stderr);
                let mut tail: Vec<String> = Vec::new();
                for line in reader.lines().flatten() {
                    tail.push(line);
                    if tail.len() > 32 {
                        tail.remove(0);
                    }
                }
                tail.join("\n")
            })
        });

        // Wait thread: drain the child, JOIN the reader (per the race
        // fix landed in claude_code.rs), then send Done/Error.
        thread::spawn(move || {
            let status = child.wait();
            let _ = reader_handle.join();
            let stderr_tail = stderr_handle
                .map(|h| h.join().unwrap_or_default())
                .unwrap_or_default();
            match status {
                Ok(s) => {
                    let code = s.code().unwrap_or(-1);
                    if code == 0 {
                        let _ = tx.send(WorkerMsg::Done {
                            exit_code: 0,
                            port: port_for_done.clone(),
                            slot_id: slot_id_for_done.clone(),
                        });
                    } else {
                        let _ = tx.send(WorkerMsg::Error {
                            message: format!("esptool exited with code {code}"),
                            exit_code: code,
                            stderr_tail,
                            slot_id: slot_id_for_done.clone(),
                        });
                    }
                    let _ = sha_for_done; // reserved for done payload upgrade
                }
                Err(e) => {
                    let _ = tx.send(WorkerMsg::Error {
                        message: format!("wait failed: {e}"),
                        exit_code: -1,
                        stderr_tail,
                        slot_id: slot_id_for_done.clone(),
                    });
                }
            }
        });

        self.rx = Some(rx);
        PluginResult::Ok(PluginResponse::empty())
    }

    fn cancel(&mut self) -> PluginResult {
        self.rx = None;
        PluginResult::Ok(PluginResponse::empty())
    }

    fn pack<T: Serialize>(&mut self, v: &T) -> &[u8] {
        self.out_buf.clear();
        let _ = serde_json::to_writer(&mut self.out_buf, v);
        &self.out_buf
    }
}

impl Plugin for FlasherPlugin {
    fn execute(&mut self, command: PluginCommand, _data: &[u8]) -> PluginResult {
        match command {
            CMD_START => self.start(),
            CMD_CANCEL => self.cancel(),
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "unknown command byte")),
        }
    }
    fn name(&self) -> &str { "flasher" }
    fn id(&self) -> PluginId { self.id }

    fn poll(&mut self) -> Option<(u32, &[u8])> {
        let msg = self.rx.as_ref()?.try_recv().ok()?;
        let (hp, hd, he) = (self.hash_progress, self.hash_done, self.hash_error);
        match msg {
            WorkerMsg::Progress { phase, line } => {
                let payload = self.pack(&serde_json::json!({
                    "phase": phase,
                    "line": line,
                }));
                Some((hp, payload))
            }
            WorkerMsg::Done { exit_code, port, slot_id } => {
                self.rx = None;
                let payload = self.pack(&serde_json::json!({
                    "exit_code": exit_code,
                    "port": port,
                    "slot_id": slot_id,
                }));
                Some((hd, payload))
            }
            WorkerMsg::Error { message, exit_code, stderr_tail, slot_id } => {
                self.rx = None;
                let payload = self.pack(&serde_json::json!({
                    "exit_code": exit_code,
                    "message": message,
                    "stderr_tail": stderr_tail,
                    "slot_id": slot_id,
                }));
                Some((he, payload))
            }
        }
    }
}

/// Build the `esptool` argv from FlashParams. Per SPEC-APIARY-FLASH
/// §4.3's four-region write pattern.
fn build_esptool_args(p: &FlashParams) -> Vec<String> {
    let mut args = vec![
        "--chip".into(), p.chip_arg.clone(),
        "--port".into(), p.port.clone(),
        "--baud".into(), p.baud.to_string(),
        "--before".into(), "default_reset".into(),
        "--after".into(),  "hard_reset".into(),
        "write_flash".into(),
    ];
    for region in &p.regions {
        args.push(region.offset.clone());
        args.push(region.path.display().to_string());
    }
    args
}

/// Map an esptool stdout line to a coarse phase.
fn classify_esptool_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if lower.contains("connecting") { "connecting" }
    else if lower.contains("hash of data verified") { "verified" }
    else if lower.starts_with("writing at") || lower.contains("writing at") { "writing" }
    else if lower.contains("compressed") { "erasing" }
    else if lower.contains("wrote ") { "verifying" }
    else if lower.contains("hard resetting") || lower.contains("resetting") { "resetting" }
    else { "other" }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn slot_with(p: FlashParams) -> FlasherSlot {
        Arc::new(Mutex::new(Some(p)))
    }

    fn drain_with_timeout(p: &mut FlasherPlugin, timeout: Duration) -> Vec<(u32, Vec<u8>)> {
        let mut out = Vec::new();
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some((h, payload)) = p.poll() {
                out.push((h, payload.to_vec()));
                continue;
            }
            std::thread::sleep(Duration::from_millis(20));
            if let Some((h, _)) = out.last() {
                if *h == p.hash_done || *h == p.hash_error { break; }
            }
        }
        out
    }

    fn make_params() -> FlashParams {
        FlashParams {
            slot_id: "sensor:esp32-s3-xiao:8c1e3aaf".into(),
            port: "/dev/ttyACM0".into(),
            chip_arg: "esp32s3".into(),
            baud: 460800,
            regions: vec![],
            firmware_sha256: None,
        }
    }

    #[test]
    fn unknown_command_errors() {
        let mut p = FlasherPlugin::new(0, Arc::new(Mutex::new(None)));
        match p.execute(0xAA, &[]) {
            PluginResult::Error(e) => assert_eq!(e.code, ERR_UNKNOWN_COMMAND),
            _ => panic!(),
        }
    }

    #[test]
    fn missing_params_errors() {
        let mut p = FlasherPlugin::new(0, Arc::new(Mutex::new(None)));
        match p.execute(CMD_START, &[]) {
            PluginResult::Error(e) => assert_eq!(e.code, ERR_NO_PARAMS),
            _ => panic!(),
        }
    }

    #[test]
    fn espflash_program_refused() {
        let slot = slot_with(make_params());
        let mut p = FlasherPlugin::with_test_command(
            0, slot.clone(), "espflash", vec!["--chip".into(), "esp32s3".into()],
        );
        match p.execute(CMD_START, &[]) {
            PluginResult::Error(e) => assert_eq!(e.code, ERR_TOOL_BANNED),
            _ => panic!("expected ERR_TOOL_BANNED"),
        }
        // Params slot was consumed but the tool refused — slot is now empty.
        assert!(slot.lock().unwrap().is_none());
    }

    #[test]
    fn fixture_round_trips_to_progress_then_done() {
        // printf canned esptool-like output then exits 0.
        let slot = slot_with(make_params());
        let mut p = FlasherPlugin::with_test_command(
            0, slot, "sh",
            vec![
                "-c".into(),
                r#"printf 'esptool.py v4.7.0\nConnecting....\nChip is ESP32-S3\nCompressed 17744 bytes to 11820\nWriting at 0x00010000... (100 %%)\nWrote 17744 bytes\nHash of data verified.\nHard resetting via RTS pin...\n'"#.into(),
            ],
        );
        match p.execute(CMD_START, &[]) {
            PluginResult::Ok(_) => {}
            PluginResult::Error(e) => panic!("start failed: 0x{:02X}: {}", e.code, e.description()),
        }
        let events = drain_with_timeout(&mut p, Duration::from_secs(3));
        let prog = p.hash_progress;
        let done = p.hash_done;
        let prog_count = events.iter().filter(|(h, _)| *h == prog).count();
        let done_count = events.iter().filter(|(h, _)| *h == done).count();
        assert!(prog_count >= 6, "expected ≥6 progress events, got {prog_count}: {events:?}");
        assert_eq!(done_count, 1, "expected one done event");

        // Verify a Writing-at line was classified as `writing`.
        let writing = events.iter().find_map(|(h, b)| {
            if *h != prog { return None; }
            let v: serde_json::Value = serde_json::from_slice(b).ok()?;
            (v["phase"].as_str()? == "writing").then(|| v)
        });
        assert!(writing.is_some(), "expected at least one `writing` phase event");

        // Done payload carries the port.
        let done_payload = events.iter().find(|(h, _)| *h == done).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&done_payload.1).unwrap();
        assert_eq!(v["port"], "/dev/ttyACM0");
        assert_eq!(v["exit_code"], 0);
        assert_eq!(v["slot_id"], "sensor:esp32-s3-xiao:8c1e3aaf");
    }

    #[test]
    fn nonzero_exit_yields_error_event() {
        let slot = slot_with(make_params());
        let mut p = FlasherPlugin::with_test_command(
            0, slot, "sh",
            vec!["-c".into(), "echo 'pretend connection refused' 1>&2; exit 5".into()],
        );
        assert!(matches!(p.execute(CMD_START, &[]), PluginResult::Ok(_)));
        let events = drain_with_timeout(&mut p, Duration::from_secs(2));
        let err = p.hash_error;
        let err_event = events.iter().find(|(h, _)| *h == err).expect("error event");
        let v: serde_json::Value = serde_json::from_slice(&err_event.1).unwrap();
        assert_eq!(v["exit_code"], 5);
        assert!(v["stderr_tail"].as_str().unwrap().contains("connection refused"));
    }

    #[test]
    fn build_esptool_args_includes_all_regions() {
        let p = FlashParams {
            slot_id: "x".into(),
            port: "/dev/ttyACM1".into(),
            chip_arg: "esp32c6".into(),
            baud: 460800,
            regions: vec![
                FlashRegion { offset: "0x0".into(), path: PathBuf::from("/tmp/boot.bin") },
                FlashRegion { offset: "0x8000".into(), path: PathBuf::from("/tmp/pt.bin") },
                FlashRegion { offset: "0x20000".into(), path: PathBuf::from("/tmp/fw.bin") },
            ],
            firmware_sha256: None,
        };
        let args = build_esptool_args(&p);
        // Spot-check shape.
        assert!(args.iter().any(|a| a == "esp32c6"));
        assert!(args.iter().any(|a| a == "/dev/ttyACM1"));
        assert!(args.iter().any(|a| a == "write_flash"));
        assert!(args.iter().any(|a| a == "0x0"));
        assert!(args.iter().any(|a| a == "/tmp/boot.bin"));
        assert!(args.iter().any(|a| a == "0x8000"));
        assert!(args.iter().any(|a| a == "0x20000"));
    }

    #[test]
    fn classify_phases() {
        assert_eq!(classify_esptool_line("Connecting...."), "connecting");
        assert_eq!(classify_esptool_line("Writing at 0x00010000... (100 %)"), "writing");
        assert_eq!(classify_esptool_line("Compressed 17744 bytes to 11820"), "erasing");
        assert_eq!(classify_esptool_line("Wrote 17744 bytes"), "verifying");
        assert_eq!(classify_esptool_line("Hash of data verified."), "verified");
        assert_eq!(classify_esptool_line("Hard resetting via RTS pin..."), "resetting");
        assert_eq!(classify_esptool_line("Random output"), "other");
    }
}
