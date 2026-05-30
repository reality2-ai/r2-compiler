//! Format-specific parser entry points.

use crate::ensemble::{EnsembleFile, EnsembleScore};
use crate::error::DefError;
use crate::sentant::{SentantDef, SentantFile, SwarmDef, SwarmFile};

// ── YAML ────────────────────────────────────────────────────────────────

/// Parse and validate an ensemble score from YAML text.
#[cfg(feature = "yaml")]
pub fn parse_ensemble_yaml(s: &str) -> Result<EnsembleScore, DefError> {
    let file: EnsembleFile = serde_yaml::from_str(s)?;
    file.ensemble.validate()?;
    Ok(file.ensemble)
}

/// Parse and validate a single sentant definition from YAML text.
#[cfg(feature = "yaml")]
pub fn parse_sentant_yaml(s: &str) -> Result<SentantDef, DefError> {
    let file: SentantFile = serde_yaml::from_str(s)?;
    file.sentant.validate()?;
    Ok(file.sentant)
}

/// Parse and validate a swarm definition from YAML text.
#[cfg(feature = "yaml")]
pub fn parse_swarm_yaml(s: &str) -> Result<SwarmDef, DefError> {
    let file: SwarmFile = serde_yaml::from_str(s)?;
    file.swarm.validate()?;
    Ok(file.swarm)
}

// ── JSON ────────────────────────────────────────────────────────────────

/// Parse and validate an ensemble score from JSON text.
#[cfg(feature = "json")]
pub fn parse_ensemble_json(s: &str) -> Result<EnsembleScore, DefError> {
    let file: EnsembleFile = serde_json::from_str(s)?;
    file.ensemble.validate()?;
    Ok(file.ensemble)
}

/// Parse and validate a single sentant definition from JSON text.
#[cfg(feature = "json")]
pub fn parse_sentant_json(s: &str) -> Result<SentantDef, DefError> {
    let file: SentantFile = serde_json::from_str(s)?;
    file.sentant.validate()?;
    Ok(file.sentant)
}

/// Parse and validate a swarm definition from JSON text.
#[cfg(feature = "json")]
pub fn parse_swarm_json(s: &str) -> Result<SwarmDef, DefError> {
    let file: SwarmFile = serde_json::from_str(s)?;
    file.swarm.validate()?;
    Ok(file.swarm)
}

// ── TOML ────────────────────────────────────────────────────────────────

/// Parse and validate an ensemble score from TOML text.
#[cfg(feature = "toml")]
pub fn parse_ensemble_toml(s: &str) -> Result<EnsembleScore, DefError> {
    let file: EnsembleFile = toml::from_str(s)?;
    file.ensemble.validate()?;
    Ok(file.ensemble)
}

/// Parse and validate a single sentant definition from TOML text.
#[cfg(feature = "toml")]
pub fn parse_sentant_toml(s: &str) -> Result<SentantDef, DefError> {
    let file: SentantFile = toml::from_str(s)?;
    file.sentant.validate()?;
    Ok(file.sentant)
}

/// Parse and validate a swarm definition from TOML text.
#[cfg(feature = "toml")]
pub fn parse_swarm_toml(s: &str) -> Result<SwarmDef, DefError> {
    let file: SwarmFile = toml::from_str(s)?;
    file.swarm.validate()?;
    Ok(file.swarm)
}
