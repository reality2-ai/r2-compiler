//! # R2-DISPATCH
//!
//! Local event dispatch contract for R2 hives. Sits at the boundary between L3
//! routing (R2-ROUTE) and L5+ sentant runtimes (R2-SENTANT, R2-ENSEMBLE).
//!
//! ## Purpose
//!
//! R2-ROUTE's `ForwardAction::DeliverOnly` signifies that a frame has reached its
//! destination and should be processed locally rather than relayed further. Until
//! this crate existed, every implementation wired its own ad-hoc path from
//! "DeliverOnly" to its sentant runtime. This crate defines the normative contract
//! that every r2-hive implementation MUST expose to its runtime, such that apps and
//! ensembles written against the spec can be delivered events by any conformant hive.
//!
//! The contract is intentionally minimal:
//!
//! - **`DispatchEnvelope`** — what to deliver (originator, target, event class,
//!   payload, trust-group context)
//! - **`DispatchTarget`** — the trait a runtime implements to receive envelopes
//! - **`DispatchError`** — what can go wrong and how to recover
//!
//! Everything else — sentant lifecycle, plugin process management, state
//! persistence, BEAM process per sentant, WASM sandboxes, MCU dispatch tables — is
//! the runtime's concern, not this crate's.
//!
//! ## Relationship to the R2 spec set
//!
//! Per the *one-crate-per-spec* principle (DEV-PLAN Track B), this crate is the
//! Rust implementation of the normative dispatch contract described in R2-RUNTIME
//! §2.4. An alternative-language implementer (BEAM, Go, Python, WASM) reads the
//! spec and builds the same contract in that runtime. Apps work across all
//! implementations because the contract surface is stable.
//!
//! ## Usage
//!
//! ```ignore
//! use r2_dispatch::{DispatchEnvelope, DispatchTarget, LogAndDropTarget};
//! use std::sync::Arc;
//!
//! // Default: hive with no runtime just logs and drops every DeliverOnly event.
//! let target: Arc<dyn DispatchTarget> = Arc::new(LogAndDropTarget);
//!
//! // When an ensemble loader (R2-ENSEMBLE §4.2 / R2-DEF §7) wires in a real
//! // runtime, it installs its own DispatchTarget implementation here.
//! // target = Arc::new(MyEnsembleRuntimeDispatcher::new());
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

mod envelope;
mod target;

#[cfg(feature = "std")]
mod log_drop;

pub use envelope::DispatchEnvelope;
pub use target::{DispatchError, DispatchTarget};

#[cfg(feature = "std")]
pub use log_drop::LogAndDropTarget;
