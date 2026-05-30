//! `LogAndDropTarget` — the default dispatch target when no sentant runtime is attached.
//!
//! Every dispatch logs at DEBUG level and returns `DispatchError::NoHandler`. This
//! preserves the pre-B1 behaviour of r2-hive (where `DeliverOnly` frames silently did
//! nothing) while surfacing each one to the log for observability. Also useful as a
//! test harness target — wraps arbitrary inspect/assert logic around dispatch calls.

use crate::{DispatchEnvelope, DispatchError, DispatchTarget};

/// Default target: log at DEBUG, return `NoHandler`. Zero-sized.
#[derive(Debug, Default, Clone, Copy)]
pub struct LogAndDropTarget;

#[async_trait::async_trait]
impl DispatchTarget for LogAndDropTarget {
    async fn dispatch(&self, envelope: DispatchEnvelope<'_>) -> Result<(), DispatchError> {
        log::debug!(
            "dispatch: event=0x{:08x} originator=0x{:08x} target_hive=0x{:08x} target_group=0x{:08x} msg_id={} payload={}B mcu_origin={} ts={} tg={:?} → LogAndDropTarget::NoHandler",
            envelope.event_hash,
            envelope.originator,
            envelope.target_hive,
            envelope.target_group,
            envelope.msg_id,
            envelope.payload.len(),
            envelope.mcu_origin,
            envelope.received_at,
            envelope.trust_group,
        );
        Err(DispatchError::NoHandler)
    }
}
