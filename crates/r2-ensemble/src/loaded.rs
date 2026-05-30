//! Loaded-ensemble bookkeeping types.
//!
//! [`LoadedEnsemble`] is the registry's record for one live ensemble.
//! It owns the parsed score, the constructed sentant instances, and
//! the precomputed subscription set so dispatch is O(1) per event.

use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use r2_def::{EnsembleScore, SentantDef};
use r2_engine::Sentant;

use crate::supervision::{EnsembleStatus, RestartLedger, RestartPolicy, SupervisionConfig};

/// The unique handle for a loaded ensemble. Currently the score's
/// `name` field; deduplication is enforced on load.
pub type EnsembleId = String;

/// Monotonic id assigned by the registry to each sentant instance.
/// Stable for the lifetime of the instance; not reused after stop.
pub type SentantInstanceId = u32;

/// One sentant instance inside a loaded ensemble.
///
/// The actual `Sentant` is wrapped in a [`Mutex`]: per the IPUCOD
/// determinism property, `handle_event` runs to completion without
/// I/O, so a short critical section is fine.
///
/// `parking_lot::Mutex` is used in preference to `std::sync::Mutex`
/// because it doesn't poison on panic — supervision drops and
/// rebuilds the inner box on crash, so poison would be wrong-shaped
/// for our recovery model.
pub struct SentantInstance {
    /// Globally-unique id within this registry.
    pub instance_id: SentantInstanceId,
    /// The score's definition for this sentant.
    pub def: SentantDef,
    /// FNV class hash of `def.class` (or `def.name` if `class` is None).
    /// Used for the IPUCOD identity check on reload.
    pub class_hash: u32,
    /// Event hashes this sentant subscribes to (declared by its
    /// automation transitions).
    pub subscriptions: Vec<u32>,
    /// Per-sentant supervision policy (default `Permanent`).
    pub policy: RestartPolicy,
    /// The constructed sentant. `Sync` upcast comes from `SentantFactory::build`.
    pub(crate) inner: Arc<Mutex<Box<dyn Sentant + Send + Sync>>>,
    /// Gating timestamp: while `Some(t)`, dispatch to this instance is
    /// gated until `Instant::now() >= t` (a restart is in flight).
    /// Updated under the ensemble lock.
    pub(crate) restarting_until: Arc<Mutex<Option<Instant>>>,
}

impl std::fmt::Debug for SentantInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SentantInstance")
            .field("instance_id", &self.instance_id)
            .field("name", &self.def.name)
            .field("class_hash", &format_args!("0x{:08X}", self.class_hash))
            .field("subscriptions", &self.subscriptions.len())
            .finish()
    }
}

/// One loaded ensemble.
pub struct LoadedEnsemble {
    /// Registry handle (currently equals `score.name`).
    pub id: EnsembleId,
    /// FNV hash of the canonical score (name + version + sentant defs).
    /// Used to detect identity drift on reload — same name + different
    /// hash signals an Immutable-property violation.
    pub score_hash: u32,
    /// The parsed score, validated.
    pub score: EnsembleScore,
    /// Live sentant instances in score order.
    pub sentants: Vec<SentantInstance>,
    /// Supervision config for this ensemble.
    pub supervision: SupervisionConfig,
    /// Sliding-window restart ledger; consulted on every crash.
    pub(crate) ledger: Mutex<RestartLedger>,
    /// Lifecycle status. Mutated under the registry lock.
    pub(crate) status: parking_lot::RwLock<EnsembleStatus>,
}

impl LoadedEnsemble {
    /// Current ensemble status.
    pub fn status(&self) -> EnsembleStatus {
        *self.status.read()
    }
}

impl std::fmt::Debug for LoadedEnsemble {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedEnsemble")
            .field("id", &self.id)
            .field("score_hash", &format_args!("0x{:08X}", self.score_hash))
            .field("sentants", &self.sentants.len())
            .field("status", &self.status())
            .field("supervision", &self.supervision)
            .finish()
    }
}
