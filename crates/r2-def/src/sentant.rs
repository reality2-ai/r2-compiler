//! Sentant and swarm definition types per R2-DEF §2 and §6.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::plugin::PluginRef;

/// File-level wrapper for a single sentant definition (`sentant: ...` at
/// the top of YAML/JSON/TOML).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SentantFile {
    /// The sentant definition.
    pub sentant: SentantDef,
}

/// One sentant — an autonomous FSM-based agent (R2-DEF §2).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SentantDef {
    /// Human-readable name. Used for display and routing within a hive.
    pub name: String,
    /// Reverse-DNS class string (RECOMMENDED, R2-DEF §2.2).
    #[serde(default)]
    pub class: Option<String>,
    /// Human-readable description.
    pub description: String,
    /// Storage persistence policy (R2-DEF §2.3).
    #[serde(default)]
    pub storage: StoragePolicy,
    /// Initial vars. Kept opaque — runtime interprets the values.
    #[serde(default)]
    pub data: serde_yaml::Value,
    /// Plugin bindings the sentant uses (R2-DEF §4).
    #[serde(default)]
    pub plugins: Vec<PluginRef>,
    /// Automations (FSMs) — at least one REQUIRED per R2-DEF §2.
    pub automations: Vec<Automation>,
}

/// Persistence levels per R2-DEF §2.3 / R2-SENTANT §2.2.1.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StoragePolicy {
    /// Definition and state both gone on reboot. Default.
    #[default]
    Volatile,
    /// Definition persisted; sentant restarts fresh on boot.
    Durable,
    /// Definition and last state snapshot persisted; resumes on boot.
    DurableState,
}

/// Finite-state machine within a sentant.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Automation {
    /// Unique within the sentant.
    pub name: String,
    /// Optional human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// Flat array of transitions per R2-DEF §3.1.
    pub transitions: Vec<Transition>,
}

/// One transition rule per R2-DEF §3.1.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Transition {
    /// Source state. Default `"*"` (any).
    #[serde(default)]
    pub from: Option<String>,
    /// Event name to match (REQUIRED).
    pub event: String,
    /// Destination state. Default: stay in current.
    #[serde(default)]
    pub to: Option<String>,
    /// Whether the event is part of the public interface.
    #[serde(default)]
    pub public: bool,
    /// Origin filter: `internal` | `external` | `any` (R2-SENTANT §5.4).
    #[serde(default)]
    pub origin: Option<String>,
    /// Parameter type schema (informative).
    #[serde(default)]
    pub parameters: BTreeMap<String, String>,
    /// Action pipeline. Each entry is interpreted by the runtime engine
    /// (`r2-engine`); r2-def keeps them as opaque values to avoid drift
    /// when the engine adds commands.
    #[serde(default)]
    pub actions: Vec<serde_yaml::Value>,
}

/// File-level wrapper for a swarm definition (R2-DEF §6).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SwarmFile {
    /// The swarm definition.
    pub swarm: SwarmDef,
}

/// A swarm — a bundle of sentants loaded together (R2-DEF §6).
///
/// Note: per R2-SENTANT §9.2, swarms are not protocol-visible — they are a
/// deployment convenience. R2-ENSEMBLE supersedes swarms for trust-group
/// distribution; swarm form remains for backward-compat and local dev.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SwarmDef {
    /// Human-readable swarm name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Inline sentant definitions, instantiated in order.
    pub sentants: Vec<SentantDef>,
}

impl SentantDef {
    /// Light structural validation per R2-DEF §8.1.
    /// Heavier rules (action-command validity, plugin-ref resolution) are
    /// the runtime's responsibility.
    pub fn validate(&self) -> Result<(), crate::DefError> {
        if self.name.is_empty() {
            return Err(crate::DefError::Validation(
                "sentant.name is empty (E_DEF_NAME_MISSING)".into(),
            ));
        }
        if self.description.is_empty() {
            return Err(crate::DefError::Validation(
                "sentant.description is empty (E_DEF_DESC_MISSING)".into(),
            ));
        }
        if self.automations.is_empty() {
            return Err(crate::DefError::Validation(
                "sentant.automations is empty (E_DEF_AUTOMATIONS_EMPTY)".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for a in &self.automations {
            if !seen.insert(a.name.as_str()) {
                return Err(crate::DefError::Validation(format!(
                    "sentant.automations: duplicate name '{}' (E_DEF_AUTOMATION_DUP)",
                    a.name
                )));
            }
        }
        let mut plugin_seen = std::collections::HashSet::new();
        for p in &self.plugins {
            if !plugin_seen.insert(p.name.as_str()) {
                return Err(crate::DefError::Validation(format!(
                    "sentant.plugins: duplicate name '{}' (E_DEF_PLUGIN_DUP)",
                    p.name
                )));
            }
        }
        Ok(())
    }
}

impl SwarmDef {
    /// Light structural validation per R2-DEF §8.1.
    pub fn validate(&self) -> Result<(), crate::DefError> {
        if self.sentants.is_empty() {
            return Err(crate::DefError::Validation(
                "swarm.sentants is empty (E_DEF_SWARM_EMPTY)".into(),
            ));
        }
        for s in &self.sentants {
            s.validate()?;
        }
        Ok(())
    }
}
