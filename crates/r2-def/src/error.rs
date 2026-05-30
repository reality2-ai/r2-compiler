//! Error types for r2-def parsing.

use thiserror::Error;

/// Errors produced while parsing a definition file.
#[derive(Debug, Error)]
pub enum DefError {
    /// YAML parse failure.
    #[cfg(feature = "yaml")]
    #[error("yaml parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// JSON parse failure.
    #[cfg(feature = "json")]
    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML parse failure.
    #[cfg(feature = "toml")]
    #[error("toml parse error: {0}")]
    Toml(#[from] toml::de::Error),

    /// A required field was missing or empty (R2-DEF §7.10 / §8.1).
    #[error("validation error: {0}")]
    Validation(String),
}
