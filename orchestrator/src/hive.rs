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

use std::path::PathBuf;

use crate::plugins::{
    ClaudeCodePlugin, FlasherPlugin, FlasherSlot, UsbSnapshot, UsbWatcherPlugin,
};
use crate::sentants::{AuthorSentant, BuilderSentant, DeploySentant, RosterCtx, RosterSentant};

/// Bridge handle exposed to the axum layer.
#[derive(Clone)]
pub struct EngineHandle {
    /// WS → engine channel sender.
    pub inbound_tx: mpsc::Sender<QueuedEvent>,
    /// Engine → WS broadcast.
    pub outbound_tx: broadcast::Sender<QueuedEvent>,
    /// Shared roster context — the active apiary's directory path.
    /// Mutated by the WS handler (or future apiary-open flow) when the
    /// orchestrator switches apiaries. None when no apiary is open.
    pub roster_ctx: RosterCtx,
    /// Shared snapshot of currently-attached USB serial devices —
    /// updated by the usb-watcher plugin's background thread, read by
    /// the WS handler so late-connecting clients see current state.
    pub usb_snapshot: UsbSnapshot,
}

impl EngineHandle {
    /// Subscribe a new WS client to outbound events.
    pub fn subscribe_outbound(&self) -> broadcast::Receiver<QueuedEvent> {
        self.outbound_tx.subscribe()
    }
}

/// Spawn the engine thread and return a handle for the axum layer.
///
/// `apiary_path` is the active apiary's directory, if any — primed
/// from `--apiary <name>` at startup. The roster sentant uses it to
/// read/write `apiaries/<name>/devices/roster.toml`. Runtime apiary
/// open/close (future) mutates the same Arc<Mutex<>>.
pub fn spawn(apiary_path: Option<PathBuf>, repo_root: PathBuf) -> EngineHandle {
    // Bounded inbound queue — backpressure if the engine falls behind.
    let (inbound_tx, mut inbound_rx) = mpsc::channel::<QueuedEvent>(256);
    // Outbound broadcast — every connected WS client subscribes.
    let (outbound_tx, _outbound_rx_unused) = broadcast::channel::<QueuedEvent>(256);
    let outbound_tx_thread = outbound_tx.clone();

    // Roster context — shared between hive.rs (this fn) and the
    // RosterSentant inside the engine thread. The outer handle also
    // holds a clone so the WS handler can swap apiaries later.
    let roster_ctx: RosterCtx = Arc::new(Mutex::new(apiary_path));
    let roster_ctx_engine = roster_ctx.clone();
    let roster_ctx_deploy = roster_ctx.clone();

    // Flasher params slot — Deploy sentant fills it before firing
    // the flasher's CMD_START.
    let flasher_slot: FlasherSlot = Arc::new(Mutex::new(None));
    let flasher_slot_engine = flasher_slot.clone();

    // USB snapshot — usb-watcher updates; WS handler reads for replay.
    let usb_snapshot: UsbSnapshot = Arc::new(Mutex::new(Vec::new()));
    let usb_snapshot_engine = usb_snapshot.clone();
    let catalogue_root = repo_root.join("catalogue");

    std::thread::spawn(move || {
        let mut bus = EventBus::new();

        // Register plugins FIRST — their IDs flow into the sentants that
        // dispatch to them. Two claude-code plugin instances: one for
        // the build flow (emits r2.composer.build.*) and one for the
        // author / chat flow (emits r2.composer.author.*). Same plugin
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

        // Flasher (F2) — esptool subprocess driver, fed via flasher_slot.
        let flasher_pid = bus.register_plugin(Box::new(
            FlasherPlugin::new(0, flasher_slot_engine.clone())
        ));
        info!("engine: registered flasher plugin (id={flasher_pid})");

        // USB watcher (F2b) — polls /sys/class/tty/ every 1.5s on Linux.
        let usb_pid = bus.register_plugin(Box::new(
            UsbWatcherPlugin::new(0, catalogue_root.clone(), usb_snapshot_engine)
        ));
        info!("engine: registered usb-watcher plugin (id={usb_pid})");

        // Register sentants.
        let build_sid = bus.register_sentant(Box::new(BuilderSentant::new(build_pid)));
        info!("engine: registered Builder sentant (id={build_sid})");
        let author_sid = bus.register_sentant(Box::new(
            AuthorSentant::new(author_pid, author_brief)
        ));
        info!("engine: registered Author sentant (id={author_sid})");
        let roster_sid = bus.register_sentant(Box::new(
            RosterSentant::new(roster_ctx_engine)
        ));
        info!("engine: registered Roster sentant (id={roster_sid})");
        let deploy_sid = bus.register_sentant(Box::new(
            DeploySentant::new(flasher_pid, flasher_slot_engine, repo_root.clone(), roster_ctx_deploy)
        ));
        info!("engine: registered Deploy sentant (id={deploy_sid})");

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
        roster_ctx,
        usb_snapshot,
    }
}
