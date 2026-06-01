//! `usb-watcher` plugin — Linux-poll-based detection of attached
//! ESP-family serial devices per SPEC-APIARY-FLASH §4.4.
//!
//! v0.1 (F2b scope): Linux only, poll-based on `/sys/class/tty/`.
//! macOS IOKit + Windows SetupAPI deferred (per SPEC-APIARY-FLASH
//! §12). On other platforms the watcher quietly emits nothing.
//!
//! ## Design
//!
//! - Background thread spawned in `new()` polls every 1500ms. For each
//!   tick it scans `/sys/class/tty/ttyACM*` / `ttyUSB*` and reads each
//!   port's vid/pid via the `device` symlink walked back to the USB
//!   device directory.
//! - The snapshot is diffed against the previous tick:
//!   - **new** ports → `WorkerMsg::Attached { … }`
//!   - **missing** ports → `WorkerMsg::Detached { … }`
//! - Carrier identification: each `UsbPort` is looked up against a
//!   table built once at plugin construction time from
//!   `catalogue/boards/*/board.toml [usb]` entries. The result is
//!   one of:
//!   - `carrier_guess: "<slug>", guess_confidence: "vid-pid"` (single match)
//!   - `carrier_guess: null, guess_confidence: "ambiguous"` + `candidates: [...]`
//!     (multiple matches — VID/PID is shared between e.g. ESP32-S3
//!     and ESP32-C6 native USB-Serial-JTAG; SPEC-APIARY-FLASH §4.1
//!     says `chip_id_probe` disambiguates, but the probe spawns
//!     esptool — deferred to a F2b follow-up)
//!   - `carrier_guess: null, guess_confidence: "unknown"` (no match)
//! - The plugin keeps a shared snapshot (`Arc<Mutex<Vec<UsbPort>>>`) so
//!   late-connecting WS clients can be replayed from current state.
//!
//! Calm-computing posture: attach/detach events are ambient. The
//! webapp surfaces a peripheral footer chip; never a modal.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};
use serde::Serialize;

/// One observed serial port + its USB identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsbPort {
    pub port: String,           // "/dev/ttyACM0"
    pub vid: u16,
    pub pid: u16,
    pub serial: Option<String>, // efuse MAC for ESP carriers (when readable)
    pub sysfs_path: String,
    /// Resolved at scan time from the catalogue lookup table.
    pub carrier_guess: Option<String>,
    pub guess_confidence: GuessConfidence,
    pub candidates: Vec<String>, // populated when confidence == Ambiguous
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GuessConfidence {
    VidPid,
    Ambiguous,
    Unknown,
}

/// Shared snapshot of currently-attached USB ports — held by both the
/// plugin's background thread AND by main.rs for the WS-connect replay.
pub type UsbSnapshot = Arc<Mutex<Vec<UsbPort>>>;

#[derive(Debug)]
enum WorkerMsg {
    Attached(UsbPort),
    Detached { port: String },
}

/// Polling cadence. 1.5s is the natural plug-it-in-and-look pace; calm
/// without being laggy. Exposed as a const so tests / future
/// configuration can override.
pub const POLL_INTERVAL: Duration = Duration::from_millis(1500);

pub const CMD_LIST: PluginCommand = 0x01;    // emit one attached per current port
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

pub struct UsbWatcherPlugin {
    id: PluginId,
    rx: mpsc::Receiver<WorkerMsg>,
    snapshot: UsbSnapshot,
    /// For the CMD_LIST replay path — when the orchestrator asks for a
    /// snapshot, we drain pending events first then synthesise Attached
    /// for each port still in the snapshot. Buffered through this Vec
    /// so poll() can return one event per call (same shape as other
    /// plugins' polls).
    pending_list: Vec<UsbPort>,
    hash_attached: u32,
    hash_detached: u32,
    out_buf: Vec<u8>,
}

impl UsbWatcherPlugin {
    /// Construct + spawn the background scanner. `catalogue_root` is
    /// where `boards/` lives (e.g. `<repo_root>/catalogue`). The
    /// catalogue is scanned ONCE at construction; new boards added at
    /// runtime require restart. Acceptable for v0.1.
    pub fn new(id: PluginId, catalogue_root: PathBuf, snapshot: UsbSnapshot) -> Self {
        let lookup = Self::load_catalogue_lookup(&catalogue_root);
        let (tx, rx) = mpsc::channel();
        let snapshot_thread = snapshot.clone();
        thread::spawn(move || {
            let mut previous: Vec<UsbPort> = Vec::new();
            loop {
                let raw = scan_sysfs_devices(Path::new("/sys"));
                let current: Vec<UsbPort> = raw
                    .into_iter()
                    .map(|p| classify(p, &lookup))
                    .collect();

                // Diff: emit Attached for ports in `current` not in `previous`.
                for port in &current {
                    if !previous.iter().any(|p| p.port == port.port) {
                        let _ = tx.send(WorkerMsg::Attached(port.clone()));
                    }
                }
                // Detached for ports in `previous` not in `current`.
                for port in &previous {
                    if !current.iter().any(|p| p.port == port.port) {
                        let _ = tx.send(WorkerMsg::Detached { port: port.port.clone() });
                    }
                }

                // Update the shared snapshot for late-WS-connect replay.
                if let Ok(mut s) = snapshot_thread.lock() {
                    *s = current.clone();
                }
                previous = current;

                thread::sleep(POLL_INTERVAL);
            }
        });

        Self {
            id,
            rx,
            snapshot,
            pending_list: Vec::new(),
            hash_attached: r2_fnv::fnv1a_32(b"r2.composer.usb.attached"),
            hash_detached: r2_fnv::fnv1a_32(b"r2.composer.usb.detached"),
            out_buf: Vec::with_capacity(512),
        }
    }

