//! [`EnsembleRegistry`] — top-level supervisor and lifecycle owner.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use r2_def::{EnsembleScore, SentantDef, SentantEntry};
use r2_dispatch::{DispatchEnvelope, DispatchError};
use r2_engine::{Event, EventSource};
use r2_fnv::r2_hash;

use crate::dispatch::{
    action_to_outbound, run_handler, DispatchOutcome, OutboundSink,
};
use crate::error::{LoadError, StopError};
use crate::factory::{BoxedSentant, SentantFactory};
use crate::loaded::{EnsembleId, LoadedEnsemble, SentantInstance, SentantInstanceId};
use crate::supervision::{
    EnsembleStatus, RestartLedger, RestartPolicy, RestartStrategy, SupervisionConfig,
};

/// The ensemble registry.
///
/// One per hive. Hold an `Arc<EnsembleRegistry>` and pass it where a
/// `r2_dispatch::DispatchTarget` is required — the impl on this type
/// is the dispatch path.
pub struct EnsembleRegistry {
    pub(crate) ensembles: RwLock<HashMap<EnsembleId, Arc<LoadedEnsemble>>>,
    /// Event-hash → (ensemble_id, instance_id) index. Rebuilt whenever
    /// an ensemble is loaded or stopped.
    index: RwLock<EventIndex>,
    /// Registered sentant factories, in order. First match wins.
    factories: RwLock<Vec<Arc<dyn SentantFactory>>>,
    /// Outbound sink. Optional: if absent, `Send` actions are logged
    /// and dropped.
    sink: RwLock<Option<Arc<dyn OutboundSink>>>,
    /// Monotonic instance id allocator.
    next_instance_id: AtomicU32,
}

#[derive(Default)]
struct EventIndex {
    by_hash: HashMap<u32, Vec<(EnsembleId, SentantInstanceId)>>,
}

impl EventIndex {
    fn add(&mut self, hash: u32, ensemble: EnsembleId, instance: SentantInstanceId) {
        self.by_hash
            .entry(hash)
            .or_default()
            .push((ensemble, instance));
    }

    fn remove_ensemble(&mut self, ensemble: &EnsembleId) {
        for entries in self.by_hash.values_mut() {
            entries.retain(|(eid, _)| eid != ensemble);
        }
        self.by_hash.retain(|_, v| !v.is_empty());
    }

    fn lookup(&self, hash: u32) -> Vec<(EnsembleId, SentantInstanceId)> {
        self.by_hash.get(&hash).cloned().unwrap_or_default()
    }
}

impl Default for EnsembleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl EnsembleRegistry {
    /// Create an empty registry with no factories registered.
    pub fn new() -> Self {
        Self {
            ensembles: RwLock::new(HashMap::new()),
            index: RwLock::new(EventIndex::default()),
            factories: RwLock::new(Vec::new()),
            sink: RwLock::new(None),
            next_instance_id: AtomicU32::new(1),
        }
    }

    /// Register a sentant factory. Order matters: the registry asks
    /// each factory in registration order on every load until one
    /// returns `Ok`.
    pub fn register_factory(&self, factory: Arc<dyn SentantFactory>) {
        self.factories.write().push(factory);
    }

    /// Install the outbound sink. Replaces any previously installed
    /// sink.
    pub fn set_sink(&self, sink: Arc<dyn OutboundSink>) {
        *self.sink.write() = Some(sink);
    }

    /// Load an ensemble score with default supervision.
    pub fn load(&self, score: EnsembleScore) -> Result<EnsembleId, LoadError> {
        self.load_with(score, SupervisionConfig::default())
    }

