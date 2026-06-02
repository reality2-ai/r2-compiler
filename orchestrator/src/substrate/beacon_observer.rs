//! `beacon-observer` substrate component — Linux BLE scanner for
//! R2-BEACON frames per R2-BEACON §5-7.
//!
//! Background thread runs `btleplug`'s scan continuously; the AD
//! payloads with our Company ID + R2 magic are decoded via
//! [`beacon_parser`] and converted into `r2.composer.device.beacon_observed`
//! events. The plugin's `poll()` drains them onto the bus.
//!
//! ## What a beacon CAN and CANNOT tell you
//!
//! R2-BEACON §1 / §10.2 are explicit: **"a beacon is a signpost, not a
//! passport"**. It carries:
//!
//! - class_hash (FNV-1a-32 of the device's class string)
//! - rotating RBID (re-derived every 15 min)
//! - flags byte (provisioning / mcu_mode / mobile)
//! - tx_power + anti_collision token
//!
//! It does **NOT** carry the device's Ed25519 pubkey or any TG
//! identifier. That's by design. The device_pk arrives later, via the
//! L2CAP CoC `JoinRequest` handshake (R2-PROVISION). The follow-on
//! `provision-handshake` substrate component (F4b) will close that
//! loop and emit `r2.composer.device.identity_observed{device_pk}` —
//! which is what the existing `Provision` sentant (F3) needs to mint
//! a DeviceCertificate.
//!
//! ## Mirror of [`crate::composer::usb_watcher`]
//!
//! Same structural pattern as the USB watcher: own thread, own tokio
//! runtime (btleplug needs one), shared snapshot Arc that the WS
//! handler can read for late-connect replay, queued event vector
//! drained by `poll()`.
//!
//! ## Platform
//!
//! Linux-primary (BlueZ via btleplug's bluez backend). macOS works for
//! development (CoreBluetooth backend) but the L2CAP CoC follow-up
//! (F4b) will be Linux-only because bluer is Linux-only. Windows
//! untested.
//!
//! ## Permissions
//!
//! BlueZ unprivileged scan needs either `cap_net_admin` on the
//! orchestrator binary, OR membership in the `bluetooth` group. If
//! scanning fails with permission error, the plugin logs a calm
//! warning and gives up — it does NOT crash the hive.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};
use serde::Serialize;

use crate::substrate::beacon_parser::{self, BeaconFlags, COMPANY_ID, R2_BEACON_MAGIC};

/// Shared snapshot of currently-known beacons (keyed by BLE address).
/// The WS handler can read this when a new client connects to replay
/// the current beacon set — same trick as [`crate::composer::usb_watcher`].
pub type BeaconSnapshot = Arc<Mutex<Vec<BeaconObservation>>>;

/// One observed R2-BEACON, serialised onto the bus as the payload of
/// `r2.composer.device.beacon_observed`.
#[derive(Debug, Clone, Serialize)]
pub struct BeaconObservation {
    /// BLE peripheral address (the orchestrator's future provision-
    /// handshake will use this to open an L2CAP CoC on PSM 0x00D2).
    pub ble_addr: String,
    /// 8-byte RBID hex string (rotates per R2-BEACON §6.1 — same
    /// physical device may appear under multiple RBIDs over time).
    pub rbid: String,
    /// 4-byte class hash, big-endian, formatted as `0xHHHHHHHH`. The
    /// device's class string (e.g. `ai.reality2.device.sensor`) is
    /// NOT on the wire — operators look it up via the catalogue if
    /// they want the human-readable form.
    pub class_hash: String,
    /// `provisioning` flag (R2-BEACON §7.2 bit 5): set ⇒ the device
    /// has NOT yet been enrolled into any TG and is open to forming
    /// or joining one. This is the discriminator that says "fresh
    /// device, ready for the provision handshake".
    pub provisioning: bool,
    /// `mcu_mode` flag (§7.2 bit 4): MCU-only emitter, SBC sleeping.
    pub mcu_mode: bool,
    /// `mobile` flag (§7.2 bit 3): device in motion.
    pub mobile: bool,
    /// Advertised TX power, dBm. Combine with `rssi` to estimate
    /// distance (`path_loss = tx_power - rssi`).
    pub tx_power_dbm: i8,
    /// Last-observed RSSI in dBm during the scan window.
    pub rssi: i16,
    /// Per-provisioning-session disambiguator (R2-BEACON §5.3.1).
    /// Lets the operator tell apart two virgin devices in range
    /// when their RBIDs and class hashes match.
    pub anti_collision: u16,
    /// ISO 8601 UTC of when this BLE address was first observed in
    /// the current orchestrator session.
    pub first_seen: String,
    /// ISO 8601 UTC of the most recent observation.
    pub last_seen: String,
}

