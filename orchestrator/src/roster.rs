//! Apiary device roster — per-device records under
//! `apiaries/<name>/devices/roster.toml`, with the three-orthogonal-axes
//! state model from SPEC-APIARY-FLASH §1.4 + §2.
//!
//! F1 scope (this module): the schema, the state-machine transition
//! table, atomic write-temp-fsync-rename-fsync-dir persistence, and
//! a minimal API the `Roster` sentant uses (`load` / `save` /
//! `apply_transition`). Full state coverage (placeholder → built →
//! flashed_pending_pk → enrolled → reachable / unreachable / revoked /
//! retired) follows in F2..F6 chunks; F1 exercises the
//! `placeholder` state plus the SHAPE so subsequent chunks plug in.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── Public state types — the JSON shape the webapp renders ────────────

/// One device slot in the roster — both the operator-declared intent
/// (state=`placeholder`, no `device_pk` yet) AND the post-flash /
/// post-enrolment record. The same row carries through the lifecycle;
/// `slot_id` is the immutable primary key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRow {
    pub slot_id: String,
    pub role: String,
    pub ensemble: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name_alias: String,

    /// Lifecycle state (per SPEC-APIARY-FLASH §2.1).
    pub state: String,
    /// WiFi-creds health (per §1.4).
    #[serde(default = "default_unknown")]
    pub provision_state: String,
    /// TG-certificate validity (per §1.4).
    #[serde(default = "default_unknown")]
    pub cert_status: String,

    /// Filled at first beacon — Ed25519 public key in 0x<64-hex>.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_pk: Option<String>,
    /// RBID = FNV-1a-32 of `device_pk` bytes — set when `device_pk` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rbid: Option<String>,
    /// ESP efuse MAC at flash time — ESP-carrier-specific (§4.5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac_at_flash: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware_ver: Option<String>,

    pub enrolled: String,
    #[serde(default)]
    pub last_seen: String,

    /// Append-only audit trail (per §2.4).
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    /// Optional per-USB-flash audit (per §4.5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flash_history: Vec<FlashHistoryEntry>,
}

fn default_unknown() -> String { "unknown".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub ts: String,
    pub event: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashHistoryEntry {
    pub flashed_at: String,
    pub port: String,
    #[serde(default)]
    pub mac_at_flash: String,
    pub artefact_path: String,
    pub artefact_sha256: String,
    #[serde(default)]
    pub esptool_version: String,
    pub duration_ms: u64,
}

/// On-disk root: `[[devices]] ...`. Wrapped so we can grow the file
/// with non-device top-level tables (e.g. `unaccounted` later).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Roster {
    #[serde(default)]
    pub devices: Vec<DeviceRow>,
}

// ── Public API ─────────────────────────────────────────────────────────

/// Load + parse `apiaries/<name>/devices/roster.toml`. An absent or
/// unreadable file produces an empty roster (the new-apiary case).
pub fn load(apiary_dir: &Path) -> Roster {
    let path = roster_path(apiary_dir);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Roster::default();
    };
    toml::from_str(&raw).unwrap_or_default()
}

/// Atomically persist the roster per SPEC-APIARY-FLASH §2.3:
/// write-temp + fsync(temp) + rename + fsync(dir). Never mutate
/// roster.toml in place.
pub fn save(apiary_dir: &Path, roster: &Roster) -> Result<(), String> {
    let path = roster_path(apiary_dir);
    let parent = path.parent().ok_or("roster path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("mkdir devices/: {e}"))?;

    let raw = toml_serialize(roster)?;
    let tmp = parent.join(".roster.toml.tmp");

    // Write temp.
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp).map_err(|e| format!("create tmp: {e}"))?;
        f.write_all(raw.as_bytes()).map_err(|e| format!("write tmp: {e}"))?;
        f.sync_all().map_err(|e| format!("fsync tmp: {e}"))?;
    }

    // Atomic rename — replaces any existing roster.toml.
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))?;

    // fsync the directory so the rename is durable.
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Find a row by slot_id.
pub fn find_mut<'a>(roster: &'a mut Roster, slot_id: &str) -> Option<&'a mut DeviceRow> {
    roster.devices.iter_mut().find(|d| d.slot_id == slot_id)
}

