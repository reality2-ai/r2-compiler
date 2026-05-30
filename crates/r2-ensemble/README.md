# r2-ensemble

OTP-style ensemble registry and sentant supervisor for R2 hives. Loads
`r2-def` ensemble scores, tracks live sentant instances, and implements
[`r2_dispatch::DispatchTarget`] so an L3 `DeliverOnly` decision flows
straight into the registered sentants. Crashes are caught, restart
strategies applied, and over-frequency failures escalate to a `Failed`
state.

This is the Rust counterpart of the BEAM `R2.Hive` GenServer from earlier
generations of Reality2 — same role (lifecycle owner + dispatch fanout),
different runtime model (run-to-completion sync handlers + `parking_lot`
mutexes vs. one process per sentant).

---

## What this crate is

```text
        ┌────────────────────────────────────────────────┐
        │              EnsembleRegistry                   │
        │                                                 │
        │   ┌─────────┐   ┌─────────────┐                 │
        │   │ load    │   │ list / info │ ← mgmt API      │
        │   │ stop    │   │ reset       │                 │
        │   └─────────┘   └─────────────┘                 │
        │                                                 │
        │   ┌──────────────────────────────────────────┐  │
        │   │   ensembles  : id ─► LoadedEnsemble       │  │
        │   │   index      : event_hash ─► [(id, inst)] │  │
        │   │   factories  : Vec<dyn SentantFactory>    │  │
        │   │   sink       : Option<dyn OutboundSink>   │  │
        │   └──────────────────────────────────────────┘  │
        │                                                 │
        │            ▲                       │            │
        │            │ dispatch              │ deliver    │
        │            │ (DispatchEnvelope)    ▼            │
        │     ─────from router──        ──to host sink─   │
        └────────────────────────────────────────────────┘
```

A `LoadedEnsemble` carries the parsed score, the constructed sentant
instances, the supervision config, and a sliding-window restart ledger.
Each `SentantInstance` owns a `parking_lot::Mutex<Box<dyn Sentant>>`
plus per-instance restart-policy and gating state.

---

## Public API

### Lifecycle

```rust
let reg = Arc::new(EnsembleRegistry::new());
reg.register_factory(Arc::new(MyFactory));
reg.set_sink(Arc::new(my_sink));

let id = reg.load(score)?;                     // default supervision
let id = reg.load_with(score, supervision)?;   // custom
let info = reg.info(&id);                      // Option<Arc<LoadedEnsemble>>
let ids = reg.list();                          // Vec<EnsembleId>
reg.stop(&id)?;
reg.reset(&id)?;                               // Failed → Healthy
```

### Dispatch

```rust
use r2_dispatch::{DispatchEnvelope, DispatchTarget};
reg.dispatch(envelope).await?;
```

The trait impl walks the event-hash index, runs each subscribed
sentant's `handle_event` inside `std::panic::catch_unwind`, applies the
emitted actions, and forwards `Action::Send` / `Action::DelayedSend`
through the host's `OutboundSink`.

### Factories

```rust
pub trait SentantFactory: Send + Sync {
    fn build(&self, def: &SentantDef) -> Result<BoxedSentant, LoadError>;
}
```

Factories run in registration order; the first that returns `Ok` wins.
Returning `LoadError::NoFactory { … }` defers to the next factory. v0.1
supports Rust-coded sentants only; the YAML interpreter (Phase 2 follow-up
crate) registers as just another factory.

### Outbound sink

```rust
#[async_trait]
pub trait OutboundSink: Send + Sync {
    async fn deliver(&self, event: OutboundEvent);
}
```

`OutboundEvent` carries the source sentant id, the original
`Target`, the FNV event-hash, the CBOR payload, the trust-group context,
the originator hive id (for `Target::Sender` resolution), and the
trigger msg-id (for reply correlation).

### Supervision

```rust
pub struct SupervisionConfig {
    pub strategy: RestartStrategy,    // OneForOne (default) | OneForAll | RestForOne
    pub max_restarts: u32,            // default 3
    pub period: Duration,             // default 60s
    pub backoff: BackoffPolicy,       // default Exponential 100ms→5s
}
```

Per-sentant policy is `RestartPolicy::Permanent | Transient | Temporary`.
Crashes record into a per-ensemble `RestartLedger`; if the live count in
the window exceeds `max_restarts`, the ensemble is marked `Failed` and
all dispatch to it returns `DispatchError::NoHandler`.

### Status

```rust
pub enum EnsembleStatus { Healthy, Degraded, Failed }
```

`Degraded` means at least one sentant is gated (restart in flight); the
gated instance returns `DispatchError::Backpressure` until rebuilt.

