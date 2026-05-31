# orchestrator/

The workstation-side R2 hive — a Rust binary that:

- Hosts an R2 hive (TG member, same architectural shape as `r2-workshop/dashboard/`).
- Serves the webapp WASM bundle on `localhost:21050/webapp/...`.
- Listens for `r2.composer.*` events from the browser hive at `/r2` (WebSocket).
- Runs the plugins (compiler subprocess management, cargo, esptool, webfetch, git, …) that do the actual work.
- Holds the TG KeyHolder material (off-tree at `~/.config/r2-composer/apiaries/<name>/tg_signer/`).

Per [[feedback-sentants-vs-plugins-terminology]]: sentants here are thin FSMs that route events; the imperative work happens in plugins.

## Status (2026-05-31): Phase 1.6 scaffolding ✅

What's running today:

- Single binary `r2-composer-orchestrator` built from `orchestrator/src/main.rs`.
- Listens on `127.0.0.1:21050` by default (configurable via `--port` / `R2_COMPILER_PORT`).
- Routes:

  | Route | Purpose |
  |---|---|
  | `GET /` | 308 → `/webapp/index.html` |
  | `GET /health` | JSON `{ status, version, apiary }` |
  | `GET /webapp/...` | Static files from `webapp/` |
  | `GET /catalogue/...` | Static files from `catalogue/` |
  | `GET /crates/...` | Static files from `crates/` (source viewer) |
  | `GET /scores/...` | Static files from `scores/` |
  | `GET /apiaries/...` | Static files from `apiaries/` |
  | `GET /r2` (WebSocket) | R2 event stream — **stub** today; Phase 1.7+ wires the real R2-WIRE event bus. |

- Structured logging via `tracing`; configurable via `RUST_LOG`.
- Graceful Ctrl-C / SIGTERM handling.

## What's NOT here yet

| Feature | Status |
|---|---|
| Real R2 hive (`r2-engine` + sentants + plugins) | Phase 1.7+ |
| TG / KeyHolder management | Phase 1.7+ ([[project-tg-management-workflow]]) |
| `claude-code` plugin (subprocess driver for `claude -p`) | Phase 1.7+ |
| `cargo-runner` / `flasher` / `ota-push` / `webfetch` / `git-runner` plugins | Phase 1.7+ |
| Compiler plugin (central `r2.composer.build.*` driver) | Phase 1.7 |
| `apiary` plugin (file-system watcher + apiary roster) | Phase 1.7 |
| `meta/` self-description | Phase 1.6 companion task |

The Phase 1.6 commit is intentionally a thin shell so the binary lands working before any half-implemented hive logic does.

## Running it

```bash
# From the repo root:
./orchestrator/run.sh
# → http://localhost:21050/webapp/index.html
```

The script:
1. Refreshes the catalogue manifest (`tools/build-catalogue-index.py`).
2. Builds the WASM bundle if missing (when `wasm-pack` is installed).
3. Builds the orchestrator (`cargo build -p orchestrator --release`).
4. Launches the binary; Ctrl-C to stop.

For faster iteration use `./orchestrator/run.sh debug` (skips release-profile LTO).

## Drop-in vs `webapp/run.sh`

The orchestrator binary is a drop-in replacement for the Python `http.server` used by `webapp/run.sh`:

| | `webapp/run.sh` | `orchestrator/run.sh` |
|---|---|---|
| Server | `python3 -m http.server` | `r2-composer-orchestrator` (axum) |
| Static routes | Yes | Yes |
| `/r2` WebSocket | No | Yes (stub today; real R2-WIRE Phase 1.7+) |
| Per-request structured logging | No | Yes (`tracing`) |
| `/health` endpoint | No | Yes (JSON) |
| Apiary scoping (`--apiary <path>`) | No | Stub today (accepts the arg; doesn't act on it yet) |
| Build dep | `python3` | `cargo` |

Pick `webapp/run.sh` while iterating on the catalogue + WASM hash module (no Rust rebuild). Pick `orchestrator/run.sh` when you want the `/r2` endpoint or apiary-scoped state.

## Spec

See [`../specifications/SPEC-R2-COMPOSER.md`](../specifications/SPEC-R2-COMPOSER.md) — especially §3.2 (orchestrator hive sentants), §3.3 (orchestrator hive plugins), §4 (event vocabulary), §5 (compile path), §11 (TG management), §12 (device lifecycle + deploy paths). Companion: [`../specifications/SPEC-APIARY-LAYOUT.md`](../specifications/SPEC-APIARY-LAYOUT.md) for apiary directory + `apiary.toml` schema.
