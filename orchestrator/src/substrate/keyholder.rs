//! `keyholder` substrate — custodian of the active apiary's TG Ed25519
//! private key.
//!
//! Per R2-HIVE §2.1 + R2-TRUST §5.5 (the "Key Holder" role).
//!
//! ## v0.1 scope (post-F4b)
//!
//! Lazy-loads the TG signing key from the off-tree path on first use.
//! Exposes the public-key fingerprint via `CMD_FINGERPRINT`. That's
//! it — DeviceCertificate minting moved into the
//! [`crate::substrate::provision_handshake`] substrate when F4b
//! landed and retired F3's hand-rolled 144-byte cert format.
//!
//! ## Why keep this around at all?
//!
//! - The CMD_FINGERPRINT command is used by the Stack view's L5 row
//!   ("substrate/keyholder · TG fp …") and the chat author flow when
//!   the operator asks "what's the apiary's TG fingerprint?".
//! - Apiary creation (SPEC-APIARY-CREATE §2.2) generates the keypair
//!   today via the AI's Write tool; formalising that here is an F3b
//!   task — adding `CMD_GENERATE` / `CMD_IMPORT` to this same plugin.
//! - Loading the key once + caching is the natural home for it; other
//!   substrates that need the SK (provision-handshake) currently do
//!   their own load. F4c will consolidate by routing all sign-with-SK
//!   requests through this plugin.
//!
//! ## What changed in F4b
//!
//! - `CMD_SIGN_CERT` removed — the 144-byte fake cert format it
//!   produced was never the R2-TRUST DeviceCertificate format. Real
//!   cert minting now happens via
//!   `TrustGroup::process_join_request` inside
//!   `substrate::provision_handshake`.
//! - `SignCertRequest` + `KeyholderSlot` types removed.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::sentants::RosterCtx;

/// Compatibility shim — F4b retired the SIGN_CERT path, but keeping
/// the type alias avoids a wider rename until consumers update. The
/// shared slot is no longer dispatched against; we keep the type so
/// hive.rs's existing construction doesn't break.
pub type KeyholderSlot = Arc<Mutex<Option<SignCertRequest>>>;

/// Deprecated as of F4b — the JoinResponse path inside
/// [`crate::substrate::provision_handshake`] produces a real R2-TRUST
/// `DeviceCertificate` (147 bytes), not this fake 144-byte format.
/// Kept to avoid a wider rename until consumers update.
#[derive(Debug, Clone)]
pub struct SignCertRequest {
    pub device_pk: [u8; 32],
    pub valid_secs: u64,
    pub slot_id: String,
}

pub const CMD_FINGERPRINT: PluginCommand = 0x02;

pub const ERR_NO_APIARY:       u8 = 0x01;
pub const ERR_KEY_LOAD_FAILED: u8 = 0x02;
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

pub struct KeyholderPlugin {
    id: PluginId,
    apiary_ctx: RosterCtx,
    #[allow(dead_code)] // reserved for F4c SIGN_X commands
    sign_slot: KeyholderSlot,
    /// Root for off-tree key storage. Production:
    /// `$HOME/.config/r2-composer/`. Tests override.
    config_root: PathBuf,
    cached_key: Option<LoadedKey>,
    out_buf: Vec<u8>,
}

struct LoadedKey {
    public: [u8; 32],
    fingerprint: String,
    #[allow(dead_code)] // F4c will use for additional sign-X commands
    signing: ed25519_dalek::SigningKey,
    #[allow(dead_code)] // diagnostics only
    priv_path: PathBuf,
}

impl KeyholderPlugin {
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
            out_buf: Vec::with_capacity(128),
        }
    }

    pub fn default_config_root() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".config/r2-composer")
    }

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
            public, fingerprint, signing, priv_path,
        });
        Ok(())
    }

    fn handle_fingerprint(&mut self) -> PluginResult {
        if let Err((code, msg)) = self.ensure_loaded() {
            return PluginResult::Error(PluginError::new(code, &msg));
        }
        let fp = self.cached_key.as_ref().unwrap().fingerprint.clone();
        let bytes = self.pack_with(&serde_json::json!({ "fp": fp }));
        PluginResult::Ok(PluginResponse::with_data(&bytes))
    }

    fn pack_with<T: Serialize>(&mut self, v: &T) -> Vec<u8> {
        self.out_buf.clear();
        let _ = serde_json::to_writer(&mut self.out_buf, v);
        self.out_buf.clone()
    }
}

impl Plugin for KeyholderPlugin {
    fn execute(&mut self, command: PluginCommand, _data: &[u8]) -> PluginResult {
        match command {
            CMD_FINGERPRINT => self.handle_fingerprint(),
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "unknown command")),
        }
    }
    fn name(&self) -> &str { "keyholder" }
    fn id(&self) -> PluginId { self.id }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                assert_eq!(fp.len(), 64);
                assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
            }
            PluginResult::Error(e) => panic!("fp failed: code={:02X} {}", e.code, e.description()),
        }
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
