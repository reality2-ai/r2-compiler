//! `DispatchTarget` trait and `DispatchError` — the contract between a hive's router
//! and whatever sentant runtime is attached to it.
//!
//! A hive holds exactly one `DispatchTarget` at a time (typically `Arc<dyn DispatchTarget>`).
//! When the route engine resolves a frame to `ForwardAction::DeliverOnly`, the router
//! constructs a `DispatchEnvelope` and calls `dispatch()` on the target. The target's
//! behaviour is opaque to the router — it may run sentants in BEAM processes, in a Rust
//! state-machine dispatcher, in a WebAssembly sandbox, or drop the event entirely.
//!
//! Per R2-RUNTIME §2.4, every conformant r2-hive implementation MUST expose this
//! contract to its sentant runtime. The default target on a freshly-started hive with
//! no ensembles loaded is `LogAndDropTarget` (or its equivalent), which returns
//! `DispatchError::NoHandler` for every dispatch so upstream code can detect the
//! absence of a live runtime.

use crate::DispatchEnvelope;

/// Result of a dispatch call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchError {
    /// No sentant or plugin is registered to handle this event class. The hive's
    /// capability bloom SHOULD NOT advertise this event class when `NoHandler` is the
    /// typical result, so a peer querying capabilities discovers this hive is not a
    /// useful handler.
    NoHandler,

    /// A handler rejected the event for policy reasons (e.g. trust-group mismatch,
    /// capability revoked, sentant in a state that refuses this event).
    Rejected,

    /// The handler is overloaded; the caller SHOULD back off and retry. Transport-level
    /// retransmission (R2-WIRE §7 spray-and-wait) will eventually redeliver; dispatch
    /// backpressure signals the hive to drop rather than queue.
    Backpressure,

    /// Transient I/O or runtime error. The event may be delivered on retry.
    Io,

    /// The envelope was malformed — payload doesn't match the event class schema, CBOR
    /// decode failure, etc. This is a protocol-level error; the event is unsafe to
    /// retry with the same payload.
    Invalid,
}

/// The dispatch contract — async on `std`, sync on no_std.
///
/// On `std`, uses `async_trait` so runtime-specific async implementations (Tokio on
/// r2-hive, async-std elsewhere) can plug in directly.
///
/// On no_std, implementations are expected to be sync and non-blocking; the MCU
/// runtime pattern (A6 in the dev plan) wires dispatch into a fixed dispatch table
/// and returns synchronously.
#[cfg(feature = "std")]
#[async_trait::async_trait]
pub trait DispatchTarget: Send + Sync {
    async fn dispatch(&self, envelope: DispatchEnvelope<'_>) -> Result<(), DispatchError>;
}

/// Sync flavour of the dispatch contract, usable from no_std contexts. Not required
/// to be `Send + Sync` — MCU dispatchers typically run on a single RTOS task.
#[cfg(not(feature = "std"))]
pub trait DispatchTarget {
    fn dispatch(&self, envelope: DispatchEnvelope<'_>) -> Result<(), DispatchError>;
}
