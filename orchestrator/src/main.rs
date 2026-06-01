//! r2-composer orchestrator — workstation-side R2 hive.
//!
//! Phase 1.7a: real `r2-engine` EventBus running on a dedicated OS
//! thread; mpsc/broadcast channels bridge the async axum WS handler
//! to/from the engine; the [`Builder`](sentants::BuilderSentant)
//! sentant responds to `r2.composer.build.start` with synthetic
//! progress events.
//!
//! Phase 1.7b wires in the `claude-code` plugin (subprocess driver
//! for `claude -p '<brief>' --output-format=stream-json`) so the
//! progress events come from a real build cycle. Phase 1.7+ adds
//! Author / Deploy / Sync / Tg / Catalogue / Apiary sentants + their
//! plugins per SPEC-R2-COMPOSER §3.

mod apiary;
mod bridge;
mod composer;   // r2-composer-specific authoring scaffolding (subprocess wrappers, sysfs watchers)
mod hive;
mod roster;
mod sentants;
mod substrate;  // R2 stack components per R2-HIVE §2.1 (TG-agnostic substrate roles)

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{IntoResponse, Json, Redirect},
    routing::get,
    Router,
};
use clap::Parser;
use serde::Serialize;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::{info, warn};

use apiary::ApiaryState;
use bridge::{envelope_to_queued, queued_to_envelope, WireEnvelope};
use hive::EngineHandle;

use std::sync::Arc;

#[derive(Debug, Parser)]
#[command(version, about = "r2-composer workstation orchestrator hive")]
struct Cli {
    #[arg(long, default_value_t = 21050)]
    port: u16,

    #[arg(long, default_value = "127.0.0.1")]
    bind: String,

    #[arg(long, default_value = "webapp")]
    webapp_root: PathBuf,

    /// Optional path to the active apiary directory. Phase 1.7a accepts the
    /// arg but doesn't act on it yet — Phase 1.7+ wires it via the
    /// `apiary` plugin.
    #[arg(long)]
    apiary: Option<PathBuf>,
}

#[derive(Clone)]
struct AppState {
    apiary_path: Option<PathBuf>,
    /// Cached apiary state — populated at startup when `--apiary` is set.
    /// Sent to each /r2 client as `r2.composer.apiary.active` right after
    /// the hello so the webapp's apiary canvas hydrates without polling.
    apiary_state: Option<Arc<ApiaryState>>,
    /// Repo root — used by the WS handler to scan `apiaries/` for the
    /// empty-canvas listing per SPEC-APIARY-CREATE C-A1.
    repo_root: PathBuf,
    engine: EngineHandle,
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
    apiary: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();

    let webapp_root = cli
        .webapp_root
        .canonicalize()
        .unwrap_or_else(|_| cli.webapp_root.clone());
    info!("webapp root: {}", webapp_root.display());

    let repo_root = webapp_root
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    info!("repo root:   {}", repo_root.display());

    // Resolve the apiary path. If `--apiary` is a bare name with no
    // separator, prepend the repo's `apiaries/` directory so the user
    // can pass `--apiary rocker-rig` rather than the full path.
    let apiary_path = cli.apiary.as_ref().map(|p| {
        if p.components().count() == 1 {
            repo_root.join("apiaries").join(p)
        } else {
            p.clone()
        }
    });

    let apiary_state = if let Some(ap) = apiary_path.as_ref() {
        match apiary::load(ap) {
            Ok(state) => {
                info!(
                    "apiary:      {} ({} role-ensembles, {} targets)",
                    ap.display(),
                    state.roles.len(),
                    state.roles.iter().map(|r| r.targets.len()).sum::<usize>(),
                );
                Some(Arc::new(state))
            }
            Err(e) => {
                warn!("apiary load failed for {}: {e}", ap.display());
                None
            }
        }
    } else {
        warn!("apiary:      (none open — pass --apiary <name> or open via the webapp)");
        None
    };

    // Spawn the engine thread with the active apiary path (if any) so
    // the Roster sentant can read/write devices/roster.toml, plus the
    // repo root so the Deploy sentant can resolve catalogue/boards/.
    let engine = hive::spawn(apiary_path.clone(), repo_root.clone());
    info!("engine thread spawned");

    let state = AppState {
        apiary_path,
        apiary_state,
        repo_root: repo_root.clone(),
        engine,
    };