    /// Build the `{(vid, pid) → [carrier_slug]}` lookup table from
    /// `catalogue/boards/*/board.toml [usb]` entries.
    fn load_catalogue_lookup(catalogue_root: &Path) -> HashMap<(u16, u16), Vec<String>> {
        let mut out: HashMap<(u16, u16), Vec<String>> = HashMap::new();
        let boards_dir = catalogue_root.join("boards");
        let Ok(entries) = std::fs::read_dir(&boards_dir) else { return out };
        for entry in entries.flatten() {
            if !entry.path().is_dir() { continue }
            let toml_path = entry.path().join("board.toml");
            let Ok(raw) = std::fs::read_to_string(&toml_path) else { continue };
            let Ok(v) = raw.parse::<toml::Value>() else { continue };
            let Some(usb) = v.get("usb") else { continue };
            let vid = usb.get("vid").and_then(|x| x.as_integer()).map(|i| i as u16);
            let pid = usb.get("pid").and_then(|x| x.as_integer()).map(|i| i as u16);
            if let (Some(vid), Some(pid)) = (vid, pid) {
                let slug = entry.file_name().to_string_lossy().to_string();
                out.entry((vid, pid)).or_default().push(slug);
            }
        }
        out
    }

    fn pack<T: Serialize>(&mut self, v: &T) -> &[u8] {
        self.out_buf.clear();
        let _ = serde_json::to_writer(&mut self.out_buf, v);
        &self.out_buf
    }
}

impl Plugin for UsbWatcherPlugin {
    fn execute(&mut self, command: PluginCommand, _data: &[u8]) -> PluginResult {
        match command {
            CMD_LIST => {
                // Replay the current snapshot via subsequent poll()
                // calls. Each port becomes one Attached event.
                if let Ok(s) = self.snapshot.lock() {
                    self.pending_list.extend(s.iter().cloned());
                }
                PluginResult::Ok(PluginResponse::empty())
            }
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "unknown command byte")),
        }
    }
    fn name(&self) -> &str { "usb-watcher" }
    fn id(&self) -> PluginId { self.id }

    fn poll(&mut self) -> Option<(u32, &[u8])> {
        // Cache hashes before borrowing self mutably for pack().
        let (hash_attached, hash_detached) = (self.hash_attached, self.hash_detached);
        // 1. CMD_LIST replay events take priority — drain the buffered
        //    snapshot first so the requesting webapp sees the current
        //    state before any live attach/detach.
        if let Some(port) = self.pending_list.pop() {
            let payload = self.pack(&port);
            return Some((hash_attached, payload));
        }
        // 2. Drain one live event per call (preserves order).
        let msg = self.rx.try_recv().ok()?;
        match msg {
            WorkerMsg::Attached(port) => {
                let payload = self.pack(&port);
                Some((hash_attached, payload))
            }
            WorkerMsg::Detached { port } => {
                let payload = self.pack(&serde_json::json!({ "port": port }));
                Some((hash_detached, payload))
            }
        }
    }
}

/// Snapshot the currently-attached serial ports under `sysfs_root` —
/// pure function for testability. In prod called with `Path::new("/sys")`.
pub fn scan_sysfs_devices(sysfs_root: &Path) -> Vec<UsbPortRaw> {
    let tty_class = sysfs_root.join("class/tty");
    let Ok(entries) = std::fs::read_dir(&tty_class) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_serial_tty(&name) { continue }
        let device_link = entry.path().join("device");
        let interface_dir = match std::fs::canonicalize(&device_link) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let usb_device = match interface_dir.parent() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        let vid = read_hex_file(&usb_device.join("idVendor"));
        let pid = read_hex_file(&usb_device.join("idProduct"));
        if let (Some(vid), Some(pid)) = (vid, pid) {
            let serial = std::fs::read_to_string(usb_device.join("serial"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            out.push(UsbPortRaw {
                port: format!("/dev/{name}"),
                vid, pid, serial,
                sysfs_path: usb_device.display().to_string(),
            });
        }
    }
    out
}

/// Pre-classification raw entry — just the sysfs facts. `classify()`
/// turns this into a `UsbPort` with `carrier_guess` populated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbPortRaw {
    pub port: String,
    pub vid: u16,
    pub pid: u16,
    pub serial: Option<String>,
    pub sysfs_path: String,
}

fn is_serial_tty(name: &str) -> bool {
    name.starts_with("ttyACM") || name.starts_with("ttyUSB")
}

