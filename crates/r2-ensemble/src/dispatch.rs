//! `DispatchTarget` impl + outbound-event sink.
//!
//! When an envelope arrives, the registry:
//!
//! 1. Resolves the event hash to a list of subscribed
//!    [`crate::SentantInstance`]s in the loaded ensembles.
//! 2. For each, locks its sentant, runs `handle_event` inside a
//!    `catch_unwind`, and collects emitted actions.
//! 3. On panic: marks the instance crashed and asks the supervisor to
//!    restart per the ensemble's [`crate::SupervisionConfig`].
//! 4. On success: applies state transitions and forwards
//!    `Action::Send` to the host's [`OutboundSink`].
//!
//! The sink is the host-supplied bridge to wire / mesh I/O: the
//! ensemble registry never frames or routes events itself.

use std::sync::Arc;

use async_trait::async_trait;
use r2_dispatch::{DispatchEnvelope, DispatchError, DispatchTarget};
use r2_engine::{Action, Target};

use crate::loaded::{EnsembleId, SentantInstanceId};
use crate::registry::EnsembleRegistry;
use crate::supervision::EnsembleStatus;

/// One outbound event produced by a sentant action.
///
/// The host (r2-hive) consumes these and frames them onto the wire
/// using its existing R2-WIRE encoder + transport selection.
#[derive(Debug, Clone)]
pub struct OutboundEvent {
    /// The ensemble that emitted the event.
    pub source_ensemble: EnsembleId,
    /// The sentant that emitted the event.
    pub source_instance: SentantInstanceId,
    /// Where the sentant asked the event to go.
    pub target: Target,
    /// FNV-1a hash of the event class.
    pub event_hash: u32,
    /// CBOR payload bytes.
    pub payload: Vec<u8>,
    /// Hash of the trust group context the dispatching envelope
    /// belonged to (forwarded so the host can choose the right TG to
    /// emit on).
    pub trust_group: Option<[u8; 8]>,
    /// Hive id of the event's originator (`DispatchEnvelope::originator`).
    /// Required for resolving `Target::Sender`. `None` only when the
    /// event was emitted from `Sentant::init`, which has no originator.
    pub originator: Option<u32>,
    /// Wire `msg_id` of the inbound event that triggered this outbound
    /// (forwarded from `DispatchEnvelope::msg_id`). Used as the
    /// reply-correlation field on the outbound frame.
    pub trigger_msg_id: u32,
}

/// Host-supplied delivery sink.
///
/// The registry calls `deliver` for every `Action::Send` produced by a
/// sentant. The host is responsible for re-framing and routing.
#[async_trait]
pub trait OutboundSink: Send + Sync {
    /// Deliver an outbound event. Errors are logged by the registry
    /// and dropped — the sentant has already moved on.
    async fn deliver(&self, event: OutboundEvent);
}

/// A sink that records events into a `Mutex<Vec<…>>`. Useful for
/// tests; not intended for production.
#[derive(Default)]
pub struct CapturingSink {
    /// Captured events in arrival order.
    pub events: parking_lot::Mutex<Vec<OutboundEvent>>,
}

#[async_trait]
impl OutboundSink for CapturingSink {
    async fn deliver(&self, event: OutboundEvent) {
        self.events.lock().push(event);
    }
}

#[async_trait]
impl DispatchTarget for EnsembleRegistry {
    async fn dispatch(&self, envelope: DispatchEnvelope<'_>) -> Result<(), DispatchError> {
        let subscribers = self.subscribers_for(envelope.event_hash);
        if subscribers.is_empty() {
            return Err(DispatchError::NoHandler);
        }

        let mut any_dispatched = false;
        let mut last_err: Option<DispatchError> = None;

        for (ensemble_id, instance_id) in subscribers {
            // Skip Failed ensembles entirely.
            if self.is_ensemble_failed(&ensemble_id) {
                continue;
            }

            match self.dispatch_to(&ensemble_id, instance_id, &envelope).await {
                Ok(()) => any_dispatched = true,
                Err(e) => last_err = Some(e),
            }
        }

        if any_dispatched {
            Ok(())
        } else {
            Err(last_err.unwrap_or(DispatchError::NoHandler))
        }
    }
}

