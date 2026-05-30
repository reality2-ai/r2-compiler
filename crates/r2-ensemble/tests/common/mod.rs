//! Shared test fixtures: a hand-written `Sentant` impl that records
//! every event it sees, optionally panics, and a factory that builds
//! it from a `SentantDef` when name == "echo".

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use r2_def::SentantDef;
use r2_engine::{ActionBuf, Event, Sentant, StateId, Target};
use r2_ensemble::{BoxedSentant, LoadError, SentantFactory};
use r2_fnv::r2_hash;

/// An echo sentant: every event with hash matching one of its
/// subscriptions emits one outbound `Action::Send` to `Target::Sender`
/// with the same payload, then increments `seen` and tracks state.
pub struct EchoSentant {
    pub state: StateId,
    pub class_hash: u32,
    pub seen: Arc<AtomicU32>,
    pub will_panic: Arc<AtomicBool>,
    pub subscriptions: Vec<u32>,
}

impl Sentant for EchoSentant {
    fn handle_event(&mut self, event: &Event, actions: &mut ActionBuf) {
        if self.will_panic.load(Ordering::SeqCst) {
            // One-shot: clear so restart succeeds.
            self.will_panic.store(false, Ordering::SeqCst);
            panic!("EchoSentant injected panic");
        }
        if !self.subscriptions.contains(&event.hash) {
            return;
        }
        self.seen.fetch_add(1, Ordering::SeqCst);
        actions.push(r2_engine::Action::send(
            Target::Sender,
            event.hash,
            event.payload,
        ));
    }
    fn state(&self) -> StateId {
        self.state
    }
    fn class_hash(&self) -> u32 {
        self.class_hash
    }
    fn name(&self) -> &str {
        "echo"
    }
    fn subscriptions(&self) -> &[u32] {
        &self.subscriptions
    }
}

/// Factory that builds `EchoSentant`s for definitions whose name
/// starts with `echo`. Counters/flags are shared across rebuilds so
/// supervisor restarts don't reset our test instrumentation.
pub struct EchoFactory {
    pub seen: Arc<AtomicU32>,
    pub will_panic: Arc<AtomicBool>,
    pub builds: Arc<AtomicU32>,
}

impl EchoFactory {
    pub fn new() -> Self {
        Self {
            seen: Arc::new(AtomicU32::new(0)),
            will_panic: Arc::new(AtomicBool::new(false)),
            builds: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl SentantFactory for EchoFactory {
    fn build(&self, def: &SentantDef) -> Result<BoxedSentant, LoadError> {
        if !def.name.starts_with("echo") {
            return Err(LoadError::NoFactory {
                name: def.name.clone(),
                reason: "EchoFactory only handles 'echo*' names".into(),
            });
        }
        let class = def.class.as_deref().unwrap_or(&def.name);
        let class_hash = r2_hash(class).map_err(|_| LoadError::BadEventClass(class.into()))?;
        let mut subs = Vec::new();
        for auto in &def.automations {
            for tr in &auto.transitions {
                let h = r2_hash(&tr.event)
                    .map_err(|_| LoadError::BadEventClass(tr.event.clone()))?;
                if !subs.contains(&h) {
                    subs.push(h);
                }
            }
        }
        self.builds.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(EchoSentant {
            state: 0,
            class_hash,
            seen: self.seen.clone(),
            will_panic: self.will_panic.clone(),
            subscriptions: subs,
        }))
    }
}

/// Build a minimal one-sentant ensemble score for tests.
pub fn make_score(name: &str, sentant_name: &str, events: &[&str]) -> r2_def::EnsembleScore {
    let yaml = format!(
        r#"
ensemble:
  name: {name}
  description: test ensemble
  version: "0.1.0"
  ensemble_version: "0.1"
  sentants:
    - name: {sentant_name}
      class: nz.test.{sentant_name}
      description: a test sentant
      automations:
        - name: main
          transitions:
{transitions}
"#,
        transitions = events
            .iter()
            .map(|e| format!("            - event: {e}\n              from: \"*\""))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let parsed: r2_def::EnsembleFile = serde_yaml::from_str(&yaml).expect("test fixture parses");
    parsed.ensemble
}
