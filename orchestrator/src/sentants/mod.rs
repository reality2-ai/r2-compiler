//! Sentants performed by the orchestrator hive.
//!
//! Per [[feedback-sentants-vs-plugins-terminology]] in memory:
//! sentants here are thin FSMs that route events; the imperative work
//! happens in plugins. Phase 1.7a lands Builder; Phase 1.7d adds
//! Author; Phase 1.7+ adds Deploy / Sync / Tg / Catalogue / Apiary.

pub mod author;
pub mod builder;
pub mod roster;

pub use author::AuthorSentant;
pub use builder::BuilderSentant;
pub use roster::{RosterCtx, RosterSentant};
