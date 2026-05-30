//! # R2-ENSEMBLE
//!
//! Ensemble registry for R2 hives. Loads R2-DEF ensemble scores, tracks
//! live sentant instances, and exposes an
//! [`r2_dispatch::DispatchTarget`] that routes `DeliverOnly` envelopes
//! through the loaded sentants via [`r2_engine`].
//!
//! ## Role
//!
//! When the L3 router (R2-ROUTE) decides a frame is destined for local
//! delivery, it builds a [`r2_dispatch::DispatchEnvelope`] and calls
//! `dispatch()` on whatever target the hive currently has. Until an
//! ensemble runtime is attached, that target is
//! `LogAndDropTarget` — every event is logged and discarded. This crate
//! supplies a real target.
//!
//! The registry plays the role that `R2.Hive` plays on the BEAM side:
//! the lifecycle owner for the loaded ensembles, plus the event-hash
//! subscription index used for delivery.
//!
//! ## What this crate handles
//!
//! - Parsing and validating ensemble scores via [`r2_def`].
//! - Constructing sentant instances via a pluggable
//!   [`SentantFactory`] (Rust-coded sentants in v0.1; the YAML
//!   interpreter is a separate, follow-up crate).
//! - Tracking ensembles in a `HashMap<EnsembleId, LoadedEnsemble>` and
//!   maintaining the event-hash → sentant subscription index.
//! - Implementing [`r2_dispatch::DispatchTarget`]: looking up
//!   subscribers by `event_hash`, calling `Sentant::handle_event`, and
//!   handing actions to a host-supplied
//!   [`OutboundSink`] for the `Send` actions that need to leave the
//!   process.
//!
//! ## What this crate does NOT handle
//!
//! - Wire I/O. Outbound events are pushed to an [`OutboundSink`]
//!   trait-object the host wires up; the host (r2-hive) is responsible
//!   for re-framing and routing them.
//! - Plugin processes. v0.1 records the plugin definitions and lets the
//!   host instantiate them; the registry doesn't fork processes.
//! - Trust group cryptography. Score-signature verification is the
//!   loader's responsibility before calling [`EnsembleRegistry::load`];
//!   the registry trusts the parsed score it receives.

#![deny(missing_docs)]

mod dispatch;
mod error;
mod factory;
mod loaded;
mod registry;
mod supervision;

pub use dispatch::{ensemble_status, CapturingSink, OutboundEvent, OutboundSink};
pub use error::{LoadError, StopError};
pub use factory::{BoxedSentant, NoOpFactory, SentantFactory};
pub use loaded::{EnsembleId, LoadedEnsemble, SentantInstance, SentantInstanceId};
pub use registry::EnsembleRegistry;
pub use supervision::{
    BackoffPolicy, EnsembleStatus, RestartLedger, RestartPolicy, RestartStrategy,
    SupervisionConfig,
};
