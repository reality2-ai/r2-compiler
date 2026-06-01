//! `keyholder` plugin — owns the active apiary's TG Ed25519 keypair.
//!
//! Per SPEC-APIARY-CREATE §2.2 + SPEC-APIARY-FLASH §3.3. v0.1 (F3 scope):
//! lazy-loads the private key from the off-tree path on first use; signs
//! DeviceCertificate blobs when the Roster asks; reports the public
//! fingerprint. Key GENERATION + IMPORT are deferred — today the AI
//! writes them directly during apiary.create (the chat-author flow);
//! formalising that into a CMD on this plugin is an F3b task per
//! SPEC-APIARY-CREATE §10.
//!
//! ## DeviceCertificate format (v0.1)
//!
//! ```
//!   device_pk    : 32 bytes (Ed25519 pubkey)
//!   tg_pub       : 32 bytes (apiary's TG Ed25519 pubkey)
//!   valid_from   :  8 bytes (u64 BE — unix seconds)
//!   valid_until  :  8 bytes (u64 BE — unix seconds)
//!   signature    : 64 bytes (Ed25519 over the previous 80 bytes,
//!                            signed by the apiary's TG private key)
//!   ────────
//!   total        : 144 bytes
//! ```
//!
//! Written to `apiaries/<name>/devices/certs/<device_pk_hex>.bin`. This
//! is r2-composer's *operational* cert format for v0.1; the upstream
//! R2-TRUST spec MAY define a richer one — when that lands, this
//! module gets updated in lockstep.
//!
//! Lifecycle:
//! - `Plugin::execute(CMD_SIGN_CERT, payload)` — sign a cert for
//!   `device_pk`. Payload `{device_pk, valid_secs}`. Returns the 144-byte
//!   cert. Emits `r2.composer.provision.cert_issued{device_pk, cert_hex,
//!   tg_fp}` so the Roster can write it + transition the slot.
//! - `Plugin::execute(CMD_FINGERPRINT, _)` — return the SHA-256
//!   fingerprint of the current TG pub key. Lazy-loads if needed.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::sentants::RosterCtx;

/// Side-channel payload for CMD_SIGN_CERT — bus payload is small enough
/// that we could pass these via `Action::PluginCall.data`, but using
/// the shared-slot pattern matches the other plugins (claude-code,
/// flasher) and gives room for future fields without bus-payload
/// pressure.
pub type KeyholderSlot = Arc<Mutex<Option<SignCertRequest>>>;

#[derive(Debug, Clone)]
pub struct SignCertRequest {
    /// Ed25519 pubkey of the device — 32 raw bytes.
    pub device_pk: [u8; 32],
    /// Cert validity window in seconds from now.
    pub valid_secs: u64,
    /// Slot id this cert is for — passed through to the emitted event
    /// so the Roster knows which row to mutate.
    pub slot_id: String,
}

pub const CMD_SIGN_CERT:   PluginCommand = 0x01;
pub const CMD_FINGERPRINT: PluginCommand = 0x02;

pub const ERR_NO_APIARY:        u8 = 0x01;
pub const ERR_KEY_LOAD_FAILED:  u8 = 0x02;
pub const ERR_NO_REQUEST:       u8 = 0x03;
pub const ERR_SIGN_FAILED:      u8 = 0x04;
pub const ERR_UNKNOWN_COMMAND:  u8 = 0xFE;

pub struct KeyholderPlugin {
    id: PluginId,
    apiary_ctx: RosterCtx,
    sign_slot: KeyholderSlot,
    /// Root for off-tree key storage. Production:
    /// `$HOME/.config/r2-composer/`. Tests override.
    config_root: PathBuf,
    /// Lazy-loaded keypair. None until first SIGN/FINGERPRINT call,
    /// then cached for the lifetime of the active apiary. Cleared if
    /// the apiary changes (future runtime apiary-open path).
    cached_key: Option<LoadedKey>,
    /// Pre-hashed event names.
    hash_cert_issued: u32,
    hash_error: u32,
    out_buf: Vec<u8>,
    /// Pending outputs to surface via poll(). CMD_SIGN_CERT writes here
    /// after successfully minting a cert.
    pending: Vec<(u32, Vec<u8>)>,
}