/// Internal state per BLE address — keeps a fingerprint of the last-
/// emitted observation so we don't spam the bus on every scan tick
/// when nothing meaningful changed.
struct InternalEntry {
    obs: BeaconObservation,
    /// Hash of the bytes we care about (rbid + flags + class_hash + ac).
    /// If this changes between observations, we emit a fresh event.
    fingerprint: u64,
    last_observed: Instant,
}

pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

pub struct BeaconObserverPlugin {
    id: PluginId,
    /// Hash of `r2.composer.device.beacon_observed` event name.
    hash_observed: u32,
    /// Hash of `r2.composer.device.beacon_lost` event name.
    hash_lost: u32,
    /// Output buffer for poll() — reused across calls.
    out_buf: Vec<u8>,
    /// Events queued by the background thread; drained by poll().
    pending: Arc<Mutex<Vec<(u32, Vec<u8>)>>>,
    /// Public snapshot for late-connect WS replay.
    #[allow(dead_code)] // exposed via getter; held here for lifecycle
    snapshot: BeaconSnapshot,
}

impl BeaconObserverPlugin {
    /// Construct + spawn the background scanner. `snapshot` is the
    /// shared Arc the WS layer reads for replay.
    pub fn new(id: PluginId, snapshot: BeaconSnapshot) -> Self {
        let hash_observed = r2_fnv::fnv1a_32(b"r2.composer.device.beacon_observed");
        let hash_lost     = r2_fnv::fnv1a_32(b"r2.composer.device.beacon_lost");
        let pending: Arc<Mutex<Vec<(u32, Vec<u8>)>>> = Arc::new(Mutex::new(Vec::new()));
        spawn_scanner_thread(snapshot.clone(), pending.clone(), hash_observed, hash_lost);
        Self {
            id,
            hash_observed,
            hash_lost,
            out_buf: Vec::with_capacity(512),
            pending,
            snapshot,
        }
    }
}

impl Plugin for BeaconObserverPlugin {
    fn execute(&mut self, _command: PluginCommand, _data: &[u8]) -> PluginResult {
        // No commands in F4 — the scanner runs continuously in the
        // background. Future: STOP / START / FORCE_SCAN.
        PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "no commands"))
    }
    fn name(&self) -> &str { "beacon-observer" }
    fn id(&self) -> PluginId { self.id }

    fn poll(&mut self) -> Option<(u32, &[u8])> {
        let mut pending = self.pending.lock().unwrap();
        if pending.is_empty() { return None }
        let (hash, payload) = pending.remove(0);
        self.out_buf = payload;
        Some((hash, &self.out_buf))
    }
}

// ── Background scanner thread ────────────────────────────────────────

/// How often we restart the btleplug scan window. btleplug doesn't
/// stream advertisements continuously across `stop_scan` calls — we
/// cycle to refresh the peripheral list and pick up new RBIDs.
const SCAN_WINDOW_SECS: u64 = 5;

/// Inter-scan gap, gives the kernel a moment + bounds CPU.
const SCAN_GAP_MILLIS: u64 = 250;

/// How long without an observation before we declare a beacon "lost".
/// R2-BEACON §6.1 says RBIDs rotate every 15 minutes; we set the
/// lost-window slightly longer than that to avoid false "lost" events
/// on RBID flips (where the device IS still present, just under a new
/// RBID — but in our snapshot we key by BLE address which is stable,
/// so a real loss is real).
const LOST_AFTER_SECS: u64 = 60;

fn spawn_scanner_thread(
    snapshot: BeaconSnapshot,
    pending: Arc<Mutex<Vec<(u32, Vec<u8>)>>>,
    hash_observed: u32,
    hash_lost: u32,
) {
    std::thread::Builder::new()
        .name("beacon-observer".into())
        .spawn(move || {
            // btleplug needs a tokio runtime. Build our own — the
            // orchestrator's axum runtime lives on a different thread
            // and we don't want to bind to it.
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("beacon-observer: tokio runtime build failed: {e}");
                    return;
                }
            };
            rt.block_on(scanner_loop(snapshot, pending, hash_observed, hash_lost));
        })
        .expect("spawn beacon-observer thread");
}

