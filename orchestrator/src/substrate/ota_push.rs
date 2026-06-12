//! `ota_push` substrate — wire-v1 OTA firmware push over TCP.
//!
//! Per R2-UPDATE §3.1.2.2 ("TCP OTA Protocol") + r2-workshop's
//! device-side `crates/r2-esp/src/ota_tcp.rs`. This is the L6
//! Management role that pushes a new `firmware.bin` to a device's
//! always-listening OTA receiver on TCP port **21043** (0x5233).
//!
//! ## Wire-v1 (byte-exact — confirmed against R2-UPDATE + r2-workshop)
//!
//! r2-composer's earlier `SPEC-APIARY-FLASH §6.2/§6.4` described a
//! text-line shape (`[u32 BE length]`, `OK <sha>\n`, `REBOOTING\n`).
//! **That was wrong.** The real wire is binary-framed:
//!
//! Request (client → device):
//! ```text
//!   [0x01 CMD_START][size: u32 LE][sha256: 32 raw bytes][firmware bytes…]
//!   then half-close the write side (TCP FIN) to signal EOF.
//! ```
//! The device reads a 1-byte command, then a 36-byte preamble
//! (`size` + `sha256`), then streams the body until EOF.
//!
//! Response (device → device):
//! ```text
//!   [status: u8][msg_len: u16 LE][msg: utf-8]
//!   status 0x00 = OK   (msg is literally "OK"; the sha is NOT echoed)
//!   status 0x01 = error (msg = "<CODE> <detail>", a CODE token from the
//!                 OTA reply-status contract — see classify_device_error)
//! ```
//! There is no `REBOOTING` frame: on success the device logs locally
//! and `esp_restart()`s ~2 s later, so the connection just drops. The
//! pusher does **not** send a `CMD_QUERY` (0x02) version probe first —
//! it opens and sends `CMD_START` directly.
//!
//! ## Lifecycle (same shape as `composer::flasher`)
//!
//! - `execute(CMD_START, _)` — takes an `OtaPushParams` from the shared
//!   slot (the Deploy sentant fills it before firing the PluginCall),
//!   spawns the push on a worker thread, returns `Ok(empty)` at once.
//! - `execute(CMD_CANCEL, _)` — drops the receiver; the worker finishes
//!   in the background (v0.1 does not abort an in-flight TCP write).
//! - `poll()` — drains one worker message at a time as `(hash, payload)`.
//!
//! ## Events emitted (by hash, via `poll()`) — SPEC-APIARY-FLASH §8.1
//!
//! - `r2.composer.deploy.device.progress` — `{batch_id, slot_id, phase,
//!   bytes_sent, bytes_total, percent}`. Phases: `connecting` →
//!   `sending` → `awaiting-ack` → `rebooting`.
//! - `r2.composer.deploy.device.done` — `{batch_id, slot_id,
//!   artefact_sha256, duration_ms}`.
//! - `r2.composer.deploy.device.error` — `{batch_id, slot_id,
//!   error_kind, message}`. `error_kind` ∈ {`unreachable`, `send-failed`,
//!   `reboot-timeout`, `response-malformed`, `artefact-missing`} for
//!   transport-side failures, plus the device-reported CODEs mapped by
//!   `classify_device_error`: `sha-mismatch`, `too-big`, `bad-magic`,
//!   `write-fail`, `no-slot`, `preamble`, `short`, `device-error`.
//!
//! ## Out of scope for F5 (deferred to F5b)
//!
//! §6.2 steps 5–7 — waiting up to 90 s for the rebooted device to emit
//! a beacon carrying the NEW firmware's sha, then updating
//! `firmware_sha`/`firmware_ver` on the roster row. That needs
//! cross-component correlation with [`crate::substrate::beacon_observer`],
//! so the push transaction lands first; `rebooting` is the last phase
//! this component emits.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Side-channel slot for delivering push parameters from the Deploy
/// sentant to the ota_push plugin (mirrors `composer::flasher`'s
/// `FlasherSlot`).
pub type OtaPushSlot = Arc<Mutex<Option<OtaPushParams>>>;