---

## IPUCOD properties

| Property | How this registry preserves it |
|---|---|
| **Immutable** | Score-identity FNV hash computed at load. Reload with the same name + different hash is a `LoadError::Validation`. |
| **Persistent** | Three policies expressed via `r2_def::StoragePolicy` (Volatile / Durable / DurableState). The hive (not this crate) owns the startup folder; durability == "score on disk, replayed on boot". |
| **Unique** | Monotonic `SentantInstanceId` from an `AtomicU32`. Reloading the same `(ensemble_id, sentant_name)` without stop yields `LoadError::AlreadyLoaded`. |
| **Consistent** | Each sentant has sole ownership of its `&mut self` via the `parking_lot::Mutex`; no cross-sentant aliasing. |
| **Opaque** | `Box<dyn Sentant>` exposes only the trait; sentant fields are unreachable from outside. |
| **Deterministic** | The `Sentant` trait signature forbids I/O and async — `handle_event(&mut self, &Event, &mut ActionBuf)` is pure transform of state + event into actions. |

Determinism leaks (timers, plugin calls) sit at the action boundary, not
inside the handler — preserved by the spec's scoping rules.

---

## Crash supervision

```text
                handle_event panics
                       │
              catch_unwind catches
                       │
                       ▼
            ledger.record(now); evict expired
                       │
                  exceeded?
                  /        \
               yes          no
                │            │
                ▼            ▼
           ensemble        gate the sentant for `backoff.delay_for(n)`
           Failed          tokio::spawn rebuild via factory
                                   │
                          (during gate)
                                   │
                              dispatch to
                              gated sentant
                                   │
                                   ▼
                       DispatchError::Backpressure
                                   │
                          backoff completes
                                   │
                          factory.build(def)
                                   │
                          swap Mutex contents
                                   │
                       last gated cleared → Healthy
```

`reset()` is the operator-driven escape hatch: clears the ledger,
rebuilds every sentant from its def, returns to Healthy.

---

## R2 crates this crate uses

| Crate | Role |
|---|---|
| [`r2-def`](../r2-def/) | Parses ensemble scores; the registry consumes the `EnsembleScore` type and walks `SentantEntry`/`SentantDef` |
| [`r2-engine`](../r2-engine/) | The `Sentant` trait, `Event`, `ActionBuf`, `Action` and `Target` types are all defined here |
| [`r2-dispatch`](../r2-dispatch/) | Defines the `DispatchEnvelope` and `DispatchTarget` contract this crate implements |
| [`r2-fnv`](../r2-fnv/) | FNV-1a hashing of event class strings (`Sentant::class_hash`, score-identity hash, subscription index keys) |

External dependencies: `tokio` (runtime/timer), `parking_lot` (non-poisoning
mutex), `async-trait`, `thiserror`, `log`.

---

## Examples

A full hand-written sentant fixture is at
[`tests/common/mod.rs`](tests/common/mod.rs). Integration tests covering
the happy path, panic recovery, intensity escalation, and reset live in
[`tests/load_dispatch.rs`](tests/load_dispatch.rs) and
[`tests/supervision.rs`](tests/supervision.rs).

```rust
use r2_dispatch::{DispatchEnvelope, DispatchTarget};
use r2_ensemble::{CapturingSink, EnsembleRegistry};
use r2_fnv::r2_hash;

let reg = Arc::new(EnsembleRegistry::new());
reg.register_factory(Arc::new(MyFactory));
reg.set_sink(Arc::new(CapturingSink::default()));
reg.load(score)?;

let env = DispatchEnvelope {
    originator: 0xCAFE_BABE,
    target_hive: 0,
    target_group: 0,
    event_hash: r2_hash("note.create").unwrap(),
    payload: b"{}",
    msg_id: 42,
    mcu_origin: false,
    received_at: 0,
    trust_group: None,
};
reg.dispatch(env).await?;
```

---

## License

Reality2 follows an **open-core** model
(`r2-specifications/specs/thurisaz/TH-ESG.md §8`):

- The R2 protocol suite — including this crate — is open source.
- The Mariko marketplace and vertical-market services (TH-MARKET) are
  licensed commercially and live elsewhere.

This crate is dual-licensed under either of:

- **Apache License, Version 2.0** ([`LICENSE-APACHE`](../../LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- **MIT License** ([`LICENSE-MIT`](../../LICENSE-MIT) or
  <https://opensource.org/licenses/MIT>)

at your option — the standard permissive Rust ecosystem dual license.
No copyleft obligation.

Contributions are accepted under the same dual license unless you say
otherwise, per the Apache-2.0 contribution clause.