/// Apply a state transition + append a history entry. Validates the
/// transition against §2.2's table; returns Err if refused.
pub fn apply_transition(
    row: &mut DeviceRow,
    to: &str,
    event: &str,
    detail: &str,
    ts: &str,
) -> Result<(), String> {
    if !is_valid_transition(&row.state, to) {
        return Err(format!(
            "E_ROSTER_TRANSITION_REFUSED: {} → {} is not in the §2.2 table",
            row.state, to
        ));
    }
    let entry = HistoryEntry {
        ts: ts.to_string(),
        event: event.to_string(),
        from: row.state.clone(),
        to: to.to_string(),
        detail: detail.to_string(),
    };
    row.state = to.to_string();
    row.history.push(entry);
    Ok(())
}

/// SPEC-APIARY-FLASH §2.2 — the normative transition table.
/// Any pair not present here is REFUSED.
pub fn is_valid_transition(from: &str, to: &str) -> bool {
    // F1 scope: cover the placeholder lifecycle that matters today.
    // Later chunks extend with `built`, `flashed_pending_pk`, etc.
    const VALID: &[(&str, &[&str])] = &[
        ("",                   &["placeholder"]),
        ("placeholder",        &["built", "retired", "PURGED"]),
        ("built",              &["flashed_pending_pk", "retired", "PURGED"]),
        ("flashed_pending_pk", &["enrolled", "retired", "PURGED"]),
        ("enrolled",           &["reachable", "unreachable", "revoked", "retired"]),
        ("reachable",          &["unreachable", "revoked", "retired"]),
        ("unreachable",        &["reachable", "revoked", "retired"]),
        ("revoked",            &["retired", "PURGED"]),
        ("retired",            &["PURGED"]),
    ];
    VALID
        .iter()
        .find(|(f, _)| *f == from)
        .map(|(_, allowed)| allowed.contains(&to))
        .unwrap_or(false)
}

/// Create a new slot in `state: "placeholder"`. The orchestrator does
/// this on `r2.composer.device.slot.create` from the AI (or any
/// caller). Returns the new row by value; the caller appends to
/// `roster.devices`.
pub fn new_placeholder(role: &str, ensemble: &str, host: &str, name_alias: &str, now: &str) -> DeviceRow {
    let mut row = DeviceRow {
        slot_id: uuid::Uuid::new_v4().to_string(),
        role: role.to_string(),
        ensemble: ensemble.to_string(),
        host: host.to_string(),
        name_alias: name_alias.to_string(),
        state: "".into(),
        provision_state: "unknown".into(),
        cert_status: "unknown".into(),
        device_pk: None,
        rbid: None,
        mac_at_flash: None,
        firmware_sha: None,
        firmware_ver: None,
        enrolled: now.to_string(),
        last_seen: String::new(),
        history: Vec::new(),
        flash_history: Vec::new(),
    };
    let _ = apply_transition(&mut row, "placeholder", "slot.created", "operator declared", now);
    row
}

// ── Internals ──────────────────────────────────────────────────────────

fn roster_path(apiary_dir: &Path) -> PathBuf {
    apiary_dir.join("devices").join("roster.toml")
}

/// TOML serialise — uses a top-level pretty-printer so the file stays
/// human-editable. We never expect to commit dynamically-typed
/// payloads here; the `Roster` shape is closed.
fn toml_serialize(roster: &Roster) -> Result<String, String> {
    // toml v0.8 doesn't preserve insertion order across complex
    // structures, but the round-trip is field-stable enough.
    toml::to_string_pretty(roster).map_err(|e| format!("toml serialise: {e}"))
}

