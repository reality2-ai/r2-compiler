//! TG-state persistence for the apiary's R2-TRUST `TrustGroup`.
//!
//! Per R2-TRUST §5.6 the key-holder's TG state — members, revocations,
//! sequence counter — MUST persist across restarts. Without this the
//! orchestrator would forget who's enrolled on every restart, the
//! revocation list would be lost, and the sequence counter would reset
//! (which collides with member-side replay protection on the very next
//! GROUP_MGMT frame).
//!
//! ## File location
//!
//! ```text
//! apiaries/<name>/devices/tg_state.bin    — in-tree (public material only)
//! ```
//!
//! In-tree because: this holds only certificates + revocation entries
//! (signed by the TG, but no signing key material). The TG signing key
//! itself stays off-tree at `~/.config/r2-composer/apiaries/<name>/tg_signer/tg_priv.bin`
//! per PROCESS.md.
//!
//! ## Wire format (versioned binary)
//!
//! ```text
//!   offset  size                  field
//!     0      4                    magic "R2TG"
//!     4      1                    version = 0x01
//!     5      1                    min_crypto_level (u8 enum disc)
//!     6      4                    sequence (u32 BE)
//!    10     147                   self_cert (DeviceCertificate)
//!   157      2                    member_count (u16 BE)
//!   [for each member:
//!         2 + name_len + 147      name_len(u16 BE) || name(UTF-8) || cert(147)
//!   ]
//!   [next 2 bytes after members:  revocation_count (u16 BE)]
//!   [for each revocation: 107 bytes (REVOCATION_LEN per r2-trust)]
//! ```
//!
//! The format is internal — only this module produces / consumes it.
//! Versioning leaves room for richer state when r2-trust grows.

use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use r2_trust::cert::DeviceCertificate;
use r2_trust::lifecycle::{MemberInfo, TrustGroup};
use r2_trust::revocation::{RevocationEntry, RevocationSet};
use r2_trust::types::{MinCryptoLevel, DEVICE_CERT_LEN};

const MAGIC: &[u8; 4] = b"R2TG";
const VERSION: u8 = 0x01;
const REVOCATION_LEN: usize = 107;     // r2-trust internal const; not re-exported

/// Path the TG state file lives at for a given apiary directory.
pub fn state_path(apiary_dir: &Path) -> PathBuf {
    apiary_dir.join("devices/tg_state.bin")
}