/// All inputs one OTA push needs.
#[derive(Debug, Clone)]
pub struct OtaPushParams {
    /// Stable per-target slot id (roster row key).
    pub slot_id: String,
    /// Correlates this push with its enclosing batch.
    pub batch_id: String,
    /// Device IP on the WiFi network, e.g. `192.168.1.42`.
    ///
    /// NOTE: the IP-resolution *source* is underspecified upstream —
    /// roster rows carry no IP and R2-BEACON is BLE (no IP). For F5
    /// the Deploy sentant supplies it from the `deploy.batch.start`
    /// payload (operator/AI-provided, per SPEC-APIARY-FLASH §6.1's
    /// "force-push to basement" flow). A reachability/mDNS cache that
    /// fills this automatically is a follow-up.
    pub target_ip: String,
    /// OTA receiver port — `21043` per r2-workshop.
    pub port: u16,
    /// Absolute path to the `firmware.bin` to push.
    pub firmware_path: PathBuf,
    /// Optional `apiaries/<name>/devices/deploy_log.jsonl` for the
    /// §6.5 audit append. `None` skips logging (e.g. in tests).
    pub log_path: Option<PathBuf>,
}

/// Command opcode: start a push.
pub const CMD_START: PluginCommand = 0x01;
/// Command opcode: forget the running session.
pub const CMD_CANCEL: PluginCommand = 0x02;

// ── On-the-wire constants (R2-UPDATE §3.1.2.2 / r2-workshop ota_tcp.rs) ──
const WIRE_CMD_START: u8 = 0x01;
const WIRE_STATUS_OK: u8 = 0x00;

pub const ERR_BUSY: u8 = 0x02;
pub const ERR_NO_PARAMS: u8 = 0x03;
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

/// Chunk size for streaming the firmware body (and the granularity at
/// which `sending` progress is throttled — one event per percent).
const SEND_CHUNK: usize = 32 * 1024;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
/// The device verifies the SHA-256 then writes the inactive OTA slot
/// before replying, which can take tens of seconds on a large image —
/// hence a generous read timeout.
const READ_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug)]
enum WorkerMsg {
    Progress { phase: String, bytes_sent: u64, bytes_total: u64, percent: u8 },
    Done { artefact_sha256: String, duration_ms: u64 },
    Error { error_kind: String, message: String },
}

pub struct OtaPushPlugin {
    id: PluginId,
    rx: Option<mpsc::Receiver<WorkerMsg>>,
    hash_progress: u32,
    hash_done: u32,
    hash_error: u32,
    out_buf: Vec<u8>,
    params_slot: OtaPushSlot,
    /// (batch_id, slot_id) for the in-flight push — stamped into every
    /// `poll()` payload so the webapp can correlate.
    current: Option<(String, String)>,
}

impl OtaPushPlugin {
    pub fn new(id: PluginId, params_slot: OtaPushSlot) -> Self {
        Self {
            id,
            rx: None,
            hash_progress: r2_fnv::fnv1a_32(b"r2.composer.deploy.device.progress"),
            hash_done:     r2_fnv::fnv1a_32(b"r2.composer.deploy.device.done"),
            hash_error:    r2_fnv::fnv1a_32(b"r2.composer.deploy.device.error"),
            out_buf: Vec::with_capacity(256),
            params_slot,
            current: None,
        }
    }

    fn start(&mut self) -> PluginResult {
        if self.rx.is_some() {
            return PluginResult::Error(PluginError::new(ERR_BUSY, "ota_push session already running"));
        }
        let params = match self.params_slot.lock().unwrap().take() {
            Some(p) => p,
            None => return PluginResult::Error(PluginError::new(
                ERR_NO_PARAMS,
                "no OtaPushParams in slot — Deploy sentant must fill before firing CMD_START",
            )),
        };
        self.current = Some((params.batch_id.clone(), params.slot_id.clone()));
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || run_push(params, tx));
        self.rx = Some(rx);
        PluginResult::Ok(PluginResponse::empty())
    }

    fn cancel(&mut self) -> PluginResult {
        self.rx = None;
        self.current = None;
        PluginResult::Ok(PluginResponse::empty())
    }

    fn pack<T: Serialize>(&mut self, v: &T) -> &[u8] {
        self.out_buf.clear();
        let _ = serde_json::to_writer(&mut self.out_buf, v);
        &self.out_buf
    }
}