/// Minimal ISO 8601 UTC formatter, second-precision. We avoid pulling
/// in chrono just for this — the format only needs to be sortable and
/// human-readable. Same algorithm as apiary.rs.
pub fn now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let sec_of_day = (secs % 86_400) as u32;
    let (h, m, s) = (sec_of_day / 3600, (sec_of_day / 60) % 60, sec_of_day % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// Suppress unused-warning for the BTreeMap import — kept reserved
// for slot-detail tables (e.g. plugin_overrides per-slot) added in
// F2+ chunks without re-touching this module's imports.
#[allow(dead_code)]
fn _reserve_btreemap() -> BTreeMap<String, String> { BTreeMap::new() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_placeholder_starts_in_placeholder_state() {
        let row = new_placeholder("sensor", "rocker-sensor", "esp32-s3-xiao", "kitchen", "2026-06-01T00:00:00Z");
        assert_eq!(row.state, "placeholder");
        assert_eq!(row.role, "sensor");
        assert_eq!(row.host, "esp32-s3-xiao");
        assert_eq!(row.name_alias, "kitchen");
        assert!(!row.slot_id.is_empty());
        assert_eq!(row.history.len(), 1);
        assert_eq!(row.history[0].from, "");
        assert_eq!(row.history[0].to, "placeholder");
        assert_eq!(row.provision_state, "unknown");
        assert_eq!(row.cert_status, "unknown");
        assert!(row.device_pk.is_none());
    }

    #[test]
    fn transitions_validated_against_table() {
        assert!(is_valid_transition("placeholder", "built"));
        assert!(is_valid_transition("built", "flashed_pending_pk"));
        assert!(is_valid_transition("flashed_pending_pk", "enrolled"));
        assert!(is_valid_transition("reachable", "unreachable"));
        assert!(is_valid_transition("unreachable", "reachable"));

        // Forbidden — flashed_pending_pk cannot regress to built without
        // a fresh USB flash (§2.2 note).
        assert!(!is_valid_transition("flashed_pending_pk", "built"));
        // Forbidden — no skipping states.
        assert!(!is_valid_transition("placeholder", "enrolled"));
        // Forbidden — PURGED is a sink; cannot re-emerge.
        assert!(!is_valid_transition("PURGED", "retired"));
    }

    #[test]
    fn apply_transition_appends_history_and_updates_state() {
        let mut row = new_placeholder("sensor", "rocker-sensor", "esp32-s3-xiao", "", "2026-06-01T00:00:00Z");
        apply_transition(&mut row, "built", "built", "artefact x", "2026-06-01T01:00:00Z").unwrap();
        assert_eq!(row.state, "built");
        assert_eq!(row.history.len(), 2);
        assert_eq!(row.history[1].from, "placeholder");
        assert_eq!(row.history[1].to, "built");
    }

    #[test]
    fn apply_transition_refuses_invalid() {
        let mut row = new_placeholder("sensor", "rocker-sensor", "esp32-s3-xiao", "", "2026-06-01T00:00:00Z");
        let err = apply_transition(&mut row, "enrolled", "wrong", "", "2026-06-01T01:00:00Z").unwrap_err();
        assert!(err.contains("E_ROSTER_TRANSITION_REFUSED"));
        // Row state unchanged.
        assert_eq!(row.state, "placeholder");
        assert_eq!(row.history.len(), 1);
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let mut roster = Roster::default();
        roster.devices.push(new_placeholder("sensor", "rocker-sensor", "esp32-s3-xiao", "kitchen", "2026-06-01T00:00:00Z"));
        save(dir.path(), &roster).expect("atomic save");

        let loaded = load(dir.path());
        assert_eq!(loaded.devices.len(), 1);
        assert_eq!(loaded.devices[0].name_alias, "kitchen");
        assert_eq!(loaded.devices[0].state, "placeholder");
    }

    #[test]
    fn load_returns_empty_when_no_roster_yet() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load(dir.path());
        assert_eq!(loaded.devices.len(), 0);
    }
}