    let app = Router::new()
        .route("/", get(redirect_to_webapp))
        .route("/health", get(health_handler))
        .route("/r2", get(websocket_handler))
        .nest_service(
            "/webapp",
            ServeDir::new(&webapp_root).append_index_html_on_directories(true),
        )
        .nest_service("/catalogue", ServeDir::new(repo_root.join("catalogue")))
        .nest_service("/crates", ServeDir::new(repo_root.join("crates")))
        .nest_service("/scores", ServeDir::new(repo_root.join("scores")))
        .nest_service("/apiaries", ServeDir::new(repo_root.join("apiaries")))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cli.bind, cli.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!("r2-composer orchestrator listening on http://{addr}");
    info!("open: http://{}/webapp/index.html", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("orchestrator stopped cleanly");
    Ok(())
}

// ── Handlers ──────────────────────────────────────────────────────────

async fn redirect_to_webapp() -> Redirect {
    Redirect::permanent("/webapp/index.html")
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        apiary: state.apiary_path,
    })
}

/// /r2 WebSocket endpoint. Bridges the connection to the engine thread.
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(
        socket,
        state.engine,
        state.apiary_state,
        state.repo_root,
    ))
}

async fn handle_socket(
    mut socket: WebSocket,
    engine: EngineHandle,
    apiary_state: Option<Arc<ApiaryState>>,
    repo_root: PathBuf,
) {
    // Capture the USB snapshot for replay — done before any state-
    // sensitive ops so the WS client gets the current attached set
    // right after hello.
    let usb_snapshot_replay: Vec<_> = engine.usb_snapshot.lock().ok()
        .map(|s| s.clone())
        .unwrap_or_default();
    info!("/r2 client connected");

    let mut outbound_rx = engine.subscribe_outbound();

    let hello = WireEnvelope::Hello {
        from: "r2-composer-orchestrator".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        note: Some("Phase 1.7a engine running — Builder sentant active".into()),
    };
    if socket
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .is_err()
    {
        return;
    }

    // Emit one r2.composer.apiary.entry per known apiary so the webapp
    // can populate the empty-canvas picker per SPEC-APIARY-CREATE §2.1
    // / C-A1. The active apiary (if any) appears here AND as
    // apiary.active below — the entry is the picker row; the active is
    // the hydration payload.
    let entries = apiary::list_entries(&repo_root);
    info!(
        "/r2 → emitting {} apiary.entry events (active: {})",
        entries.len(),
        if apiary_state.is_some() { "yes" } else { "no" },
    );
    for entry in &entries {
        let env = WireEnvelope::Event {
            name: "r2.composer.apiary.entry".into(),
            payload: serde_json::to_value(entry).unwrap_or(serde_json::Value::Null),
        };
        let text = serde_json::to_string(&env).unwrap_or_else(|_| "{}".into());
        if socket.send(Message::Text(text.into())).await.is_err() {
            return;
        }
    }

    // Emit the active apiary state right after the entries so the
    // webapp's canvas hydrates without polling.
    if let Some(ap) = apiary_state.as_ref() {
        let env = WireEnvelope::Event {
            name: "r2.composer.apiary.active".into(),
            payload: serde_json::to_value(ap.as_ref()).unwrap_or(serde_json::Value::Null),
        };
        let text = serde_json::to_string(&env).unwrap_or_else(|_| "{}".into());
        if socket.send(Message::Text(text.into())).await.is_err() {
            return;
        }
    }

    // Replay currently-attached USB devices so the canvas footer chip
    // populates immediately for late-connecting clients. (Live attach/
    // detach events thereafter flow through the usb-watcher plugin via
    // the bus.)
    for port in &usb_snapshot_replay {
        let env = WireEnvelope::Event {
            name: "r2.composer.usb.attached".into(),
            payload: serde_json::to_value(port).unwrap_or(serde_json::Value::Null),
        };
        let text = serde_json::to_string(&env).unwrap_or_else(|_| "{}".into());
        if socket.send(Message::Text(text.into())).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            ws_msg = socket.recv() => {
                let Some(msg) = ws_msg else { break };
                match msg {
                    Ok(Message::Text(text)) => {
                        match serde_json::from_str::<WireEnvelope>(&text) {
                            Ok(env) => {
                                if let Some(q) = envelope_to_queued(&env) {
                                    if engine.inbound_tx.try_send(q).is_err() {
                                        warn!("/r2 inbound channel full — dropping event");
                                    }
                                } else {
                                    info!("/r2 ← {text} (non-event envelope, ignored)");
                                }
                            }
                            Err(e) => warn!("/r2 ← unparseable: {e}"),
                        }
                    }
                    Ok(Message::Ping(p)) => { let _ = socket.send(Message::Pong(p)).await; }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            engine_msg = outbound_rx.recv() => {
                if let Ok(q) = engine_msg {
                    let env = queued_to_envelope(&q);
                    let text = serde_json::to_string(&env).unwrap_or_else(|_| "{}".into());
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }

    info!("/r2 client disconnected");
}

// ── Setup ─────────────────────────────────────────────────────────────

fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let env = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn"));
    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false).compact())
        .with(env)
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            warn!("failed to install ctrl_c handler: {e}");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => warn!("failed to install SIGTERM handler: {e}"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    info!("shutdown signal received");
}