fn read_hex_file(p: &Path) -> Option<u16> {
    let raw = std::fs::read_to_string(p).ok()?;
    u16::from_str_radix(raw.trim().trim_start_matches("0x"), 16).ok()
}

/// Match the raw entry against the catalogue lookup table.
pub fn classify(
    raw: UsbPortRaw,
    lookup: &HashMap<(u16, u16), Vec<String>>,
) -> UsbPort {
    let candidates = lookup.get(&(raw.vid, raw.pid)).cloned().unwrap_or_default();
    let (carrier_guess, guess_confidence) = match candidates.len() {
        0 => (None, GuessConfidence::Unknown),
        1 => (Some(candidates[0].clone()), GuessConfidence::VidPid),
        _ => (None, GuessConfidence::Ambiguous),
    };
    UsbPort {
        port: raw.port,
        vid: raw.vid,
        pid: raw.pid,
        serial: raw.serial,
        sysfs_path: raw.sysfs_path,
        carrier_guess,
        guess_confidence,
        candidates: if matches!(guess_confidence, GuessConfidence::Ambiguous)
            { candidates } else { Vec::new() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup_with(entries: &[((u16, u16), Vec<&str>)]) -> HashMap<(u16, u16), Vec<String>> {
        let mut m = HashMap::new();
        for ((v, p), slugs) in entries {
            m.insert((*v, *p), slugs.iter().map(|s| s.to_string()).collect());
        }
        m
    }

    fn raw(port: &str, vid: u16, pid: u16) -> UsbPortRaw {
        UsbPortRaw {
            port: port.into(),
            vid, pid,
            serial: None,
            sysfs_path: format!("/sys/devices/test/{port}"),
        }
    }

    #[test]
    fn classify_unknown_when_no_match() {
        let lk = lookup_with(&[]);
        let p = classify(raw("/dev/ttyACM0", 0x1234, 0x5678), &lk);
        assert_eq!(p.carrier_guess, None);
        assert_eq!(p.guess_confidence, GuessConfidence::Unknown);
        assert!(p.candidates.is_empty());
    }

    #[test]
    fn classify_vid_pid_when_single_match() {
        let lk = lookup_with(&[((0x303a, 0x1001), vec!["esp32-s3-xiao"])]);
        let p = classify(raw("/dev/ttyACM0", 0x303a, 0x1001), &lk);
        assert_eq!(p.carrier_guess.as_deref(), Some("esp32-s3-xiao"));
        assert_eq!(p.guess_confidence, GuessConfidence::VidPid);
    }

    #[test]
    fn classify_ambiguous_when_multiple_matches() {
        // ESP32-S3 + ESP32-C6 native USB-Serial-JTAG share VID/PID.
        let lk = lookup_with(&[(
            (0x303a, 0x1001),
            vec!["esp32-s3-devkitc", "esp32-s3-xiao", "esp32-c6-dfr1117"],
        )]);
        let p = classify(raw("/dev/ttyACM0", 0x303a, 0x1001), &lk);
        assert_eq!(p.carrier_guess, None);
        assert_eq!(p.guess_confidence, GuessConfidence::Ambiguous);
        assert_eq!(p.candidates.len(), 3);
    }

    #[test]
    fn read_hex_file_parses_sysfs_hex() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("idVendor");
        std::fs::write(&p, "303a\n").unwrap();
        assert_eq!(read_hex_file(&p), Some(0x303a));
    }

    #[test]
    fn read_hex_file_handles_missing_or_bad() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_hex_file(&dir.path().join("missing")), None);
        std::fs::write(dir.path().join("bad"), "not-hex").unwrap();
        assert_eq!(read_hex_file(&dir.path().join("bad")), None);
    }

    #[test]
    fn is_serial_tty_matches_acm_and_usb() {
        assert!(is_serial_tty("ttyACM0"));
        assert!(is_serial_tty("ttyACM12"));
        assert!(is_serial_tty("ttyUSB0"));
        assert!(!is_serial_tty("tty0"));
        assert!(!is_serial_tty("ttyS0"));
        assert!(!is_serial_tty("console"));
    }

    #[test]
    fn scan_returns_empty_on_missing_sysfs_root() {
        let dir = tempfile::tempdir().unwrap();
        // No `class/tty` subdir.
        let out = scan_sysfs_devices(dir.path());
        assert!(out.is_empty());
    }

    #[test]
    fn load_catalogue_lookup_reads_real_board_tomls() {
        // Use the live repo catalogue — should find at least one ESP carrier.
        let lookup = UsbWatcherPlugin::load_catalogue_lookup(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../catalogue"),
        );
        // VID 0x303a + PID 0x1001 should match the three ESP carriers
        // (devkitc, xiao, dfr1117).
        let espressif = lookup.get(&(0x303a, 0x1001));
        assert!(espressif.is_some(),
            "expected 0x303a/0x1001 to be in catalogue lookup, got {:?}",
            lookup.keys().collect::<Vec<_>>());
        let slugs = espressif.unwrap();
        assert!(slugs.iter().any(|s| s.contains("esp32")),
            "expected at least one esp32-* carrier, got {slugs:?}");
    }
}