struct LoadedKey {
    /// The 32-byte public key (cached for cert composition).
    public: [u8; 32],
    /// SHA-256 of the public key bytes (lowercase hex, 64 chars).
    fingerprint: String,
    /// The signing key (kept private inside this struct).
    signing: ed25519_dalek::SigningKey,
    /// The off-tree path the priv key was loaded from (for diagnostics).
    priv_path: PathBuf,
}

impl KeyholderPlugin {
    /// `config_root` is the directory holding `apiaries/<name>/tg_signer/`.
    /// In production: `$HOME/.config/r2-composer/`. In tests: a tempdir.
    pub fn new(
        id: PluginId,
        apiary_ctx: RosterCtx,
        sign_slot: KeyholderSlot,
        config_root: PathBuf,
    ) -> Self {
        Self {
            id,
            apiary_ctx,
            sign_slot,
            config_root,
            cached_key: None,
            hash_cert_issued: r2_fnv::fnv1a_32(b"r2.composer.provision.cert_issued"),
            hash_error:       r2_fnv::fnv1a_32(b"r2.composer.provision.cert_error"),
            out_buf: Vec::with_capacity(256),
            pending: Vec::new(),
        }
    }

    /// Default production config root: `$HOME/.config/r2-composer/`.
    pub fn default_config_root() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".config/r2-composer")
    }

    /// Lazy-load + cache the apiary's TG keypair from off-tree.
    /// Returns `()` so callers can drop the mut borrow immediately and
    /// re-borrow `self.cached_key` immutably for read access.
    fn ensure_loaded(&mut self) -> Result<(), (u8, String)> {
        if self.cached_key.is_some() { return Ok(()); }
        let apiary_dir = {
            let ctx = self.apiary_ctx.lock().unwrap();
            match ctx.as_ref() {
                Some(p) => p.clone(),
                None => return Err((ERR_NO_APIARY, "no apiary open".into())),
            }
        };
        let apiary_name = apiary_dir.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let priv_path = self.config_root
            .join("apiaries")
            .join(&apiary_name)
            .join("tg_signer/tg_priv.bin");
        let priv_bytes = std::fs::read(&priv_path)
            .map_err(|e| (ERR_KEY_LOAD_FAILED,
                format!("read {}: {e}", priv_path.display())))?;
        if priv_bytes.len() != 32 {
            return Err((ERR_KEY_LOAD_FAILED,
                format!("expected 32-byte Ed25519 seed, got {}", priv_bytes.len())));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&priv_bytes);
        let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
        let verifying = signing.verifying_key();
        let public = *verifying.as_bytes();

        // Also read tg_pub.bin from in-tree and cross-check it matches —
        // if they don't, the apiary's on-disk state is inconsistent and
        // we refuse to operate.
        let pub_path = apiary_dir.join("trust_keys/tg_pub.bin");
        if let Ok(in_tree_pub) = std::fs::read(&pub_path) {
            if in_tree_pub != public.to_vec() {
                return Err((ERR_KEY_LOAD_FAILED,
                    format!("tg_pub.bin in-tree disagrees with derived pub key — \
                        priv_path={} pub_path={}",
                        priv_path.display(), pub_path.display())));
            }
        }

        let mut h = Sha256::new();
        h.update(public);
        let fingerprint = hex::encode(h.finalize());

        self.cached_key = Some(LoadedKey {
            public,
            fingerprint,
            signing,
            priv_path,
        });
        Ok(())
    }

    fn handle_sign_cert(&mut self) {
        let taken = { self.sign_slot.lock().unwrap().take() };
        let req = match taken {
            Some(r) => r,
            None => {
                self.queue_error(ERR_NO_REQUEST, "CMD_SIGN_CERT with no request in slot".into(), "");
                return;
            }
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let valid_from = now;
        let valid_until = now.saturating_add(req.valid_secs);

        if let Err((code, msg)) = self.ensure_loaded() {
            self.queue_error(code, msg, &req.slot_id);
            return;
        }

        // Borrow cached_key only long enough to sign + copy out owned
        // values; drop before re-borrowing self for pack_with.
        let (cert, tg_pub_hex, tg_fp) = {
            let key = self.cached_key.as_ref().unwrap();
            let mut tbs = Vec::with_capacity(80);
            tbs.extend_from_slice(&req.device_pk);
            tbs.extend_from_slice(&key.public);
            tbs.extend_from_slice(&valid_from.to_be_bytes());
            tbs.extend_from_slice(&valid_until.to_be_bytes());
            use ed25519_dalek::Signer;
            let sig = key.signing.sign(&tbs);
            let mut cert = Vec::with_capacity(144);
            cert.extend_from_slice(&tbs);
            cert.extend_from_slice(&sig.to_bytes());
            (cert, hex::encode(key.public), key.fingerprint.clone())
        };

        let payload = self.pack_with(&serde_json::json!({
            "slot_id":     req.slot_id,
            "device_pk":   hex::encode(req.device_pk),
            "tg_pub":      tg_pub_hex,
            "tg_fp":       tg_fp,
            "valid_from":  valid_from,
            "valid_until": valid_until,
            "cert_hex":    hex::encode(&cert),
        }));
        self.pending.push((self.hash_cert_issued, payload));
    }

    fn handle_fingerprint(&mut self) -> PluginResult {
        if let Err((code, msg)) = self.ensure_loaded() {
            return PluginResult::Error(PluginError::new(code, &msg));
        }
        let fp = self.cached_key.as_ref().unwrap().fingerprint.clone();
        let bytes = self.pack_with(&serde_json::json!({ "fp": fp }));
        PluginResult::Ok(PluginResponse::with_data(&bytes))
    }

    fn queue_error(&mut self, code: u8, message: String, slot_id: &str) {
        let payload = self.pack_with(&serde_json::json!({
            "slot_id": slot_id,
            "code": code,
            "message": message,
        }));
        self.pending.push((self.hash_error, payload));
    }

    /// Pack to an owned Vec so we can push to `pending` without
    /// borrow-checker issues with `out_buf`.
    fn pack_with<T: Serialize>(&mut self, v: &T) -> Vec<u8> {
        self.out_buf.clear();
        let _ = serde_json::to_writer(&mut self.out_buf, v);
        self.out_buf.clone()
    }
}