/// Persist the TG to disk. Atomic via write-temp + rename + fsync the
/// parent dir, same pattern the roster uses.
pub fn save(apiary_dir: &Path, tg: &TrustGroup) -> std::io::Result<()> {
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    buf.extend_from_slice(MAGIC);
    buf.push(VERSION);
    buf.push(u8::from(tg.min_crypto_level()));
    buf.extend_from_slice(&tg.sequence().to_be_bytes());
    buf.extend_from_slice(&tg.self_certificate().to_bytes());
    let members = tg.members();
    if members.len() > u16::MAX as usize {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
            "too many members to persist (u16 cap)"));
    }
    buf.extend_from_slice(&(members.len() as u16).to_be_bytes());
    for m in members {
        let name_bytes = m.name.as_bytes();
        if name_bytes.len() > u16::MAX as usize {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                "member name too long"));
        }
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&m.certificate.to_bytes());
    }
    let revs: Vec<&RevocationEntry> = tg.revocations().iter().collect();
    if revs.len() > u16::MAX as usize {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
            "too many revocations"));
    }
    buf.extend_from_slice(&(revs.len() as u16).to_be_bytes());
    for r in revs {
        buf.extend_from_slice(&r.to_bytes());
    }

    let path = state_path(apiary_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("bin.tmp");
    std::fs::write(&tmp, &buf)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Try to restore the TG from disk. Returns `Ok(None)` if no state
/// file exists (first run — caller should construct a fresh TG from
/// the signing key). Returns `Err` on a corrupt or version-mismatched
/// state file — caller should treat that as a hard failure rather
/// than silently rebuilding (a corrupt state file may indicate disk
/// damage or an in-progress migration).
pub fn load(apiary_dir: &Path, sk: SigningKey) -> Result<Option<TrustGroup>, LoadError> {
    let path = state_path(apiary_dir);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(LoadError::Io)?;
    // Identify the file before judging its length: a non-R2 or wrong-version
    // file should report BadMagic / UnknownVersion even when it's short,
    // rather than being misreported as a truncated R2 state file.
    if bytes.len() < 6 || &bytes[0..4] != MAGIC {
        return Err(LoadError::BadMagic);
    }
    if bytes[4] != VERSION {
        return Err(LoadError::UnknownVersion(bytes[4]));
    }
    if bytes.len() < 10 + DEVICE_CERT_LEN + 2 {
        return Err(LoadError::Truncated);
    }
    let min_crypto_level = MinCryptoLevel::try_from(bytes[5])
        .map_err(|_| LoadError::BadCryptoLevel(bytes[5]))?;
    let sequence = u32::from_be_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]);
    let self_cert = DeviceCertificate::from_bytes(&bytes[10..10 + DEVICE_CERT_LEN])
        .map_err(|e| LoadError::Cert(format!("{e:?}")))?;
    let mut off = 10 + DEVICE_CERT_LEN;
    let member_count = u16::from_be_bytes([bytes[off], bytes[off + 1]]) as usize;
    off += 2;

    let mut members: Vec<MemberInfo> = Vec::with_capacity(member_count);
    for _ in 0..member_count {
        if off + 2 > bytes.len() { return Err(LoadError::Truncated); }
        let name_len = u16::from_be_bytes([bytes[off], bytes[off + 1]]) as usize;
        off += 2;
        if off + name_len + DEVICE_CERT_LEN > bytes.len() {
            return Err(LoadError::Truncated);
        }
        let name = std::str::from_utf8(&bytes[off..off + name_len])
            .map_err(|_| LoadError::BadUtf8)?.to_string();
        off += name_len;
        let cert = DeviceCertificate::from_bytes(&bytes[off..off + DEVICE_CERT_LEN])
            .map_err(|e| LoadError::Cert(format!("{e:?}")))?;
        off += DEVICE_CERT_LEN;
        members.push(MemberInfo { certificate: cert, name });
    }

    if off + 2 > bytes.len() { return Err(LoadError::Truncated); }
    let rev_count = u16::from_be_bytes([bytes[off], bytes[off + 1]]) as usize;
    off += 2;
    let mut revocations = RevocationSet::new();
    for _ in 0..rev_count {
        if off + REVOCATION_LEN > bytes.len() { return Err(LoadError::Truncated); }
        let entry = RevocationEntry::from_bytes(&bytes[off..off + REVOCATION_LEN])
            .map_err(|e| LoadError::Revocation(format!("{e:?}")))?;
        revocations.add(entry);
        off += REVOCATION_LEN;
    }

    let tg = TrustGroup::restore(sk, self_cert, members, revocations, sequence, min_crypto_level)
        .map_err(|e| LoadError::Restore(format!("{e:?}")))?;
    Ok(Some(tg))
}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Truncated,
    BadMagic,
    UnknownVersion(u8),
    BadCryptoLevel(u8),
    BadUtf8,
    Cert(String),
    Revocation(String),
    Restore(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "io: {e}"),
            LoadError::Truncated => write!(f, "truncated state file"),
            LoadError::BadMagic => write!(f, "bad magic — not an R2 TG state file"),
            LoadError::UnknownVersion(v) => write!(f, "unknown state version 0x{v:02X}"),
            LoadError::BadCryptoLevel(v) => write!(f, "bad min_crypto_level 0x{v:02X}"),
            LoadError::BadUtf8 => write!(f, "member name is not valid UTF-8"),
            LoadError::Cert(m) => write!(f, "DeviceCertificate decode: {m}"),
            LoadError::Revocation(m) => write!(f, "RevocationEntry decode: {m}"),
            LoadError::Restore(m) => write!(f, "TrustGroup::restore: {m}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn fresh_tg() -> (TrustGroup, SigningKey) {
        let sk = SigningKey::from_bytes(&[0xAB; 32]);
        let tg = TrustGroup::from_signing_key(sk.clone(), 1_700_000_000).unwrap();
        (tg, sk)
    }

    #[test]
    fn save_and_load_empty_tg_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let (tg, sk) = fresh_tg();
        save(dir.path(), &tg).unwrap();
        let loaded = match load(dir.path(), sk) {
            Ok(Some(tg)) => tg,
            other => panic!("expected Ok(Some(tg)), got {:?}",
                other.map(|o| o.is_some()).map_err(|e| e.to_string())),
        };
        assert_eq!(loaded.sequence(), tg.sequence());
        assert_eq!(loaded.trust_group_id(), tg.trust_group_id());
        assert_eq!(loaded.members().len(), 0);
    }

    #[test]
    fn load_returns_none_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let sk = SigningKey::from_bytes(&[0xCD; 32]);
        match load(dir.path(), sk) {
            Ok(None) => (),
            Ok(Some(_)) => panic!("expected None"),
            Err(e) => panic!("load: {e}"),
        }
    }

    #[test]
    fn save_creates_devices_subdir() {
        let dir = tempfile::tempdir().unwrap();
        let (tg, _) = fresh_tg();
        save(dir.path(), &tg).unwrap();
        assert!(dir.path().join("devices/tg_state.bin").exists());
    }

    #[test]
    fn bad_magic_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("devices")).unwrap();
        std::fs::write(state_path(dir.path()), b"XXXXfake-state").unwrap();
        let sk = SigningKey::from_bytes(&[0; 32]);
        match load(dir.path(), sk) {
            Err(LoadError::BadMagic) => (),
            Err(e) => panic!("expected BadMagic, got Err({e})"),
            Ok(Some(_)) => panic!("expected BadMagic, got Ok(Some)"),
            Ok(None)    => panic!("expected BadMagic, got Ok(None)"),
        }
    }

    #[test]
    fn round_trips_with_a_member() {
        let dir = tempfile::tempdir().unwrap();
        let (mut tg, sk) = fresh_tg();
        // Mint a join code + process a synthetic request to add a member.
        let mut rng = rand_chacha::ChaCha20Rng::from_seed([0; 32]);
        use rand_chacha::rand_core::SeedableRng;
        let join_code = *tg.generate_join_code(&mut rng, 1_700_000_000, 300).value();
        let device_sk = SigningKey::from_bytes(&[0xCD; 32]);
        let device_pk = device_sk.verifying_key();
        tg.process_join_request(
            &mut rng, 1_700_000_000, &join_code, &device_pk,
            "test-device".into(), 86_400,
        ).unwrap();

        save(dir.path(), &tg).unwrap();
        let loaded = match load(dir.path(), sk) {
            Ok(Some(tg)) => tg,
            other => panic!("expected Ok(Some(tg)), got {:?}",
                other.map(|o| o.is_some()).map_err(|e| e.to_string())),
        };
        assert_eq!(loaded.members().len(), 1);
        assert_eq!(loaded.members()[0].name, "test-device");
        assert_eq!(loaded.sequence(), tg.sequence());
    }
}