/// Per-instance dispatch outcome (internal).
pub(crate) enum DispatchOutcome {
    /// Handler ran cleanly and produced these actions.
    Ok(Vec<Action>),
    /// Handler panicked — caller should consult supervisor.
    Crashed,
    /// Instance is currently gated (restart in flight).
    Gated,
}

/// Run `handle_event` on a sentant under a panic guard. Returns the
/// emitted actions on success, or `Crashed` on panic.
///
/// The instance's `Mutex` is locked only for the duration of the
/// handler — actions are returned by value so the sink can be invoked
/// without holding the lock.
pub(crate) fn run_handler(
    inst: &crate::loaded::SentantInstance,
    event: r2_engine::Event<'_>,
) -> DispatchOutcome {
    {
        let gate = inst.restarting_until.lock();
        if let Some(t) = *gate {
            if t > std::time::Instant::now() {
                return DispatchOutcome::Gated;
            }
        }
    }

    let inner = Arc::clone(&inst.inner);

    // The closure mutates the boxed sentant; AssertUnwindSafe is
    // required because Box<dyn Sentant> isn't UnwindSafe in general.
    // On panic we throw the box away and rebuild via the factory, so
    // observing a partially-mutated sentant is fine.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut guard = inner.lock();
        let mut buf = r2_engine::ActionBuf::new();
        guard.handle_event(&event, &mut buf);
        let mut out: Vec<Action> = Vec::with_capacity(buf.len());
        for a in buf.iter() {
            out.push(a.clone());
        }
        out
    }));

    match result {
        Ok(actions) => DispatchOutcome::Ok(actions),
        Err(_panic_payload) => DispatchOutcome::Crashed,
    }
}

/// Convert a `Sentant` action into an `OutboundEvent`. Returns `None`
/// for actions that don't generate outbound traffic (transitions, log
/// lines, plugin calls — handled in-process).
pub(crate) fn action_to_outbound(
    ensemble_id: &EnsembleId,
    instance_id: SentantInstanceId,
    trust_group: Option<[u8; 8]>,
    originator: Option<u32>,
    trigger_msg_id: u32,
    action: Action,
) -> Option<OutboundEvent> {
    match action {
        Action::Send {
            target,
            event_hash,
            payload,
        } => Some(OutboundEvent {
            source_ensemble: ensemble_id.clone(),
            source_instance: instance_id,
            target,
            event_hash,
            payload: payload.as_slice().to_vec(),
            trust_group,
            originator,
            trigger_msg_id,
        }),
        // DelayedSend: v0.1 lowers to immediate Send; the timer machinery
        // belongs in r2-hive (which has the tokio runtime + clock).
        // Surfaced via the same OutboundEvent so r2-hive can schedule it.
        Action::DelayedSend {
            delay_ms: _,
            target,
            event_hash,
            payload,
        } => Some(OutboundEvent {
            source_ensemble: ensemble_id.clone(),
            source_instance: instance_id,
            target,
            event_hash,
            payload: payload.as_slice().to_vec(),
            trust_group,
            originator,
            trigger_msg_id,
        }),
        // Transitions are applied internally by the sentant — nothing
        // to forward.
        Action::Transition(_) => None,
        // Plugin calls are owned by the ensemble's local plugin set;
        // not yet wired in v0.1 (they currently no-op).
        Action::PluginCall { .. } => None,
        // Log actions go through the standard `log` crate from the
        // host (after dispatch returns).
        Action::Log { .. } => None,
    }
}

/// Surface for testing: snapshot of an ensemble's current status.
pub fn ensemble_status(reg: &EnsembleRegistry, id: &str) -> Option<EnsembleStatus> {
    reg.ensembles
        .read()
        .get(id)
        .map(|e| e.status())
}
