//! R2-DEF: parser for sentant, swarm, and ensemble definitions.
//!
//! Implements the file-format side of the R2-DEF specification:
//!
//! - **Sentants** (R2-DEF §2): a single autonomous agent (FSM + plugins).
//! - **Swarms** (R2-DEF §6): a bundle of sentants loaded together (Sentant-only).
//! - **Ensembles** (R2-DEF §7, R2-ENSEMBLE): a Sentant+plugin+registration
//!   composition that the trust group distributes as a unit.
//!
//! This crate handles the *file format only*: it deserialises YAML / JSON /
//! TOML into typed Rust structures, validates light structural rules
//! (required fields, non-empty arrays), and re-serialises losslessly so
//! editor tools / linters / CI validators can build on it.
//!
//! Action and automation *semantics* (what each command does at runtime)
//! are deliberately not modelled here — those belong in the runtime engine
//! (`r2-engine`). The action-pipeline values are kept as opaque
//! `serde_yaml::Value` trees so r2-def doesn't have to track every command
//! the engine adds.
//!
//! ## Quick start
//!
//! ```no_run
//! use r2_def::parse_ensemble_yaml;
//!
//! let yaml = std::fs::read_to_string("notekeeper.yaml").unwrap();
//! let ensemble = parse_ensemble_yaml(&yaml).unwrap();
//! assert_eq!(ensemble.name, "notekeeper");
//! ```

#![deny(missing_docs)]

mod error;
mod ensemble;
mod plugin;
mod sentant;

#[cfg(any(feature = "yaml", feature = "json", feature = "toml"))]
mod parse;

pub use error::DefError;
pub use ensemble::{
    CapabilityAggregate, EnsembleFile, EnsembleScore, SentantEntry, Signature,
    TrustGroupConstraints,
};
pub use plugin::{
    PluginDef, PluginRef, WebChannelDef, WebCspOverride, WebPluginManifest, WebSubscriptionDef,
};
pub use sentant::{Automation, SentantDef, SentantFile, StoragePolicy, SwarmDef, SwarmFile, Transition};

#[cfg(feature = "yaml")]
pub use parse::{parse_ensemble_yaml, parse_sentant_yaml, parse_swarm_yaml};

#[cfg(feature = "json")]
pub use parse::{parse_ensemble_json, parse_sentant_json, parse_swarm_json};

#[cfg(feature = "toml")]
pub use parse::{parse_ensemble_toml, parse_sentant_toml, parse_swarm_toml};