async fn scanner_loop(
    snapshot: BeaconSnapshot,
    pending: Arc<Mutex<Vec<(u32, Vec<u8>)>>>,
    hash_observed: u32,
    hash_lost: u32,
) {
    // Internal state: BLE address → last observation + fingerprint.
    let mut seen: HashMap<String, InternalEntry> = HashMap::new();

    use btleplug::api::Manager as _;
    let manager = match btleplug::platform::Manager::new().await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                "beacon-observer: btleplug manager init failed: {e} \
                 (BLE adapter missing or permission denied — substrate disabled)"
            );
            return;
        }
    };
    let central = {
        let adapters = match manager.adapters().await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(
                    "beacon-observer: enumerate adapters failed: {e} (substrate disabled)"
                );
                return;
            }
        };
        match adapters.into_iter().next() {
            Some(c) => c,
            None => {
                tracing::warn!(
                    "beacon-observer: no BLE adapter found (substrate disabled — \
                     install BlueZ + ensure user is in 'bluetooth' group)"
                );
                return;
            }
        }
    };
    tracing::info!("beacon-observer: scanning every {SCAN_WINDOW_SECS}s");

    use btleplug::api::{Central, Peripheral as _, ScanFilter};
    loop {
        if let Err(e) = central.start_scan(ScanFilter::default()).await {
            tracing::warn!("beacon-observer: start_scan failed: {e}");
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }
        tokio::time::sleep(Duration::from_secs(SCAN_WINDOW_SECS)).await;
        let _ = central.stop_scan().await;

        let peripherals = match central.peripherals().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("beacon-observer: peripherals() failed: {e}");
                continue;
            }
        };

        let now = std::time::SystemTime::now();
        let now_iso = iso8601_utc(now);
        let now_inst = Instant::now();
        let mut observed_this_round: Vec<String> = Vec::new();

        for p in peripherals {
            let props = match p.properties().await {
                Ok(Some(props)) => props,
                _ => continue,
            };
            // R2 beacons live under our Company ID with the magic byte
            // as the first byte of the manufacturer-data payload.
            let mfr = match props.manufacturer_data.get(&COMPANY_ID) {
                Some(data) => data.clone(),
                None => continue,
            };
            if mfr.is_empty() || mfr[0] != R2_BEACON_MAGIC { continue; }
            let ad = beacon_parser::reconstruct_ad(&mfr);
            let parsed = match beacon_parser::parse_legacy_beacon(&ad) {
                Ok(b) => b,
                Err(e) => {
                    tracing::debug!(
                        "beacon-observer: skipping malformed beacon from {}: {:?}",
                        p.address(), e
                    );
                    continue;
                }
            };
            let addr = p.address().to_string();
            let rssi = props.rssi.unwrap_or(0) as i16;
            let obs = BeaconObservation {
                ble_addr: addr.clone(),
                rbid: hex::encode(parsed.rbid),
                class_hash: format!("0x{:08X}", u32::from_be_bytes(parsed.class_hash)),
                provisioning: parsed.flags.provisioning,
                mcu_mode: parsed.flags.mcu_mode,
                mobile: parsed.flags.mobile,
                tx_power_dbm: parsed.tx_power,
                rssi,
                anti_collision: parsed.anti_collision,
                first_seen: seen.get(&addr)
                    .map(|e| e.obs.first_seen.clone())
                    .unwrap_or_else(|| now_iso.clone()),
                last_seen: now_iso.clone(),
            };
            let fp = fingerprint(&parsed.flags, &parsed.rbid, &parsed.class_hash, parsed.anti_collision);
            observed_this_round.push(addr.clone());
            let prior_fp = seen.get(&addr).map(|e| e.fingerprint);
            seen.insert(addr.clone(), InternalEntry {
                obs: obs.clone(),
                fingerprint: fp,
                last_observed: now_inst,
            });
            // Emit a beacon_observed event ONLY if it's new OR the
            // meaningful contents changed. The RSSI alone changing on
            // every scan would be noise.
            if prior_fp != Some(fp) {
                emit(&pending, hash_observed, &obs);
            }
        }

        // Sweep for losses: BLE addresses we previously had that
        // didn't show in this round AND haven't been seen in
        // LOST_AFTER_SECS. Emit beacon_lost + remove from snapshot.
        let mut to_drop: Vec<String> = Vec::new();
        for (addr, entry) in seen.iter() {
            if observed_this_round.contains(addr) { continue; }
            if now_inst.duration_since(entry.last_observed)
                >= Duration::from_secs(LOST_AFTER_SECS)
            {
                to_drop.push(addr.clone());
            }
        }
        for addr in to_drop {
            if let Some(entry) = seen.remove(&addr) {
                emit_lost(&pending, hash_lost, &entry.obs);
            }
        }

        // Update the snapshot for late-connect WS replay.
        {
            let mut snap = snapshot.lock().unwrap();
            snap.clear();
            for entry in seen.values() {
                snap.push(entry.obs.clone());
            }
        }

        tokio::time::sleep(Duration::from_millis(SCAN_GAP_MILLIS)).await;
    }
}

