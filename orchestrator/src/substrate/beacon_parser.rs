//! R2-BEACON Legacy 28-byte AD parser — observer side.
//!
//! Vendored from `r2-core::beacon` (private repo) per the
//! [[project-repo-visibility-state]] memo: r2-composer is public,
//! r2-core is private, so we can't path-dep. The parsing logic is
//! straightforward byte-slicing (no crypto) and tightly bounded by the
//! R2-BEACON §7.3 wire format.
//!
//! ## What's here
//!
//! - `R2_BEACON_MAGIC` / `BEACON_VERSION` / `COMPANY_ID` — constants
//! - `BeaconFlags` — the §7.2 flags byte broken out
//! - `LegacyBeacon` — the §7.3 parsed AD structure
//! - `parse_legacy_beacon` — decodes the canonical 28-byte AD
//! - `BeaconError` — parse failure modes
//!
//! ## What's NOT here
//!
//! - `compute_rbid` (HMAC-SHA256) — emit-side only; the observer just
//!   stores the rotating RBID as opaque bytes (R2-BEACON §6.1)
//! - Extended beacon parsing — Phase 2; out of F4 scope
//! - Bloom filter — Extended-only; out of scope
//!
//! ## Canonical wire format (R2-BEACON §7.3)
//!
//! ```text
//!   offset  size  field
//!     0      1    AD Length (= 0x1B = 27 bytes follow)
//!     1      1    AD Type (= 0xFF Manufacturer Specific)
//!     2-3    2    Company ID (= 0xFFFF LE)
//!     4      1    R2 magic (= 0xB2)
//!     5      1    Beacon version (= 0x01)
//!     6      1    Flags byte (§7.2)
//!     7-14   8    RBID (rotating per §6.1)
//!     15-18  4    class_hash (FNV-1a-32, big-endian)
//!     19     1    TX power int8 dBm
//!     20-21  2    anti_collision u16 LE
//!     22-27  6    reserved (zero)
//! ```
//!
//! ## Platform API gotcha (§7.7.3)
//!
//! BLE platform APIs (btleplug, bluer, CoreBluetooth, WinRT) strip
//! bytes 0-3 (AD-Length + AD-Type + Company ID) and key the
//! manufacturer-data dict by Company ID. The payload you receive
//! starts at offset 4 (the `0xB2` magic byte). Callers MUST
//! reconstruct the canonical 28-byte AD before calling
//! `parse_legacy_beacon` — see [`reconstruct_ad`].

/// R2 Beacon magic byte (R2-BEACON §7.3 offset 4).
pub const R2_BEACON_MAGIC: u8 = 0xB2;

/// Current beacon protocol version.
pub const BEACON_VERSION: u8 = 0x01;

/// Company ID for development (R2-BEACON §7.3). 0xFFFF is the
/// reserved-for-internal-use ID; production will register a real one.
pub const COMPANY_ID: u16 = 0xFFFF;

/// Beacon flags byte broken out per R2-BEACON §7.2.
///
/// Bit layout:
/// ```text
///   7: profile      (0 = Legacy, 1 = Extended)
///   6: has_bloom    (Extended only)
///   5: provisioning (device in provisioning mode — fresh / un-enrolled)
///   4: mcu_mode     (MCU-only beacon, SBC sleeping)
///   3: mobile       (device in motion)
///   2-0: reserved
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeaconFlags {
    pub profile: u8,
    pub has_bloom: bool,
    pub provisioning: bool,
    pub mcu_mode: bool,
    pub mobile: bool,
}

impl BeaconFlags {
    pub fn decode(byte: u8) -> Self {
        Self {
            profile: (byte >> 7) & 1,
            has_bloom: (byte >> 6) & 1 != 0,
            provisioning: (byte >> 5) & 1 != 0,
            mcu_mode: (byte >> 4) & 1 != 0,
            mobile: (byte >> 3) & 1 != 0,
        }
    }

    pub fn encode(&self) -> u8 {
        (self.profile << 7)
            | ((self.has_bloom as u8) << 6)
            | ((self.provisioning as u8) << 5)
            | ((self.mcu_mode as u8) << 4)
            | ((self.mobile as u8) << 3)
    }
}

/// Parsed Legacy beacon (R2-BEACON §7.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyBeacon {
    pub version: u8,
    pub flags: BeaconFlags,
    pub rbid: [u8; 8],
    pub class_hash: [u8; 4],
    pub tx_power: i8,
    pub anti_collision: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeaconError {
    TooShort,
    NotR2Beacon,
    InvalidVersion(u8),
}

