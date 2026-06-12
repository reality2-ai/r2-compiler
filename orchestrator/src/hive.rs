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

use crate::composer::{ClaudeCodePlugin, FlasherPlugin, FlasherSlot, UsbSnapshot, UsbWatcherPlugin};
use crate::substrate::{
    BeaconObserverPlugin, BeaconSnapshot, KeyholderPlugin, KeyholderSlot,
    OtaPushPlugin, OtaPushSlot, ProvisionHandshakePlugin, ProvisionHandshakeSlot,
    ProvisionPlugin, ProvisionSlot,
};
use crate::sentants::test_coordinator::TestCoordinator;
use crate::sentants::{
    AuthorSentant, BuilderSentant, DeploySentant, ProvisionSentant, RosterCtx, RosterSentant,
};

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
    /// Shared snapshot of currently-observed R2-BEACONs — updated by
    /// the beacon-observer substrate. WS replay uses this so a late-
    /// connecting client sees the current set without waiting for the
    /// next scan window.
    pub beacon_snapshot: BeaconSnapshot,
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
    let roster_ctx_keyholder = roster_ctx.clone();
    let roster_ctx_provision = roster_ctx.clone();
    let roster_ctx_provision_sentant = roster_ctx.clone();
    let roster_ctx_handshake = roster_ctx.clone();

    // Flasher params slot — Deploy sentant fills it before firing
    // the flasher's CMD_START.
    let flasher_slot: FlasherSlot = Arc::new(Mutex::new(None));
    let flasher_slot_engine = flasher_slot.clone();

    // OTA push params slot (F5) — Deploy sentant fills it before firing
    // the ota_push substrate's CMD_START, once per device in a batch.
    let ota_push_slot: OtaPushSlot = Arc::new(Mutex::new(None));
    let ota_push_slot_engine = ota_push_slot.clone();

    // F3 side-slots for keyholder + provision plugins. Filled by the
    // ProvisionSentant before each PluginCall.
    let keyholder_slot: KeyholderSlot = Arc::new(Mutex::new(None));
    let keyholder_slot_engine = keyholder_slot.clone();
    let provision_slot: ProvisionSlot = Arc::new(Mutex::new(None));
    let provision_slot_engine = provision_slot.clone();
    // F4b side-slot for the provision-handshake substrate. Sentant
    // fills it before dispatching CMD_START.
    let handshake_slot: ProvisionHandshakeSlot = Arc::new(Mutex::new(None));
    let handshake_slot_engine = handshake_slot.clone();
    let config_root = KeyholderPlugin::default_config_root();

    // USB snapshot — usb-watcher updates; WS handler reads for replay.
    let usb_snapshot: UsbSnapshot = Arc::new(Mutex::new(Vec::new()));
    let usb_snapshot_engine = usb_snapshot.clone();
    // Beacon snapshot — beacon-observer updates; same WS replay role.
    let beacon_snapshot: BeaconSnapshot = Arc::new(Mutex::new(Vec::new()));
    let beacon_snapshot_engine = beacon_snapshot.clone();
    let catalogue_root = repo_root.join("catalogue");

    std::thread::spawn(move || {
        let mut bus = EventBus::new();

        // Register plugins FIRST — their IDs flow into the sentants that
        // dispatch to them. Two claude-code plugin instances: one for
        // the build flow (emits r2.composer.build.*) and one for the
        // author / chat flow (emits r2.composer.author.*). Same plugin
        // impl; different event-name configuration at construction time.
        let build_pid = bus.register_plugin(Box::new(ClaudeCodePlugin::new(0)));
        info!("engine: registered composer/claude-code (build) (id={build_pid})");

        // Author brief is delivered through a shared slot rather than
        // the bus's PluginCall data (which caps at 256B). Both sides
        // hold a clone of the same Arc<Mutex<Option<String>>>.
        let author_brief: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let author_pid = bus.register_plugin(Box::new(
            ClaudeCodePlugin::new_author(0, author_brief.clone())
        ));
        info!("engine: registered composer/claude-code (author) (id={author_pid})");

        // Flasher (F2) — esptool subprocess driver, fed via flasher_slot.
        let flasher_pid = bus.register_plugin(Box::new(
            FlasherPlugin::new(0, flasher_slot_engine.clone())
        ));
        info!("engine: registered composer/flasher (id={flasher_pid})");

        // USB watcher (F2b) — polls /sys/class/tty/ every 1.5s on Linux.
        let usb_pid = bus.register_plugin(Box::new(
            UsbWatcherPlugin::new(0, catalogue_root.clone(), usb_snapshot_engine)
        ));
        info!("engine: registered composer/usb-watcher (id={usb_pid})");

        // OTA push (F5) — wire-v1 TCP firmware push, fed via ota_push_slot.
        let ota_push_pid = bus.register_plugin(Box::new(
            OtaPushPlugin::new(0, ota_push_slot_engine.clone())
        ));
        info!("engine: registered substrate/ota-push (id={ota_push_pid})");

        // Keyholder (F3) — Ed25519 signer for DeviceCertificate minting.
        let keyholder_pid = bus.register_plugin(Box::new(KeyholderPlugin::new(
            0, roster_ctx_keyholder, keyholder_slot_engine.clone(), config_root.clone(),
        )));
        info!("engine: registered substrate/keyholder (id={keyholder_pid})");

        // Provision (F3) — WiFi credential store + #wifi_offer composer.
        let provision_pid = bus.register_plugin(Box::new(ProvisionPlugin::new(
            0, roster_ctx_provision, provision_slot_engine.clone(), config_root,
        )));
        info!("engine: registered substrate/provision (id={provision_pid})");

        // Beacon observer (F4) — BLE scan + R2-BEACON parser. Linux-
        // primary; macOS works for development. If no BLE adapter is
        // available the plugin logs a warning and stays inert — the
        // hive does NOT fail to boot.
        let beacon_pid = bus.register_plugin(Box::new(
            BeaconObserverPlugin::new(0, beacon_snapshot_engine)
        ));
        info!("engine: registered substrate/beacon-observer (id={beacon_pid})");

        // Provision handshake (F4b) — L2CAP CoC + R2-PROVISION join
        // exchange. Linux-only (bluer). Mints real R2-TRUST
        // DeviceCertificates via TrustGroup::process_join_request.
        let handshake_pid = bus.register_plugin(Box::new(ProvisionHandshakePlugin::new(
            0, roster_ctx_handshake, handshake_slot_engine.clone(),
            ProvisionHandshakePlugin::default_config_root(),
        )));
        info!("engine: registered substrate/provision-handshake (id={handshake_pid})");

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
            DeploySentant::new(
                flasher_pid, flasher_slot_engine,
                ota_push_pid, ota_push_slot_engine,
                repo_root.clone(), roster_ctx_deploy,
            )
        ));
        info!("engine: registered Deploy sentant (id={deploy_sid})");

        // Provision sentant (F4b) — orchestrates the
        // beacon_observed → handshake → identity_observed → enrolled
        // chain. The pre-F4b signature (keyholder+provision side-slots)
        // is gone; the L2CAP handshake substrate mints + delivers the
        // R2-TRUST DeviceCertificate end-to-end.
        let _ = (keyholder_pid, keyholder_slot_engine,
                 provision_pid, provision_slot_engine); // F4c will revive for WiFi offer
        let provision_sid = bus.register_sentant(Box::new(ProvisionSentant::new(
            handshake_pid, handshake_slot_engine,
            roster_ctx_provision_sentant,
        )));
        info!("engine: registered Provision sentant (id={provision_sid})");

        // TestCoordinator (Phase 3 D5) — transient-networking test adjudicator.
        // Self-contained (its own ledger); subscribes to r2.tn.inject/report/assert
        // (fnv-routed) so it's drivable now via the /r2 JSON bridge, ahead of the
        // raw /r2/wire frame channel.
        let tc_sid = bus.register_sentant(Box::new(TestCoordinator::new()));
        info!("engine: registered TestCoordinator sentant (id={tc_sid})");

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
        beacon_snapshot,
    }
}
