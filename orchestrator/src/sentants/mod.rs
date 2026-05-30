//! Sentants performed by the orchestrator hive.
//!
//! Per [[feedback-sentants-vs-plugins-terminology]] in memory:
//! sentants here are thin FSMs that route events; the imperative work
//! happens in plugins. Phase 1.7a lands just the Builder; Phase 1.7+
//! adds Author / Deploy / Sync / Tg / Catalogue / Apiary.

pub mod builder;

pub use builder::BuilderSentant;