impl Plugin for OtaPushPlugin {
    fn execute(&mut self, command: PluginCommand, _data: &[u8]) -> PluginResult {
        match command {
            CMD_START => self.start(),
            CMD_CANCEL => self.cancel(),
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "unknown command byte")),
        }
    }
    fn name(&self) -> &str { "ota_push" }
    fn id(&self) -> PluginId { self.id }

    fn poll(&mut self) -> Option<(u32, &[u8])> {
        let msg = self.rx.as_ref()?.try_recv().ok()?;
        let (batch_id, slot_id) = self.current.clone().unwrap_or_default();
        let (hp, hd, he) = (self.hash_progress, self.hash_done, self.hash_error);
        match msg {
            WorkerMsg::Progress { phase, bytes_sent, bytes_total, percent } => {
                let payload = self.pack(&serde_json::json!({
                    "batch_id": batch_id,
                    "slot_id": slot_id,
                    "phase": phase,
                    "bytes_sent": bytes_sent,
                    "bytes_total": bytes_total,
                    "percent": percent,
                }));
                Some((hp, payload))
            }
            WorkerMsg::Done { artefact_sha256, duration_ms } => {
                self.rx = None;
                let payload = self.pack(&serde_json::json!({
                    "batch_id": batch_id,
                    "slot_id": slot_id,
                    "artefact_sha256": artefact_sha256,
                    "duration_ms": duration_ms,
                }));
                Some((hd, payload))
            }
            WorkerMsg::Error { error_kind, message } => {
                self.rx = None;
                let payload = self.pack(&serde_json::json!({
                    "batch_id": batch_id,
                    "slot_id": slot_id,
                    "error_kind": error_kind,
                    "message": message,
                }));
                Some((he, payload))
            }
        }
    }
}

/// Resolve `host:port` to a single `SocketAddr` and connect with a
/// bounded timeout (so an unreachable device fails fast as
/// `E_OTA_UNREACHABLE` rather than hanging on the OS default).
fn connect_timeout(addr: &str, timeout: Duration) -> std::io::Result<TcpStream> {
    let sock = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no address resolved"))?;
    TcpStream::connect_timeout(&sock, timeout)
}