/// Reconstruct the canonical 28-byte AD from a BLE platform API's
/// stripped manufacturer-data payload (R2-BEACON §7.7.3). The payload
/// starts at offset 4 (the magic byte); this prepends the AD-Length,
/// AD-Type, and Company ID bytes so [`parse_legacy_beacon`] can run.
///
/// Caller MUST have already verified the payload starts with
/// `R2_BEACON_MAGIC` and the manufacturer-data key was `COMPANY_ID`.
pub fn reconstruct_ad(payload: &[u8]) -> [u8; 28] {
    let mut full_ad = [0u8; 28];
    full_ad[0] = 0x1B;
    full_ad[1] = 0xFF;
    full_ad[2] = (COMPANY_ID & 0xFF) as u8;
    full_ad[3] = (COMPANY_ID >> 8) as u8;
    let copy_len = payload.len().min(24);
    full_ad[4..4 + copy_len].copy_from_slice(&payload[..copy_len]);
    full_ad
}

/// Parse a Legacy beacon from the canonical 28-byte AD. Caller passes
/// the FULL reconstructed AD (offset 0 = AD-Length, offset 4 = magic).
pub fn parse_legacy_beacon(data: &[u8]) -> Result<LegacyBeacon, BeaconError> {
    if data.len() < 28 {
        return Err(BeaconError::TooShort);
    }
    if data[4] != R2_BEACON_MAGIC {
        return Err(BeaconError::NotR2Beacon);
    }
    let version = data[5];
    if version != BEACON_VERSION {
        return Err(BeaconError::InvalidVersion(version));
    }
    let flags = BeaconFlags::decode(data[6]);
    let mut rbid = [0u8; 8];
    rbid.copy_from_slice(&data[7..15]);
    let mut class_hash = [0u8; 4];
    class_hash.copy_from_slice(&data[15..19]);
    let tx_power = data[19] as i8;
    let anti_collision = u16::from_le_bytes([data[20], data[21]]);

    Ok(LegacyBeacon {
        version,
        flags,
        rbid,
        class_hash,
        tx_power,
        anti_collision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a canonical 28-byte AD for testing.
    fn build_test_ad(flags: u8, class_hash: [u8; 4], rbid: [u8; 8]) -> [u8; 28] {
        let mut ad = [0u8; 28];
        ad[0] = 0x1B;
        ad[1] = 0xFF;
        ad[2] = 0xFF;
        ad[3] = 0xFF;
        ad[4] = R2_BEACON_MAGIC;
        ad[5] = BEACON_VERSION;
        ad[6] = flags;
        ad[7..15].copy_from_slice(&rbid);
        ad[15..19].copy_from_slice(&class_hash);
        ad[19] = (-12i8) as u8;
        ad[20] = 0x39;
        ad[21] = 0x30; // 0x3039 = 12345 LE
        ad
    }

    #[test]
    fn parses_canonical_provisioning_beacon() {
        let ad = build_test_ad(
            0b0010_0000, // provisioning bit set
            [0x43, 0x89, 0x5E, 0x89],
            [1, 2, 3, 4, 5, 6, 7, 8],
        );
        let b = parse_legacy_beacon(&ad).expect("parse");
        assert!(b.flags.provisioning, "provisioning flag set");
        assert!(!b.flags.mcu_mode);
        assert_eq!(b.class_hash, [0x43, 0x89, 0x5E, 0x89]);
        assert_eq!(b.rbid, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(b.tx_power, -12);
        assert_eq!(b.anti_collision, 12345);
        assert_eq!(b.version, BEACON_VERSION);
    }

    #[test]
    fn rejects_short_buffer() {
        let short = [0u8; 27];
        assert_eq!(parse_legacy_beacon(&short), Err(BeaconError::TooShort));
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut ad = build_test_ad(0, [0; 4], [0; 8]);
        ad[4] = 0xC2; // not R2 magic
        assert_eq!(parse_legacy_beacon(&ad), Err(BeaconError::NotR2Beacon));
    }

    #[test]
    fn rejects_unknown_version() {
        let mut ad = build_test_ad(0, [0; 4], [0; 8]);
        ad[5] = 0x02; // future version
        assert_eq!(parse_legacy_beacon(&ad), Err(BeaconError::InvalidVersion(0x02)));
    }

    #[test]
    fn reconstruct_ad_handles_btleplug_strip() {
        // Simulate what btleplug gives us — the manufacturer-data payload
        // starts at the magic byte (offset 4 of the canonical AD).
        let payload: Vec<u8> = vec![
            R2_BEACON_MAGIC, BEACON_VERSION, 0b0010_0000,
            1, 2, 3, 4, 5, 6, 7, 8,            // rbid
            0x43, 0x89, 0x5E, 0x89,            // class_hash
            ((-12i8) as u8),                    // tx_power
            0x39, 0x30,                         // anti_collision LE
            0, 0, 0, 0, 0, 0,                   // reserved
        ];
        let ad = reconstruct_ad(&payload);
        let b = parse_legacy_beacon(&ad).expect("parse");
        assert!(b.flags.provisioning);
        assert_eq!(b.anti_collision, 12345);
    }

    #[test]
    fn flags_round_trip() {
        let raw = 0b0010_1000; // provisioning + mobile
        let f = BeaconFlags::decode(raw);
        assert!(f.provisioning);
        assert!(f.mobile);
        assert!(!f.mcu_mode);
        assert_eq!(f.encode(), raw);
    }
}
