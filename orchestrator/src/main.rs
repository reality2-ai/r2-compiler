//! r2-compiler orchestrator — workstation-side R2 hive.
//!
//! Phase 1.6 scope: the minimal binary that listens on a single port,
//! serves the webapp bundle as static files, and exposes `/r2` as a
//! WebSocket endpoint (which currently just accepts connections + logs).
//!
//! Phase 1.7+ wires in the real R2 hive (`r2-engine`), the `claude-code`
//! plugin (subprocess driver), the compile + author + deploy flows
//! per SPEC-R2-COMPILER §5 / §11 / §12.
//!
//! ## CLI
//!
//! ```text
//! r2-compiler-orchestrator [--port 21050] [--webapp-root webapp/] [--apiary path/to/apiary]
//! ```
//!
//! ## Endpoints
//!
//! - `GET /` → redirects to `/webapp/index.html`
//! - `GET /webapp/...` → static files from `--webapp-root`
//! - `GET /catalogue/...` → static files from the repo's `catalogue/` (so the webapp can fetch entry files)
//! - `GET /crates/...` → static files from `crates/` (source viewer)
//! - `GET /scores/...` → static files from `scores/`
//! - `GET /r2` (WebSocket) → R2 event stream — currently a stub.
//! - `GET /health` → JSON `{ "status": "ok", "version": "..." }`

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

/// CLI for the orchestrator.
#[derive(Debug, Parser)]
#[command(version, about = "r2-compiler workstation orchestrator hive")]
struct Cli {
    /// Port to bind. Defaults to 21050 (the SPEC-R2-COMPILER §3 default).
    #[arg(long, default_value_t = 21050)]
    port: u16,

    /// Bind address. Defaults to localhost; use 0.0.0.0 to expose to the LAN.
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,

    /// Path to the webapp directory. Defaults to "webapp" relative to cwd.
    #[arg(long, default_value = "webapp")]
    webapp_root: PathBuf,

    /// Optional path to the active apiary directory. When set, the orchestrator
    /// scopes its state to this apiary per SPEC-APIARY-LAYOUT §6. Phase 1.6
    /// accepts the path but doesn't act on it yet — Phase 1.7+ wires it in.
    #[arg(long)]
    apiary: Option<PathBuf>,
}

/// Shared application state — small for now; grows as the hive lands.
#[derive(Clone)]
struct AppState {
    /// Optional active-apiary path (informational in v0.1).
    apiary_path: Option<PathBuf>,
}

/// Health-check response body.
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

    let webapp_root = cli.webapp_root.canonicalize().unwrap_or(cli.webapp_root.clone());
    info!("webapp root: {}", webapp_root.display());

    // The repo root is the parent of webapp/. Most catalogue / crates /
    // scores paths the webapp fetches resolve relative to it.
    let repo_root = webapp_root.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    info!("repo root:   {}", repo_root.display());

    if let Some(ap) = cli.apiary.as_ref() {
        info!("apiary:      {}", ap.display());
    } else {
        warn!("apiary:      (none open — open via the webapp's apiary switcher when available)");
    }

    let state = AppState {
        apiary_path: cli.apiary.clone(),
    };

    let app = Router::new()
        .route("/", get(redirect_to_webapp))
        .route("/health", get(health_handler))
        .route("/r2", get(websocket_handler))
        .nest_service(
            "/webapp",
            ServeDir::new(&webapp_root)
                .append_index_html_on_directories(true),
        )
        // The webapp fetches ../catalogue/... + ../crates/... + ../scores/...
        // — surface them at the corresponding URL prefixes off the repo root.
        .nest_service("/catalogue", ServeDir::new(repo_root.join("catalogue")))
        .nest_service("/crates",    ServeDir::new(repo_root.join("crates")))
        .nest_service("/scores",    ServeDir::new(repo_root.join("scores")))
        .nest_service("/apiaries",  ServeDir::new(repo_root.join("apiaries")))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cli.bind, cli.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!("r2-compiler orchestrator listening on http://{addr}");
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

/// /r2 WebSocket endpoint. Phase 1.6 stub — accepts connections, echoes a
/// hello, logs anything the client sends. Phase 1.7 wires this into the
/// real R2-WIRE event bus.
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    info!("/r2 client connected");

    // Send a hello message so the browser-side handler has something
    // visible on the wire during early bring-up. Phase 1.7 replaces this
    // with the proper r2.compiler.apiary.active hello envelope.
    let hello = serde_json::json!({
        "kind": "hello",
        "from": "r2-compiler-orchestrator",
        "version": env!("CARGO_PKG_VERSION"),
        "note": "Phase 1.6 stub — R2-WIRE events arrive in Phase 1.7+",
    });
    if socket.send(Message::Text(hello.to_string().into())).await.is_err() {
        warn!("/r2 client disconnected before hello could be sent");
        return;
    }

    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Text(text)) => {
                info!("/r2 ← {}", text);
                // Phase 1.6 stub: echo back wrapped in an ack.
                let ack = serde_json::json!({ "kind": "ack", "echo": &*text });
                if socket.send(Message::Text(ack.to_string().into())).await.is_err() {
                    break;
                }
            }
            Ok(Message::Binary(b)) => {
                info!("/r2 ← {} bytes binary (Phase 1.7+: R2-WIRE frames)", b.len());
            }
            Ok(Message::Ping(p)) => {
                let _ = socket.send(Message::Pong(p)).await;
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    info!("/r2 client disconnected");
}

// ── Setup ─────────────────────────────────────────────────────────────

fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let env = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn"));
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