/// The whole push transaction, run on a worker thread. Every exit path
/// sends exactly one terminal `Done`/`Error`.
fn run_push(params: OtaPushParams, tx: mpsc::Sender<WorkerMsg>) {
    let start = Instant::now();

    let fw = match std::fs::read(&params.firmware_path) {
        Ok(b) => b,
        Err(e) => {
            let _ = tx.send(WorkerMsg::Error {
                error_kind: "artefact-missing".into(),
                message: format!("read {}: {e}", params.firmware_path.display()),
            });
            return;
        }
    };
    let total = fw.len() as u64;

    let digest = {
        let mut h = Sha256::new();
        h.update(&fw);
        h.finalize()
    };
    let sha_hex = hex::encode(digest);

    let _ = tx.send(WorkerMsg::Progress {
        phase: "connecting".into(), bytes_sent: 0, bytes_total: total, percent: 0,
    });

    let addr = format!("{}:{}", params.target_ip, params.port);
    let mut stream = match connect_timeout(&addr, CONNECT_TIMEOUT) {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(WorkerMsg::Error {
                error_kind: "unreachable".into(),
                message: format!("connect {addr}: {e}"),
            });
            return;
        }
    };
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));

    // Preamble: [0x01][size u32 LE][sha256 32 raw bytes].
    let mut preamble = Vec::with_capacity(37);
    preamble.push(WIRE_CMD_START);
    preamble.extend_from_slice(&(fw.len() as u32).to_le_bytes());
    preamble.extend_from_slice(&digest);
    if let Err(e) = stream.write_all(&preamble) {
        let _ = tx.send(WorkerMsg::Error {
            error_kind: "send-failed".into(),
            message: format!("preamble write: {e}"),
        });
        return;
    }

    // Stream the firmware body, throttling progress to one event/percent.
    let _ = tx.send(WorkerMsg::Progress {
        phase: "sending".into(), bytes_sent: 0, bytes_total: total, percent: 0,
    });
    let mut sent: u64 = 0;
    let mut last_pct: u8 = u8::MAX;
    for chunk in fw.chunks(SEND_CHUNK) {
        if let Err(e) = stream.write_all(chunk) {
            let _ = tx.send(WorkerMsg::Error {
                error_kind: "send-failed".into(),
                message: format!("body write: {e}"),
            });
            return;
        }
        sent += chunk.len() as u64;
        let pct = if total == 0 { 100 } else { ((sent * 100) / total) as u8 };
        if pct != last_pct {
            last_pct = pct;
            let _ = tx.send(WorkerMsg::Progress {
                phase: "sending".into(), bytes_sent: sent, bytes_total: total, percent: pct,
            });
        }
    }
    let _ = stream.flush();
    // Half-close the write side — the device reads the body until EOF.
    let _ = stream.shutdown(Shutdown::Write);

    let _ = tx.send(WorkerMsg::Progress {
        phase: "awaiting-ack".into(), bytes_sent: total, bytes_total: total, percent: 100,
    });

    // Response frame: [status u8][msg_len u16 LE][msg utf-8].
    let mut status = [0u8; 1];
    if let Err(e) = stream.read_exact(&mut status) {
        // No response before the socket closed/timed out — the device
        // may have rebooted (or rolled back) without acking.
        let _ = tx.send(WorkerMsg::Error {
            error_kind: "reboot-timeout".into(),
            message: format!("no ack frame: {e}"),
        });
        return;
    }
    let mut len_buf = [0u8; 2];
    if let Err(e) = stream.read_exact(&mut len_buf) {
        let _ = tx.send(WorkerMsg::Error {
            error_kind: "response-malformed".into(),
            message: format!("ack length read: {e}"),
        });
        return;
    }
    let msg_len = u16::from_le_bytes(len_buf) as usize;
    let mut msg_buf = vec![0u8; msg_len];
    if msg_len > 0 {
        if let Err(e) = stream.read_exact(&mut msg_buf) {
            let _ = tx.send(WorkerMsg::Error {
                error_kind: "response-malformed".into(),
                message: format!("ack message read: {e}"),
            });
            return;
        }
    }
    let msg = String::from_utf8_lossy(&msg_buf).to_string();
    let duration_ms = start.elapsed().as_millis() as u64;

    if status[0] == WIRE_STATUS_OK {
        append_deploy_log(&params, 1, "done", &sha_hex, duration_ms);
        // The device reboots ~2 s after acking; we report `rebooting`
        // and stop here (beacon-confirm of the new sha is F5b).
        let _ = tx.send(WorkerMsg::Progress {
            phase: "rebooting".into(), bytes_sent: total, bytes_total: total, percent: 100,
        });
        let _ = tx.send(WorkerMsg::Done { artefact_sha256: sha_hex, duration_ms });
    } else {
        let error_kind = classify_device_error(&msg);
        append_deploy_log(&params, 1, "error", &sha_hex, duration_ms);
        let _ = tx.send(WorkerMsg::Error { error_kind: error_kind.into(), message: msg });
    }
}

/// Map a device error reply's leading CODE token to a kebab `error_kind`, per
/// the OTA reply-status contract (`specifications/OTA-REPLY-STATUS-CONTRACT.md`)
/// — the vocabulary hive's no_std embassy-net receiver emits. Unknown codes
/// fall back to `device-error`.
fn classify_device_error(msg: &str) -> &'static str {
    match msg.split_whitespace().next().unwrap_or("") {
        "SHA_MISMATCH" => "sha-mismatch",
        "TOO_BIG" => "too-big",
        "BAD_MAGIC" => "bad-magic",
        "WRITE_FAIL" => "write-fail",
        "NO_SLOT" => "no-slot",
        "PREAMBLE" => "preamble",
        "SHORT" => "short",
        _ => "device-error",
    }
}

