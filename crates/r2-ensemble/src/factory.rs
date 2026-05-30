//! Sentant construction.
//!
//! v0.1 supports Rust-coded sentants only. The host registers one or
//! more [`SentantFactory`] impls before loading any ensemble; on load,
//! the registry walks each sentant entry in the score and asks every
//! factory in registration order to build it. The first factory whose
//! [`SentantFactory::build`] returns `Ok` wins. If no factory can build
//! a given sentant (typically because the score is purely declarative
//! and no interpreter is registered), [`crate::LoadError::NoFactory`]
//! is returned and the load is rolled back.

use r2_def::SentantDef;
use r2_engine::Sentant;

use crate::error::LoadError;

/// A constructed sentant ready to be inserted into the registry.
///
/// The registry takes ownership and calls `init` immediately after
/// insertion (per `Sentant::init` contract).
pub type BoxedSentant = Box<dyn Sentant + Send + Sync>;

/// Pluggable sentant constructor.
///
/// Implementations can match on `def.class`, `def.name`, or feature
/// flags inside the definition. Returning `Err(LoadError::NoFactory)`
/// signals "I don't handle this kind of sentant" — the registry will
/// keep asking the next factory.
pub trait SentantFactory: Send + Sync {
    /// Build a sentant from its definition. Return
    /// `Err(LoadError::NoFactory { … })` to defer to the next factory.
    fn build(&self, def: &SentantDef) -> Result<BoxedSentant, LoadError>;
}

/// A factory that always defers — useful as the trailing fallback when
/// the registry is configured to allow loading scores even without a
/// matching factory (it isn't, currently, but the type makes test
/// scaffolding ergonomic).
pub struct NoOpFactory;

impl SentantFactory for NoOpFactory {
    fn build(&self, def: &SentantDef) -> Result<BoxedSentant, LoadError> {
        Err(LoadError::NoFactory {
            name: def.name.clone(),
            reason: "NoOpFactory always defers".to_string(),
        })
    }
}
