//! `DispatchEnvelope` — structured view of an R2-WIRE event bound for local delivery.
//!
//! After the route engine decides a frame is for local delivery (`ForwardAction::DeliverOnly`
//! per R2-ROUTE §4), the hive constructs a `DispatchEnvelope` from the parsed header +
//! route stack + payload and hands it to its `DispatchTarget`. The envelope carries
//! everything a sentant runtime needs to decide what to do with the event, with nothing
//! from the transport layer that shouldn't concern a handler (a sentant MUST NOT care
//! whether the frame arrived via BLE, WiFi, or LoRa — that is a routing concern, not a
//! dispatch concern).
//!
//! Per R2-RUNTIME §2.4 (normative), the envelope fields defined here are the minimum
//! REQUIRED contents of any dispatch envelope. Implementations MAY extend with
//! additional fields but MUST preserve the required ones across the dispatch call.

/// Envelope describing a single event for local dispatch.
///
/// Lifetime `'a` binds the payload slice — dispatch MUST NOT retain the envelope past
/// the call; the underlying buffer may be reclaimed immediately afterwards. Callers
/// that need asynchronous processing MUST copy the relevant fields into an owned form
/// before yielding.
#[derive(Debug, Clone)]
pub struct DispatchEnvelope<'a> {
    /// Originating hive (route_stack[0] in the wire frame, zero-extended to 32 bits
    /// if the frame was compact). See R2-WIRE §4.2.3, §4.3.3.
    pub originator: u32,

    /// Target hive ID from the wire header, or `0` for broadcast (R2-WIRE §6.3).
    pub target_hive: u32,

    /// Target trust-group hash from the wire header (upper 32 bits of the SHA-256 of
    /// TG_PK), or `0` if no group target is present.
    pub target_group: u32,

    /// FNV-1a 32-bit hash of the event name (R2-FNV).
    pub event_hash: u32,

    /// CBOR-encoded event payload bytes (R2-CBOR). MAY be empty.
    pub payload: &'a [u8],

    /// Wire message id used for correlation and deduplication (R2-WIRE §8.2).
    pub msg_id: u32,

    /// MCU-origin flag from the R2-WIRE header. When `true`, the originator is a
    /// constrained MCU and the sentant runtime SHOULD treat errors and backpressure
    /// leniently — MCUs have no retry buffer.
    pub mcu_origin: bool,

    /// Unix timestamp in seconds at which the hive received the frame. Used for
    /// eventual-consistency ordering in state-carrying ensembles (see the Notekeeper
    /// LWW pattern).
    pub received_at: u32,

    /// First 8 bytes of SHA-256(TG_PK) identifying the trust group this event is
    /// scoped to, if the receiving hive is a member of exactly one matching trust
    /// group. `None` if the event could not be attributed to a known group (e.g.
    /// observed relay traffic for a trust group this hive does not belong to).
    pub trust_group: Option<[u8; 8]>,
}