fn fingerprint(
    flags: &BeaconFlags,
    rbid: &[u8; 8],
    class_hash: &[u8; 4],
    anti_collision: u16,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    flags.encode().hash(&mut h);
    rbid.hash(&mut h);
    class_hash.hash(&mut h);
    anti_collision.hash(&mut h);
    h.finish()
}

fn emit(pending: &Arc<Mutex<Vec<(u32, Vec<u8>)>>>, hash: u32, obs: &BeaconObservation) {
    let bytes = serde_json::to_vec(obs).unwrap_or_default();
    pending.lock().unwrap().push((hash, bytes));
}

fn emit_lost(pending: &Arc<Mutex<Vec<(u32, Vec<u8>)>>>, hash: u32, obs: &BeaconObservation) {
    // Lost payload carries just the BLE addr + last-known class hash
    // — the webapp drops the row by address.
    let payload = serde_json::json!({
        "ble_addr": obs.ble_addr,
        "class_hash": obs.class_hash,
        "last_seen": obs.last_seen,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    pending.lock().unwrap().push((hash, bytes));
}

fn iso8601_utc(t: std::time::SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Minimal RFC3339 — same shape as elsewhere in the orchestrator.
    let days_since_epoch = secs / 86_400;
    let day_secs = secs % 86_400;
    let h = day_secs / 3600;
    let m = (day_secs % 3600) / 60;
    let s = day_secs % 60;
    // Crude date — close enough for ordering, not for legal documents.
    // The orchestrator's roster.rs uses the same shape via `now_iso8601()`.
    let (y, mo, d) = days_to_ymd(days_since_epoch as i64);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Naïve days-since-epoch → (year, month, day) conversion. Good for the
/// 21st century; not date-of-publication accurate. Mirrors the same
/// approximation used elsewhere in the orchestrator to keep the log
/// shape consistent.
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    // 1970-01-01 was day 0.
    let mut y = 1970i32;
    let mut d = days;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let year_days = if leap { 366 } else { 365 };
        if d < year_days { break; }
        d -= year_days;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let mlen = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut mo = 1u32;
    for &len in mlen.iter() {
        if d < len { break; }
        d -= len;
        mo += 1;
    }
    (y, mo, (d + 1) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_stable_across_calls() {
        let flags = BeaconFlags::decode(0b0010_0000);
        let rbid = [1, 2, 3, 4, 5, 6, 7, 8];
        let ch = [0x43, 0x89, 0x5E, 0x89];
        let ac = 12345;
        assert_eq!(fingerprint(&flags, &rbid, &ch, ac), fingerprint(&flags, &rbid, &ch, ac));
    }

    #[test]
    fn fingerprint_changes_on_flags_change() {
        let rbid = [0u8; 8];
        let ch = [0u8; 4];
        let f1 = BeaconFlags::decode(0b0010_0000); // provisioning
        let f2 = BeaconFlags::decode(0b0000_0000); // un-provisioning (enrolled now)
        assert_ne!(fingerprint(&f1, &rbid, &ch, 0), fingerprint(&f2, &rbid, &ch, 0));
    }

    #[test]
    fn iso8601_utc_format() {
        let t = std::time::UNIX_EPOCH + Duration::from_secs(1_780_704_000); // 2026-06-04
        let s = iso8601_utc(t);
        assert_eq!(s.len(), 20, "RFC3339 shape");
        assert!(s.ends_with("Z"));
        assert!(s.starts_with("2026-"));
    }
}