/// Append one §6.5 audit line to `deploy_log.jsonl`. Best-effort: a
/// logging failure must not fail the push.
fn append_deploy_log(params: &OtaPushParams, attempt: u32, phase: &str, sha: &str, duration_ms: u64) {
    let Some(path) = params.log_path.as_ref() else { return; };
    let line = serde_json::json!({
        "ts": crate::roster::now_iso8601(),
        "batch_id": params.batch_id,
        "slot_id": params.slot_id,
        "attempt": attempt,
        "phase": phase,
        "artefact_sha256": sha,
        "duration_ms": duration_ms,
    });
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    /// Captured request the fake device read off the wire.
    type Capture = Arc<Mutex<Option<Vec<u8>>>>;

    /// Spawn a one-shot fake device on a loopback port. It reads the
    /// full request (until the client half-closes), stashes it in
    /// `capture`, then replies with `[status][len u16 LE][msg]`.
    fn spawn_device(status: u8, msg: &'static str, capture: Capture) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut all = Vec::new();
                let _ = stream.read_to_end(&mut all); // blocks until client FIN
                *capture.lock().unwrap() = Some(all);
                let mut resp = vec![status];
                let m = msg.as_bytes();
                resp.extend_from_slice(&(m.len() as u16).to_le_bytes());
                resp.extend_from_slice(m);
                let _ = stream.write_all(&resp);
                let _ = stream.flush();
            }
        });
        port
    }

    fn params_for(port: u16, fw: &std::path::Path) -> OtaPushParams {
        OtaPushParams {
            slot_id: "sensor:esp32-s3-xiao:8c1e3aaf".into(),
            batch_id: "b1f2c3".into(),
            target_ip: "127.0.0.1".into(),
            port,
            firmware_path: fw.to_path_buf(),
            log_path: None,
        }
    }

    fn write_fw(bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("firmware.bin");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    fn drain(p: &mut OtaPushPlugin, timeout: Duration) -> Vec<(u32, Vec<u8>)> {
        let (done, error) = (p.hash_done, p.hash_error);
        let mut out = Vec::new();
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some((h, payload)) = p.poll() {
                out.push((h, payload.to_vec()));
                if h == done || h == error { break; }
                continue;
            }
            thread::sleep(Duration::from_millis(10));
        }
        out
    }

    #[test]
    fn unknown_command_errors() {
        let mut p = OtaPushPlugin::new(0, Arc::new(Mutex::new(None)));
        match p.execute(0xAA, &[]) {
            PluginResult::Error(e) => assert_eq!(e.code, ERR_UNKNOWN_COMMAND),
            _ => panic!(),
        }
    }

    #[test]
    fn missing_params_errors() {
        let mut p = OtaPushPlugin::new(0, Arc::new(Mutex::new(None)));
        match p.execute(CMD_START, &[]) {
            PluginResult::Error(e) => assert_eq!(e.code, ERR_NO_PARAMS),
            _ => panic!(),
        }
    }

    #[test]
    fn successful_push_round_trips_to_done() {
        let fw_bytes = vec![0xABu8; 100_000]; // spans multiple SEND_CHUNKs
        let (_dir, fw_path) = write_fw(&fw_bytes);
        let capture: Capture = Arc::new(Mutex::new(None));
        let port = spawn_device(WIRE_STATUS_OK, "OK", capture.clone());

        let slot = Arc::new(Mutex::new(Some(params_for(port, &fw_path))));
        let mut p = OtaPushPlugin::new(0, slot);
        assert!(matches!(p.execute(CMD_START, &[]), PluginResult::Ok(_)));

        let events = drain(&mut p, Duration::from_secs(5));
        let progress = events.iter().filter(|(h, _)| *h == p.hash_progress).count();
        let done = events.iter().filter(|(h, _)| *h == p.hash_done).count();
        assert!(progress >= 3, "expected connecting/sending/awaiting-ack/rebooting, got {progress}");
        assert_eq!(done, 1, "expected exactly one done event: {events:?}");

        // Done payload carries the computed sha + correlation ids.
        let (_, done_payload) = events.iter().find(|(h, _)| *h == p.hash_done).unwrap();
        let v: serde_json::Value = serde_json::from_slice(done_payload).unwrap();
        let expected_sha = hex::encode(Sha256::digest(&fw_bytes));
        assert_eq!(v["artefact_sha256"], expected_sha);
        assert_eq!(v["batch_id"], "b1f2c3");
        assert_eq!(v["slot_id"], "sensor:esp32-s3-xiao:8c1e3aaf");
    }

    #[test]
    fn wire_request_is_byte_exact() {
        let fw_bytes: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let (_dir, fw_path) = write_fw(&fw_bytes);
        let capture: Capture = Arc::new(Mutex::new(None));
        let port = spawn_device(WIRE_STATUS_OK, "OK", capture.clone());

        let slot = Arc::new(Mutex::new(Some(params_for(port, &fw_path))));
        let mut p = OtaPushPlugin::new(0, slot);
        assert!(matches!(p.execute(CMD_START, &[]), PluginResult::Ok(_)));
        let _ = drain(&mut p, Duration::from_secs(5));

        // Give the device thread a beat to stash the capture.
        let deadline = Instant::now() + Duration::from_secs(2);
        while capture.lock().unwrap().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let req = capture.lock().unwrap().clone().expect("device captured a request");

        // [0x01][size u32 LE][sha 32][body…]
        assert_eq!(req[0], WIRE_CMD_START, "command byte");
        let size = u32::from_le_bytes([req[1], req[2], req[3], req[4]]);
        assert_eq!(size as usize, fw_bytes.len(), "size is u32 LE of firmware len");
        let sha_on_wire = &req[5..37];
        assert_eq!(sha_on_wire, Sha256::digest(&fw_bytes).as_slice(), "sha256 is 32 raw bytes");
        assert_eq!(&req[37..], &fw_bytes[..], "body is the raw firmware, streamed verbatim");
        assert_eq!(req.len(), 37 + fw_bytes.len(), "no trailing length terminator");
    }

    #[test]
    fn device_error_code_maps_to_kind() {
        // hive's receiver emits the CODE vocabulary (OTA-REPLY-STATUS-CONTRACT):
        // status 0x01 + "SHA_MISMATCH <detail>" → error_kind "sha-mismatch".
        let (_dir, fw_path) = write_fw(&[0u8; 1024]);
        let capture: Capture = Arc::new(Mutex::new(None));
        let port = spawn_device(0x01, "SHA_MISMATCH computed!=preamble", capture);

        let slot = Arc::new(Mutex::new(Some(params_for(port, &fw_path))));
        let mut p = OtaPushPlugin::new(0, slot);
        assert!(matches!(p.execute(CMD_START, &[]), PluginResult::Ok(_)));

        let events = drain(&mut p, Duration::from_secs(5));
        let (_, err_payload) = events.iter().find(|(h, _)| *h == p.hash_error).expect("error event");
        let v: serde_json::Value = serde_json::from_slice(err_payload).unwrap();
        assert_eq!(v["error_kind"], "sha-mismatch");
        assert_eq!(v["message"], "SHA_MISMATCH computed!=preamble");
    }

    #[test]
    fn classify_device_error_vocabulary() {
        assert_eq!(classify_device_error("SHA_MISMATCH x"), "sha-mismatch");
        assert_eq!(classify_device_error("TOO_BIG 1500000>1500000"), "too-big");
        assert_eq!(classify_device_error("BAD_MAGIC"), "bad-magic");
        assert_eq!(classify_device_error("WRITE_FAIL errno=5"), "write-fail");
        assert_eq!(classify_device_error("NO_SLOT"), "no-slot");
        assert_eq!(classify_device_error("PREAMBLE short read"), "preamble");
        assert_eq!(classify_device_error("SHORT"), "short");
        assert_eq!(classify_device_error("weird"), "device-error");
        assert_eq!(classify_device_error(""), "device-error");
    }

    #[test]
    fn unreachable_target_errors() {
        // Bind then drop to obtain a (very likely) closed port.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let (_dir, fw_path) = write_fw(&[0u8; 16]);
        let slot = Arc::new(Mutex::new(Some(params_for(port, &fw_path))));
        let mut p = OtaPushPlugin::new(0, slot);
        assert!(matches!(p.execute(CMD_START, &[]), PluginResult::Ok(_)));

        let events = drain(&mut p, Duration::from_secs(8));
        let (_, err_payload) = events.iter().find(|(h, _)| *h == p.hash_error).expect("error event");
        let v: serde_json::Value = serde_json::from_slice(err_payload).unwrap();
        assert_eq!(v["error_kind"], "unreachable");
    }

    #[test]
    fn missing_artefact_errors() {
        let capture: Capture = Arc::new(Mutex::new(None));
        let port = spawn_device(WIRE_STATUS_OK, "OK", capture);
        let mut params = params_for(port, std::path::Path::new("/nonexistent/firmware.bin"));
        params.log_path = None;
        let slot = Arc::new(Mutex::new(Some(params)));
        let mut p = OtaPushPlugin::new(0, slot);
        assert!(matches!(p.execute(CMD_START, &[]), PluginResult::Ok(_)));

        let events = drain(&mut p, Duration::from_secs(3));
        let (_, err_payload) = events.iter().find(|(h, _)| *h == p.hash_error).expect("error event");
        let v: serde_json::Value = serde_json::from_slice(err_payload).unwrap();
        assert_eq!(v["error_kind"], "artefact-missing");
    }

    #[test]
    fn deploy_log_appended_on_success() {
        let (_dir, fw_path) = write_fw(&[0x11u8; 2048]);
        let log_dir = tempfile::tempdir().unwrap();
        let log_path = log_dir.path().join("deploy_log.jsonl");
        let capture: Capture = Arc::new(Mutex::new(None));
        let port = spawn_device(WIRE_STATUS_OK, "OK", capture);

        let mut params = params_for(port, &fw_path);
        params.log_path = Some(log_path.clone());
        let slot = Arc::new(Mutex::new(Some(params)));
        let mut p = OtaPushPlugin::new(0, slot);
        assert!(matches!(p.execute(CMD_START, &[]), PluginResult::Ok(_)));
        let _ = drain(&mut p, Duration::from_secs(5));

        // The append happens on the worker thread just before Done; poll
        // returns Done after the write, so the file is present now.
        let contents = std::fs::read_to_string(&log_path).expect("deploy_log written");
        let line: serde_json::Value = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert_eq!(line["batch_id"], "b1f2c3");
        assert_eq!(line["slot_id"], "sensor:esp32-s3-xiao:8c1e3aaf");
        assert_eq!(line["phase"], "done");
        assert_eq!(line["attempt"], 1);
    }

    #[test]
    fn busy_when_session_running() {
        let fw_bytes = vec![0u8; 64];
        let (_dir, fw_path) = write_fw(&fw_bytes);
        let capture: Capture = Arc::new(Mutex::new(None));
        let port = spawn_device(WIRE_STATUS_OK, "OK", capture);
        let slot = Arc::new(Mutex::new(Some(params_for(port, &fw_path))));
        let mut p = OtaPushPlugin::new(0, slot.clone());
        assert!(matches!(p.execute(CMD_START, &[]), PluginResult::Ok(_)));
        // Second start before draining → busy.
        *slot.lock().unwrap() = Some(params_for(port, &fw_path));
        match p.execute(CMD_START, &[]) {
            PluginResult::Error(e) => assert_eq!(e.code, ERR_BUSY),
            _ => panic!("expected ERR_BUSY"),
        }
    }
}