impl Plugin for KeyholderPlugin {
    fn execute(&mut self, command: PluginCommand, _data: &[u8]) -> PluginResult {
        match command {
            CMD_SIGN_CERT => {
                self.handle_sign_cert();
                PluginResult::Ok(PluginResponse::empty())
            }
            CMD_FINGERPRINT => self.handle_fingerprint(),
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "unknown command byte")),
        }
    }
    fn name(&self) -> &str { "keyholder" }
    fn id(&self) -> PluginId { self.id }

    fn poll(&mut self) -> Option<(u32, &[u8])> {
        if self.pending.is_empty() { return None }
        // Move the next pending event into out_buf so we can return a
        // &[u8] borrow that lives until the next poll().
        let (hash, payload) = self.pending.remove(0);
        self.out_buf = payload;
        Some((hash, &self.out_buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Verifier;

    /// Returns (apiary tempdir, config-root tempdir, ctx, sign-slot).
    /// Both tempdirs are kept alive by the caller; passing config_root
    /// explicitly removes the $HOME race that plagued earlier tests.
    fn setup() -> (tempfile::TempDir, tempfile::TempDir, RosterCtx, KeyholderSlot, PathBuf) {
        let apiary = tempfile::tempdir().unwrap();
        let apiary_name = apiary.path().file_name().unwrap().to_string_lossy().to_string();

        let signing = ed25519_dalek::SigningKey::from_bytes(&[0xAB; 32]);
        let pub_bytes = signing.verifying_key().to_bytes();

        let cfg = tempfile::tempdir().unwrap();
        let priv_dir = cfg.path().join("apiaries").join(&apiary_name).join("tg_signer");
        std::fs::create_dir_all(&priv_dir).unwrap();
        std::fs::write(priv_dir.join("tg_priv.bin"), signing.to_bytes()).unwrap();
        std::fs::create_dir_all(apiary.path().join("trust_keys")).unwrap();
        std::fs::write(apiary.path().join("trust_keys/tg_pub.bin"), pub_bytes).unwrap();

        let ctx: RosterCtx = Arc::new(Mutex::new(Some(apiary.path().to_path_buf())));
        let slot: KeyholderSlot = Arc::new(Mutex::new(None));
        let cfg_root = cfg.path().to_path_buf();
        (apiary, cfg, ctx, slot, cfg_root)
    }

    #[test]
    fn fingerprint_loads_real_key() {
        let (_apiary, _cfg, ctx, slot, cfg_root) = setup();
        let mut k = KeyholderPlugin::new(0, ctx, slot, cfg_root);
        match k.execute(CMD_FINGERPRINT, &[]) {
            PluginResult::Ok(resp) => {
                let v: serde_json::Value = serde_json::from_slice(resp.as_slice()).unwrap();
                let fp = v["fp"].as_str().unwrap();
                assert_eq!(fp.len(), 64, "SHA-256 hex is 64 chars");
                assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
            }
            PluginResult::Error(e) => panic!("fingerprint failed: code={:02X} {}", e.code, e.description()),
        }
    }

    #[test]
    fn sign_cert_produces_valid_signature() {
        let (_apiary, _cfg, ctx, slot, cfg_root) = setup();
        let mut k = KeyholderPlugin::new(0, ctx.clone(), slot.clone(), cfg_root);

        // Fake device pubkey — deterministic.
        let device_signing = ed25519_dalek::SigningKey::from_bytes(&[0xCD; 32]);
        let device_pk: [u8; 32] = device_signing.verifying_key().to_bytes();

        // Fill the slot with the request.
        *slot.lock().unwrap() = Some(SignCertRequest {
            device_pk,
            valid_secs: 86_400,
            slot_id: "sensor:esp32-s3-xiao:1234".into(),
        });

        match k.execute(CMD_SIGN_CERT, &[]) {
            PluginResult::Ok(_) => {}
            other => panic!("sign failed: {other:?}"),
        }

        // poll() should yield one cert_issued event.
        let expected_hash = k.hash_cert_issued;
        let (hash, payload) = k.poll().expect("expected pending event");
        assert_eq!(hash, expected_hash);
        let v: serde_json::Value = serde_json::from_slice(payload).unwrap();
        assert_eq!(v["slot_id"], "sensor:esp32-s3-xiao:1234");
        let cert_hex = v["cert_hex"].as_str().unwrap();
        assert_eq!(cert_hex.len(), 144 * 2, "cert is 144 bytes / 288 hex chars");
        let cert = hex::decode(cert_hex).unwrap();

        // Verify signature over the to-be-signed bytes.
        let tg_pub_hex = v["tg_pub"].as_str().unwrap();
        let tg_pub = hex::decode(tg_pub_hex).unwrap();
        let tg_pub_arr: [u8; 32] = tg_pub.clone().try_into().unwrap();
        let verifying = ed25519_dalek::VerifyingKey::from_bytes(&tg_pub_arr).unwrap();
        let tbs = &cert[..80];
        let sig_bytes: [u8; 64] = cert[80..].try_into().unwrap();
        let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);
        verifying.verify(tbs, &signature).expect("signature must verify");

        // First 32 bytes of cert == device_pk.
        assert_eq!(&cert[..32], &device_pk[..]);
        // Next 32 bytes == TG pub.
        assert_eq!(&cert[32..64], &tg_pub[..]);
    }

    #[test]
    fn sign_with_no_request_emits_error() {
        let (_apiary, _cfg, ctx, slot, cfg_root) = setup();
        let mut k = KeyholderPlugin::new(0, ctx, slot, cfg_root);
        match k.execute(CMD_SIGN_CERT, &[]) {
            PluginResult::Ok(_) => {}
            _ => panic!(),
        }
        let expected_hash = k.hash_error;
        let (hash, _) = k.poll().expect("expected error event");
        assert_eq!(hash, expected_hash);
    }

    #[test]
    fn no_apiary_errors() {
        let no_ctx: RosterCtx = Arc::new(Mutex::new(None));
        let slot: KeyholderSlot = Arc::new(Mutex::new(None));
        let mut k = KeyholderPlugin::new(0, no_ctx, slot, PathBuf::from("/nonexistent"));
        match k.execute(CMD_FINGERPRINT, &[]) {
            PluginResult::Error(e) => assert_eq!(e.code, ERR_NO_APIARY),
            _ => panic!(),
        }
    }

    #[test]
    fn unknown_command_errors() {
        let no_ctx: RosterCtx = Arc::new(Mutex::new(None));
        let slot: KeyholderSlot = Arc::new(Mutex::new(None));
        let mut k = KeyholderPlugin::new(0, no_ctx, slot, PathBuf::from("/nonexistent"));
        match k.execute(0xAA, &[]) {
            PluginResult::Error(e) => assert_eq!(e.code, ERR_UNKNOWN_COMMAND),
            _ => panic!(),
        }
    }
}