    /// Load an ensemble score with a specific supervision config.
    pub fn load_with(
        &self,
        score: EnsembleScore,
        supervision: SupervisionConfig,
    ) -> Result<EnsembleId, LoadError> {
        score
            .validate()
            .map_err(|e| LoadError::Validation(format!("{e}")))?;

        let id = score.name.clone();
        if self.ensembles.read().contains_key(&id) {
            return Err(LoadError::AlreadyLoaded(id));
        }

        let score_hash = compute_score_hash(&score)?;

        let mut sentants = Vec::with_capacity(score.sentants.len());
        for entry in &score.sentants {
            let def = match entry {
                SentantEntry::Inline(def) => (**def).clone(),
                SentantEntry::External { include } => {
                    return Err(LoadError::ExternalIncludeUnsupported(include.clone()));
                }
            };
            let inst = self.build_instance(def)?;
            sentants.push(inst);
        }

        let ensemble = Arc::new(LoadedEnsemble {
            id: id.clone(),
            score_hash,
            score,
            sentants,
            supervision,
            ledger: Mutex::new(RestartLedger::new(&supervision)),
            status: parking_lot::RwLock::new(EnsembleStatus::Healthy),
        });

        // Update event index.
        {
            let mut idx = self.index.write();
            for inst in &ensemble.sentants {
                for &h in &inst.subscriptions {
                    idx.add(h, id.clone(), inst.instance_id);
                }
            }
        }

        self.ensembles.write().insert(id.clone(), ensemble.clone());

        // Call init() on each sentant after registration.
        for inst in &ensemble.sentants {
            let mut buf = r2_engine::ActionBuf::new();
            // init can panic too; we don't supervise initial init in
            // v0.1 — a sentant that panics on init fails the load.
            let inner = Arc::clone(&inst.inner);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                inner.lock().init(&mut buf);
            }));
            if result.is_err() {
                // Roll back: remove from index, drop ensembles entry.
                self.index.write().remove_ensemble(&id);
                self.ensembles.write().remove(&id);
                return Err(LoadError::NoFactory {
                    name: inst.def.name.clone(),
                    reason: "panic during Sentant::init".to_string(),
                });
            }
            // Forward any actions emitted during init via the sink.
            if let Some(sink) = self.sink.read().as_ref().cloned() {
                let ensemble_id = id.clone();
                let instance_id = inst.instance_id;
                let actions: Vec<_> = buf.iter().cloned().collect();
                tokio::spawn(async move {
                    for a in actions {
                        if let Some(out) =
                            action_to_outbound(&ensemble_id, instance_id, None, None, 0, a)
                        {
                            sink.deliver(out).await;
                        }
                    }
                });
            }
        }

        log::info!(
            "loaded ensemble '{}' (score_hash 0x{:08X}, {} sentants)",
            id,
            score_hash,
            ensemble.sentants.len()
        );

        Ok(id)
    }

    /// Stop a loaded ensemble.
    pub fn stop(&self, id: &str) -> Result<(), StopError> {
        let removed = self.ensembles.write().remove(id);
        if removed.is_none() {
            return Err(StopError::NotLoaded(id.to_string()));
        }
        self.index.write().remove_ensemble(&id.to_string());
        log::info!("stopped ensemble '{}'", id);
        Ok(())
    }

    /// List loaded ensemble ids in arbitrary order.
    pub fn list(&self) -> Vec<EnsembleId> {
        self.ensembles.read().keys().cloned().collect()
    }

    /// Look up a loaded ensemble.
    pub fn info(&self, id: &str) -> Option<Arc<LoadedEnsemble>> {
        self.ensembles.read().get(id).cloned()
    }

    /// `true` iff the ensemble is loaded and in `Failed` state.
    pub fn is_ensemble_failed(&self, id: &str) -> bool {
        matches!(
            self.ensembles.read().get(id).map(|e| e.status()),
            Some(EnsembleStatus::Failed)
        )
    }

    /// Subscribers `(ensemble_id, instance_id)` for a given event hash.
    pub(crate) fn subscribers_for(
        &self,
        event_hash: u32,
    ) -> Vec<(EnsembleId, SentantInstanceId)> {
        self.index.read().lookup(event_hash)
    }

    /// Dispatch a single envelope to one specific instance, applying
    /// supervision on crash.
    pub(crate) async fn dispatch_to(
        &self,
        ensemble_id: &str,
        instance_id: SentantInstanceId,
        envelope: &DispatchEnvelope<'_>,
    ) -> Result<(), DispatchError> {
        let ensemble = match self.ensembles.read().get(ensemble_id).cloned() {
            Some(e) => e,
            None => return Err(DispatchError::NoHandler),
        };
        if ensemble.status() == EnsembleStatus::Failed {
            return Err(DispatchError::NoHandler);
        }
        let inst = match ensemble.sentants.iter().find(|i| i.instance_id == instance_id) {
            Some(i) => i,
            None => return Err(DispatchError::NoHandler),
        };

        let event = Event {
            hash: envelope.event_hash,
            payload: envelope.payload,
            source: EventSource::Remote(envelope.originator),
            msg_id: envelope.msg_id as u16,
        };

        match run_handler(inst, event) {
            DispatchOutcome::Ok(actions) => {
                self.apply_actions(
                    ensemble_id,
                    instance_id,
                    envelope.trust_group,
                    Some(envelope.originator),
                    envelope.msg_id,
                    actions,
                )
                .await;
                Ok(())
            }
            DispatchOutcome::Gated => Err(DispatchError::Backpressure),
            DispatchOutcome::Crashed => {
                self.handle_crash(&ensemble, instance_id);
                Err(DispatchError::Rejected)
            }
        }
    }

    async fn apply_actions(
        &self,
        ensemble_id: &str,
        instance_id: SentantInstanceId,
        trust_group: Option<[u8; 8]>,
        originator: Option<u32>,
        trigger_msg_id: u32,
        actions: Vec<r2_engine::Action>,
    ) {
        let sink = self.sink.read().clone();
        for action in actions {
            // Log actions are emitted directly here.
            if let r2_engine::Action::Log { level, message } = &action {
                let msg = String::from_utf8_lossy(message.as_slice());
                match level {
                    0 => log::error!("[{ensemble_id}#{instance_id}] {}", msg),
                    1 => log::warn!("[{ensemble_id}#{instance_id}] {}", msg),
                    2 => log::info!("[{ensemble_id}#{instance_id}] {}", msg),
                    _ => log::debug!("[{ensemble_id}#{instance_id}] {}", msg),
                }
                continue;
            }
            if let Some(out) = action_to_outbound(
                &ensemble_id.to_string(),
                instance_id,
                trust_group,
                originator,
                trigger_msg_id,
                action,
            ) {
                if let Some(s) = sink.as_ref() {
                    s.deliver(out).await;
                } else {
                    log::debug!(
                        "no OutboundSink configured; dropping outbound event 0x{:08X}",
                        out.event_hash
                    );
                }
            }
        }
    }

    /// Apply the ensemble's supervision strategy in response to a
    /// crash. Marks the ensemble `Degraded`, gates the affected
    /// sentants, and schedules tokio tasks to rebuild them after
    /// backoff. Escalates to `Failed` if the intensity cap is
    /// exceeded.
    fn handle_crash(&self, ensemble: &Arc<LoadedEnsemble>, crashed_id: SentantInstanceId) {
        let now = Instant::now();
        let exceeded = {
            let mut ledger = ensemble.ledger.lock();
            let _ = ledger.record(now);
            ledger.would_exceed(now)
        };

        if exceeded {
            *ensemble.status.write() = EnsembleStatus::Failed;
            log::error!(
                "ensemble '{}' exceeded restart-intensity cap (max {} in {:?}); marking Failed",
                ensemble.id,
                ensemble.supervision.max_restarts,
                ensemble.supervision.period,
            );
            return;
        }

        // Compute restart count for backoff before marking Degraded.
        let restart_n = {
            let mut ledger = ensemble.ledger.lock();
            ledger.live_count(now).saturating_sub(1)
        };
        let delay = ensemble.supervision.backoff.delay_for(restart_n);

        *ensemble.status.write() = EnsembleStatus::Degraded;

        // Pick the sentants to restart according to strategy.
        let to_restart: Vec<SentantInstanceId> = match ensemble.supervision.strategy {
            RestartStrategy::OneForOne => vec![crashed_id],
            RestartStrategy::OneForAll => ensemble
                .sentants
                .iter()
                .filter(|i| i.policy != RestartPolicy::Temporary)
                .map(|i| i.instance_id)
                .collect(),
            RestartStrategy::RestForOne => {
                let pos = ensemble
                    .sentants
                    .iter()
                    .position(|i| i.instance_id == crashed_id)
                    .unwrap_or(0);
                ensemble.sentants[pos..]
                    .iter()
                    .filter(|i| i.policy != RestartPolicy::Temporary)
                    .map(|i| i.instance_id)
                    .collect()
            }
        };

        // Filter by per-sentant policy.
        let filtered: Vec<_> = to_restart
            .into_iter()
            .filter(|id| {
                let inst = ensemble.sentants.iter().find(|i| i.instance_id == *id);
                match inst {
                    Some(i) => i.policy != RestartPolicy::Temporary,
                    None => false,
                }
            })
            .collect();

        // Gate each affected instance.
        for id in &filtered {
            if let Some(inst) = ensemble.sentants.iter().find(|i| i.instance_id == *id) {
                *inst.restarting_until.lock() = Some(now + delay);
            }
        }

        // Schedule restart on a tokio task. We need a clone of the
        // factories list and the ensemble arc.
        let factories = self.factories.read().clone();
        let ensemble_arc = Arc::clone(ensemble);
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            for id in filtered {
                let inst = match ensemble_arc.sentants.iter().find(|i| i.instance_id == id) {
                    Some(i) => i,
                    None => continue,
                };

                let new_box = match try_build(&factories, &inst.def) {
                    Ok(b) => b,
                    Err(e) => {
                        log::error!(
                            "could not rebuild sentant '{}' after crash: {}",
                            inst.def.name,
                            e
                        );
                        *ensemble_arc.status.write() = EnsembleStatus::Failed;
                        return;
                    }
                };
                // Swap inner and clear gate.
                {
                    let mut guard = inst.inner.lock();
                    *guard = new_box;
                    let mut buf = r2_engine::ActionBuf::new();
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        guard.init(&mut buf);
                    }));
                }
                *inst.restarting_until.lock() = None;
                log::warn!(
                    "ensemble '{}' sentant '{}' restarted after crash",
                    ensemble_arc.id,
                    inst.def.name
                );
            }
            // Restore Healthy if no sentants are gated.
            let any_gated = ensemble_arc
                .sentants
                .iter()
                .any(|i| i.restarting_until.lock().is_some());
            if !any_gated && ensemble_arc.status() != EnsembleStatus::Failed {
                *ensemble_arc.status.write() = EnsembleStatus::Healthy;
            }
        });
    }

    /// Build one sentant instance via the registered factories.
    fn build_instance(&self, def: SentantDef) -> Result<SentantInstance, LoadError> {
        let factories = self.factories.read().clone();
        let inner = try_build(&factories, &def)?;
        let class_string: &str = def.class.as_deref().unwrap_or(&def.name);
        let class_hash = r2_hash(class_string)
            .map_err(|_| LoadError::BadEventClass(class_string.to_string()))?;
        let subscriptions = collect_subscriptions(&def)?;
        let policy = RestartPolicy::Permanent;
        let instance_id = self.next_instance_id.fetch_add(1, Ordering::Relaxed);

        Ok(SentantInstance {
            instance_id,
            def,
            class_hash,
            subscriptions,
            policy,
            inner: Arc::new(Mutex::new(inner)),
            restarting_until: Arc::new(Mutex::new(None)),
        })
    }

    /// Reset a `Failed` ensemble back to `Healthy` (operator action).
    /// Clears the restart ledger and reinitialises every sentant.
    pub fn reset(&self, id: &str) -> Result<(), StopError> {
        let ensemble = match self.ensembles.read().get(id).cloned() {
            Some(e) => e,
            None => return Err(StopError::NotLoaded(id.to_string())),
        };
        ensemble.ledger.lock().clear();
        let factories = self.factories.read().clone();
        for inst in &ensemble.sentants {
            if let Ok(new_box) = try_build(&factories, &inst.def) {
                let mut guard = inst.inner.lock();
                *guard = new_box;
                let mut buf = r2_engine::ActionBuf::new();
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    guard.init(&mut buf);
                }));
            }
            *inst.restarting_until.lock() = None;
        }
        *ensemble.status.write() = EnsembleStatus::Healthy;
        Ok(())
    }

    /// Returns when every loaded ensemble has no gated instances. For
    /// tests; production code should not block on this.
    pub async fn await_quiescent(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            let any_gated = {
                let ensembles = self.ensembles.read();
                ensembles.values().any(|e| {
                    e.sentants
                        .iter()
                        .any(|i| i.restarting_until.lock().is_some())
                })
            };
            if !any_gated || Instant::now() >= deadline {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

/// Walk factories in order, return the first successful build.
fn try_build(
    factories: &[Arc<dyn SentantFactory>],
    def: &SentantDef,
) -> Result<BoxedSentant, LoadError> {
    let mut last_reason = String::from("no factories registered");
    for f in factories {
        match f.build(def) {
            Ok(b) => return Ok(b),
            Err(LoadError::NoFactory { reason, .. }) => {
                last_reason = reason;
                continue;
            }
            Err(other) => return Err(other),
        }
    }
    Err(LoadError::NoFactory {
        name: def.name.clone(),
        reason: last_reason,
    })
}

/// Walk a sentant definition's automations and collect FNV hashes for
/// every transition's event class.
fn collect_subscriptions(def: &SentantDef) -> Result<Vec<u32>, LoadError> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for auto in &def.automations {
        for tr in &auto.transitions {
            let h = r2_hash(&tr.event)
                .map_err(|_| LoadError::BadEventClass(tr.event.clone()))?;
            if seen.insert(h) {
                out.push(h);
            }
        }
    }
    Ok(out)
}

/// Compute a stable identity hash for a score: FNV over name, version,
/// and the canonical YAML representation of each sentant entry. Used
/// for the IPUCOD Immutable identity check.
fn compute_score_hash(score: &EnsembleScore) -> Result<u32, LoadError> {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = write!(s, "{}|{}|{}|", score.name, score.version, score.ensemble_version);
    for entry in &score.sentants {
        let _ = match entry {
            SentantEntry::Inline(d) => write!(s, "{};", d.name),
            SentantEntry::External { include } => write!(s, "ext:{};", include),
        };
    }
    r2_hash(&s).map_err(|_| LoadError::BadEventClass(s))
}

