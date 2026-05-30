//! Error types for the ensemble registry.

use thiserror::Error;

/// Errors returned from [`crate::EnsembleRegistry::load`].
#[derive(Debug, Error)]
pub enum LoadError {
    /// The score failed structural validation (R2-DEF §7.10).
    #[error("score validation failed: {0}")]
    Validation(String),

    /// The ensemble is already loaded under this id.
    #[error("ensemble '{0}' is already loaded")]
    AlreadyLoaded(String),

    /// No registered [`crate::SentantFactory`] could build a sentant
    /// referenced by the score. Typically means a YAML-defined sentant
    /// without an interpreter present — Rust-coded sentants must be
    /// registered up-front.
    #[error("no factory could build sentant '{name}': {reason}")]
    NoFactory {
        /// Sentant name that failed to instantiate.
        name: String,
        /// Why every factory rejected it (typically: needs interpreter).
        reason: String,
    },

    /// An external sentant include reference is unsupported in v0.1.
    /// Inline `SentantEntry::Inline` is required.
    #[error("external sentant include '{0}' not supported in v0.1; inline only")]
    ExternalIncludeUnsupported(String),

    /// The score canonicalised to an event class that doesn't FNV-hash
    /// (only happens for non-ASCII inputs that R2-FNV rejects).
    #[error("event class '{0}' is not a valid FNV input")]
    BadEventClass(String),
}

/// Errors returned from [`crate::EnsembleRegistry::stop`].
#[derive(Debug, Error)]
pub enum StopError {
    /// No ensemble is loaded under the given id.
    #[error("ensemble '{0}' is not loaded")]
    NotLoaded(String),
}
