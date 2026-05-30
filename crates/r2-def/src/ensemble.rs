//! Ensemble score types per R2-DEF §7 and R2-ENSEMBLE.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::plugin::PluginDef;
use crate::sentant::SentantDef;

/// File-level wrapper for an ensemble score (`ensemble: ...` at the top
/// of YAML/JSON/TOML).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EnsembleFile {
    /// The ensemble score.
    pub ensemble: EnsembleScore,
}

/// An ensemble score per R2-DEF §7.1.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EnsembleScore {
    /// Human-readable identifier (e.g. `notekeeper`).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Semver of the ensemble's content.
    pub version: String,
    /// Reverse-DNS class string (R2-CAP §3, RECOMMENDED).
    #[serde(default)]
    pub class: Option<String>,
    /// Schema version this file targets — currently `"0.1"`.
    pub ensemble_version: String,
    /// One or more sentant entries (inline or external reference).
    pub sentants: Vec<SentantEntry>,
    /// Ensemble-owned plugins (R2-DEF §7.3).
    #[serde(default)]
    pub plugins: Vec<PluginDef>,
    /// Registrations with hive-shared singleton plugins (R2-DEF §7.4).
    /// Key = singleton plugin name (e.g. `r2-web`), value = the
    /// singleton's registration payload (shape defined by that singleton's
    /// own spec).
    #[serde(default)]
    pub registrations: BTreeMap<String, serde_yaml::Value>,
    /// Aggregate event-level capabilities (R2-DEF §7.5).
    #[serde(default)]
    pub capabilities: Option<CapabilityAggregate>,
    /// Trust-group constraints (R2-DEF §7.6).
    #[serde(default)]
    pub trust_group: Option<TrustGroupConstraints>,
    /// Default compile targets (R2-DEF §7.7). Per-part compile_target overrides.
    #[serde(default)]
    pub compile_target: Vec<String>,
    /// Ed25519 signatures (R2-DEF §7.8). REQUIRED for distributed form;
    /// OPTIONAL for local dev.
    #[serde(default)]
    pub signatures: Vec<Signature>,
}

/// One entry in an ensemble's `sentants` array — either inline or by
/// external reference. R2-DEF §7.2.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SentantEntry {
    /// Path to an external sentant definition file. Resolved relative to
    /// the ensemble file.
    External {
        /// Relative path to the sentant file.
        include: String,
    },
    /// Inline sentant definition.
    Inline(Box<SentantDef>),
}

/// Aggregate event-level capabilities the ensemble emits and consumes
/// (R2-DEF §7.5).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CapabilityAggregate {
    /// Events the ensemble emits.
    #[serde(default)]
    pub emits: Vec<String>,
    /// Events the ensemble consumes.
    #[serde(default)]
    pub consumes: Vec<String>,
}

/// Trust-group constraints the loading hive must satisfy (R2-DEF §7.6).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TrustGroupConstraints {
    /// Minimum R2-TRUST crypto level (e.g. `"standard"`, `"quantum-safe"`).
    #[serde(default)]
    pub min_crypto_level: Option<String>,
    /// R2-TRUST roles permitted to load this ensemble.
    #[serde(default)]
    pub roles_allowed: Vec<String>,
    /// Entanglement scope declarations (left opaque — depends on
    /// R2-CAP §12.5 spec maturity).
    #[serde(default)]
    pub entanglement_scope: Vec<serde_yaml::Value>,
}

/// One Ed25519 signature over the ensemble score (R2-DEF §7.8).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Signature {
    /// FNV-1a hash of signer device UUID, hex-encoded.
    pub signer: String,
    /// Algorithm identifier (e.g. `"ed25519"`).
    pub algorithm: String,
    /// Base64-encoded signature bytes.
    pub signature: String,
    /// Unix timestamp of signing.
    pub signed_at: u64,
    /// Signer's role: `keyholder` | `member` | `external`.
    #[serde(default)]
    pub scope: Option<String>,
}

impl EnsembleScore {
    /// Light structural validation per R2-DEF §7.10. Heavier rules
    /// (signature verification, registration conflicts, compile-target
    /// loadability) are runtime concerns.
    pub fn validate(&self) -> Result<(), crate::DefError> {
        if self.name.is_empty() {
            return Err(crate::DefError::Validation(
                "ensemble.name is empty (E_ENS_FIELD_MISSING)".into(),
            ));
        }
        if self.description.is_empty() {
            return Err(crate::DefError::Validation(
                "ensemble.description is empty (E_ENS_FIELD_MISSING)".into(),
            ));
        }
        if self.version.is_empty() {
            return Err(crate::DefError::Validation(
                "ensemble.version is empty (E_ENS_FIELD_MISSING)".into(),
            ));
        }
        if self.ensemble_version.is_empty() {
            return Err(crate::DefError::Validation(
                "ensemble.ensemble_version is empty (E_ENS_FIELD_MISSING)".into(),
            ));
        }
        // Currently only schema version "0.1" is recognised (R2-DEF §7.10).
        if self.ensemble_version != "0.1" {
            return Err(crate::DefError::Validation(format!(
                "ensemble_version '{}' not recognised (E_ENS_SCHEMA_VERSION)",
                self.ensemble_version
            )));
        }
        if self.sentants.is_empty() {
            return Err(crate::DefError::Validation(
                "ensemble.sentants is empty (E_ENS_NO_SENTANTS)".into(),
            ));
        }
        // Plugin name uniqueness.
        let mut seen = std::collections::HashSet::new();
        for p in &self.plugins {
            if !seen.insert(p.name.as_str()) {
                return Err(crate::DefError::Validation(format!(
                    "ensemble.plugins: duplicate name '{}' (E_ENS_PLUGIN_DUP)",
                    p.name
                )));
            }
        }
        // Validate inline sentants individually.
        for entry in &self.sentants {
            if let SentantEntry::Inline(s) = entry {
                s.validate()?;
            }
        }
        Ok(())
    }
}
