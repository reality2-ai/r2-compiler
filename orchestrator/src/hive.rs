//! Engine-thread setup + the mpsc bridge between async (axum) and
//! synchronous (`r2-engine::EventBus`).
//!
//! `r2-engine`'s `EventBus` is `!Send`-friendly per the r2-forge
//! pattern — it runs on a dedicated OS thread. The axum WS handler
//! talks to it via two channels:
//!
//! - **inbound** (`mpsc<QueuedEvent>`): WS → engine
//! - **outbound** (`broadcast<QueuedEvent>`): engine → all WS clients
//!
//! ## Phase 1.7a scope
//!
//! - Spawns the engine thread.
//! - Registers the [`BuilderSentant`] stub.
//! - Ticks the bus + polls plugins on a fixed cadence (10 ms).
//! - Drains the bus's outbound queue and forwards to the broadcast channel.
//!
//! Phase 1.7+ adds the rest of the sentant + plugin set; this scaffold
//! doesn't change.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use r2_engine::EventBus;
use r2_engine::queue::QueuedEvent;
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};

use crate::plugins::ClaudeCodePlugin;
use crate::sentants::{AuthorSentant, BuilderSentant};

/// Bridge handle exposed to the axum layer.
#[derive(Clone)]
pub struct EngineHandle {
    /// WS → engine channel sender.
    pub inbound_tx: mpsc::Sender<QueuedEvent>,
    /// Engine → WS broadcast.
    pub outbound_tx: broadcast::Sender<QueuedEvent>,
}

impl EngineHandle {
    /// Subscribe a new WS client to outbound events.
    pub fn subscribe_outbound(&self) -> broadcast::Receiver<QueuedEvent> {
        self.outbound_tx.subscribe()
    }
}

/// Spawn the engine thread and return a handle for the axum layer.
pub fn spawn() -> EngineHandle {
    // Bounded inbound queue — backpressure if the engine falls behind.
    let (inbound_tx, mut inbound_rx) = mpsc::channel::<QueuedEvent>(256);
    // Outbound broadcast — every connected WS client subscribes.
    let (outbound_tx, _outbound_rx_unused) = broadcast::channel::<QueuedEvent>(256);
    let outbound_tx_thread = outbound_tx.clone();

    std::thread::spawn(move || {
        let mut bus = EventBus::new();

        // Register plugins FIRST — their IDs flow into the sentants that
        // dispatch to them. Two claude-code plugin instances: one for
        // the build flow (emits r2.compiler.build.*) and one for the
        // author / chat flow (emits r2.compiler.author.*). Same plugin
        // impl; different event-name configuration at construction time.
        let build_pid = bus.register_plugin(Box::new(ClaudeCodePlugin::new(0)));
        info!("engine: registered claude-code plugin for build (id={build_pid})");

        // Author brief is delivered through a shared slot rather than
        // the bus's PluginCall data (which caps at 256B). Both sides
        // hold a clone of the same Arc<Mutex<Option<String>>>.
        let author_brief: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let author_pid = bus.register_plugin(Box::new(
            ClaudeCodePlugin::new_author(0, author_brief.clone())
        ));
        info!("engine: registered claude-code plugin for author (id={author_pid})");

        // Register sentants.
        let build_sid = bus.register_sentant(Box::new(BuilderSentant::new(build_pid)));
        info!("engine: registered Builder sentant (id={build_sid})");
        let author_sid = bus.register_sentant(Box::new(
            AuthorSentant::new(author_pid, author_brief)
        ));
        info!("engine: registered Author sentant (id={author_sid})");

        bus.init_all();
        info!("engine: bus initialised");

        // Engine loop. Two tasks per tick:
        // 1. Drain inbound channel into the bus.
        // 2. bus.tick() + bus.poll_plugins() + drain outbound, broadcast.
        let tick_ms: u32 = 10;
        loop {
            // Drain whatever the WS layer has queued up (non-blocking).
            let mut drained = 0;
            while let Ok(event) = inbound_rx.try_recv() {
                if !bus.enqueue(event) {
                    warn!("engine: inbound queue full — dropping event");
                }
                drained += 1;
                if drained >= 32 { break; } // bounded work per tick
            }

            bus.tick();
            bus.poll_plugins();
            bus.advance_time(tick_ms);

            for ev in bus.drain_outbound() {
                let _ = outbound_tx_thread.send(ev);
            }

            std::thread::sleep(Duration::from_millis(tick_ms as u64));
        }
    });

    EngineHandle {
        inbound_tx,
        outbound_tx,
    }
}
