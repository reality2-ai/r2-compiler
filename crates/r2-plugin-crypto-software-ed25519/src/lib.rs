//! # r2-plugin-crypto-software-ed25519
//!
//! R2 dual-mode plugin: pure-software Ed25519 signing, verification, and
//! keypair derivation. Implements the [`r2_engine::plugin::Plugin`] trait
//! per R2-PLUGIN §12.
//!
//! ## Commands
//!
//! | Byte | Name     | Input                                    | Output                |
//! |------|----------|------------------------------------------|-----------------------|
//! | 0x01 | sign     | `[32 B sk \| N B msg]`                   | `[64 B sig]`          |
//! | 0x02 | verify   | `[32 B pk \| 64 B sig \| N B msg]`       | `[1 B: 0 ok / 1 bad]` |
//! | 0x03 | generate | `[32 B seed]`                            | `[32 B pk \| 32 B sk]`|
//!
//! See the crate `README.md` for full usage, example sentants, and the
//! R2-PROVISION / R2-TRUST integration notes.
//!
//! ## Modes
//!
//! - `aot` — static link into firmware via r2-forge (`no_std`).
//! - `nif` — dynamic load into a BEAM hive via r2-nif (enables `std`).
//!
//! The two features are mutually exclusive.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]

#[cfg(all(feature = "aot", feature = "nif"))]
compile_error!("features `aot` and `nif` are mutually exclusive");

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};

/// Command opcode: sign a message with a 32-byte secret-key seed.
pub const CMD_SIGN: PluginCommand = 0x01;
/// Command opcode: verify a signature against a public key and message.
pub const CMD_VERIFY: PluginCommand = 0x02;
/// Command opcode: derive a keypair from a 32-byte seed.
pub const CMD_GENERATE: PluginCommand = 0x03;

/// Error: input byte length did not match the command's required layout.
pub const ERR_BAD_LENGTH: u8 = 0x01;
/// Error: public or verifying key failed to parse.
pub const ERR_BAD_KEY: u8 = 0x03;
/// Error: command byte was not recognised.
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

/// Response byte indicating a signature verified successfully.
pub const VERIFY_OK: u8 = 0x00;
/// Response byte indicating a signature failed to verify.
pub const VERIFY_BAD: u8 = 0x01;

/// Software Ed25519 plugin.
///
/// Holds no state other than its [`PluginId`]. Signing and verification are
/// stateless and deterministic; keypair generation derives from a
/// caller-supplied 32-byte seed so the plugin never needs an RNG.
pub struct SoftwareEd25519 {
    id: PluginId,
}

impl SoftwareEd25519 {
    /// Construct a new instance bound to `id`. The ID is unique within a hive
    /// and is assigned by the hive at registration time.
    pub const fn new(id: PluginId) -> Self {
        Self { id }
    }

    fn op_sign(data: &[u8]) -> PluginResult {
        if data.len() < 32 {
            return PluginResult::Error(PluginError::new(
                ERR_BAD_LENGTH,
                "sign: need >= 32 B (sk || msg)",
            ));
        }
        let mut sk_bytes = [0u8; 32];
        sk_bytes.copy_from_slice(&data[..32]);
        let message = &data[32..];
        let sk = SigningKey::from_bytes(&sk_bytes);
        let sig = sk.sign(message);
        PluginResult::Ok(PluginResponse::with_data(&sig.to_bytes()))
    }

    fn op_verify(data: &[u8]) -> PluginResult {
        if data.len() < 96 {
            return PluginResult::Error(PluginError::new(
                ERR_BAD_LENGTH,
                "verify: need >= 96 B (pk || sig || msg)",
            ));
        }
        let mut pk_bytes = [0u8; 32];
        pk_bytes.copy_from_slice(&data[..32]);
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&data[32..96]);
        let message = &data[96..];

        let pk = match VerifyingKey::from_bytes(&pk_bytes) {
            Ok(k) => k,
            Err(_) => {
                return PluginResult::Error(PluginError::new(
                    ERR_BAD_KEY,
                    "verify: bad public key",
                ));
            }
        };
        let sig = Signature::from_bytes(&sig_bytes);

        let byte = match pk.verify(message, &sig) {
            Ok(()) => VERIFY_OK,
            Err(_) => VERIFY_BAD,
        };
        PluginResult::Ok(PluginResponse::with_data(&[byte]))
    }

    fn op_generate(data: &[u8]) -> PluginResult {
        if data.len() != 32 {
            return PluginResult::Error(PluginError::new(
                ERR_BAD_LENGTH,
                "generate: need exactly 32 B (seed)",
            ));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(data);
        let sk = SigningKey::from_bytes(&seed);
        let pk = sk.verifying_key();
        let mut out = [0u8; 64];
        out[..32].copy_from_slice(pk.as_bytes());
        out[32..].copy_from_slice(&sk.to_bytes());
        PluginResult::Ok(PluginResponse::with_data(&out))
    }
}

impl Plugin for SoftwareEd25519 {
    fn execute(&mut self, command: PluginCommand, data: &[u8]) -> PluginResult {
        match command {
            CMD_SIGN => Self::op_sign(data),
            CMD_VERIFY => Self::op_verify(data),
            CMD_GENERATE => Self::op_generate(data),
            _ => PluginResult::Error(PluginError::new(
                ERR_UNKNOWN_COMMAND,
                "unknown command byte",
            )),
        }
    }

    fn name(&self) -> &str {
        "crypto/software-ed25519"
    }

    fn id(&self) -> PluginId {
        self.id
    }
}
