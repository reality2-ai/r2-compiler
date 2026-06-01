# r2-composer

**A calm-computing composer for Reality2 firmware.** Drag a carrier board and an ensemble onto the canvas, chat with Claude Code to refine, and the tool scaffolds + compiles + flashes per-target firmware for the apiary you're building.

The act of authoring is conversational. The canvas shows you the structure of what you're building (carrier boards, role-ensembles, plugins, sentants, devices) and the chat refines it. The AI does the actual file-writing; you describe intent, see ambient updates, confirm destructive operations.

---

## What works today

| Capability | Status |
|---|---|
| Empty-canvas apiary picker — list known apiaries in `apiaries/` | ✓ |
| Chat-driven new-apiary creation — scaffold + TG keypair + git init in one atomic transaction | ✓ |
| Catalogue browsing — boards / ensembles / plugins / sentants with class-hash badges | ✓ |
| Apiary canvas — role-ensembles, targets, plugin overrides, device slots | ✓ |
| Spec-as-brief authoring — board / ensemble / plugin / sentant / apiary / flash kinds dispatch to Tera templates that splice the matching SPEC section into the AI brief | ✓ |
| Roster state machine — slots progress `placeholder → built → flashed_pending_pk → enrolled → reachable / unreachable / revoked / retired` per SPEC-APIARY-FLASH §2 | partial (placeholder + flashed_pending_pk live; rest land per F3+) |
| USB watcher — Linux poll-based detection of attached serial devices; carrier guess from `board.toml [usb]` tables | ✓ (Linux only) |
| `esptool` flash dispatch — chat tool-call → flasher plugin → progress streamed into the Build pane | ✓ |
| BLE-bootstrap WiFi provisioning + cert minting | F3 — next leg |
| OTA push | F5 — after F3 |
| Per-target compile flow (cargo / wasm-pack / mix release fan-out) | future |

Three carriers live in the catalogue: `esp32-s3-devkitc`, `esp32-s3-xiao`, `esp32-c6-dfr1117`.

The rocker-rig apiary at `apiaries/rocker-rig/` is the canonical worked example used by SPEC-APIARY-COMPOSE §12 and the canvas tests.

---

## Quick start

```bash
git clone https://github.com/reality2-ai/r2-composer.git
cd r2-composer
./install.sh                          # check prerequisites, install what's safe
./orchestrator/run.sh                 # build + launch on http://localhost:21050
# open http://localhost:21050/webapp/index.html in your browser
```

For an existing apiary:
```bash
./orchestrator/run.sh --apiary rocker-rig
```

The chat in the bottom pane talks to Claude Code. Click `+ New apiary…` (or just type *"I want to start a new apiary for ..."*) to scaffold one.

---

## Architecture (one-screen overview)

Two cooperating R2 hives, both running locally:

```
┌──────────────────────────────────────────────────────────────────────┐
│ Browser                                                              │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │ Webapp hive (WASM)                                            │   │
│  │  · Apiary canvas (compose surface)                            │   │
│  │  · Catalogue browser (read-mostly, file viewer)               │   │
│  │  · Chat pane (talks to Claude Code via the orchestrator)      │   │
│  │  · USB footer (chip per attached serial device)               │   │
│  └─────────────────┬────────────────────────────────────────────┘    │
└────────────────────┼──────────────────────────────────────────────────┘
                     │ /r2 WebSocket — JSON event envelopes
┌────────────────────┼──────────────────────────────────────────────────┐
│ Workstation        ▼                                                  │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │ Orchestrator hive (Rust)                                      │   │
│  │  · axum HTTP/WS server on :21050                              │   │
│  │  · r2-engine event bus + sentants:                            │   │
│  │      Apiary  Builder  Author  Roster  Deploy                  │   │
│  │  · Plugins:                                                   │   │
│  │      claude-code  flasher  usb-watcher                        │   │
│  └──────────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────────┘
                     │ subprocess
                     ▼
                ┌──────────┐         ┌──────────┐
                │ claude -p│         │ esptool  │
                └──────────┘         └──────────┘
```

The orchestrator drives **Claude Code as a subprocess** (per-chat or per-build) and **esptool** for USB flashing. Both are real CLIs the operator already has installed (or `install.sh` helps them install).

Everything else — the chat templates, the spec-as-brief loop, the canvas state, the apiary directory layout — is described in the specs under `specifications/`.

---

## Project layout

```
r2-composer/
├── apiaries/                  # operator's deployments (each is its own git repo)
│   └── rocker-rig/            # bundled worked example (this one IS tracked)
├── catalogue/                 # the parts library — boards / ensembles / plugins / sentants
│   ├── boards/
│   │   ├── esp32-s3-devkitc/
│   │   ├── esp32-s3-xiao/
│   │   └── esp32-c6-dfr1117/
│   └── ensembles/
│       └── rocker-sensor/
├── crates/                    # vendored R2 protocol stack + always-linked core plugins
├── orchestrator/              # workstation-side R2 hive (Rust)
│   ├── prompts/               # Tera templates — the spec-as-brief authoring system
│   └── src/
│       ├── plugins/           # claude-code, flasher, usb-watcher
│       └── sentants/          # Apiary, Author, Builder, Deploy, Roster
├── webapp/                    # browser-side R2 hive (WASM + JS + HTML)
│   ├── crate/                 # Rust → wasm-bindgen → r2_composer_webapp
│   └── ui/                    # vanilla JS + CSS — no framework
├── specifications/            # RFC-2119-normative specs
│   ├── SPEC-R2-COMPOSER.md         — top-level architecture
│   ├── SPEC-CATALOGUE-LAYOUT.md    — boards / ensembles / plugins / sentants
│   ├── SPEC-APIARY-LAYOUT.md       — apiary directory + apiary.toml schema
│   ├── SPEC-APIARY-COMPOSE.md      — compose tree, target taxonomy, canvas
│   ├── SPEC-APIARY-CREATE.md       — new-apiary workflow
│   └── SPEC-APIARY-FLASH.md        — device flash workflow
├── plan/PLAN.md
├── AGENTS.md                   # contribution + agent-coding conventions
├── PROCESS.md                  # secrets, commits, push policy
└── install.sh                  # fresh-checkout bootstrap
```

---

## Prerequisites

`install.sh` checks all of these. If you'd rather install by hand:

| Tool | Purpose | Install hint |
|---|---|---|
| **rust + cargo** (stable) | Build the orchestrator + WASM crate | `curl https://sh.rustup.rs -sSf \| sh` |
| **wasm32-unknown-unknown** target | WASM target for the webapp | `rustup target add wasm32-unknown-unknown` |
| **wasm-pack** | wasm-bindgen pipeline for the webapp | `cargo install wasm-pack` |
| **claude** CLI | The AI behind the chat + authoring | `npm install -g @anthropic-ai/claude-code` |
| **python3** (3.10+) | `tools/build-catalogue-index.py` + WS test scripts | distro package |
| **esptool** | USB first-install for ESP boards | `pip install esptool` |
| **git** | Version control | distro package |
| **gh** CLI (optional but recommended) | `apiary.git.publish` flow | <https://cli.github.com/> |

**Linux USB access**: add yourself to the `dialout` group so `/dev/ttyACM*` is readable without sudo:
```bash
sudo usermod -aG dialout $USER
# then log out and back in
```

**Platform**: Linux is the primary target. macOS works for everything except USB detection (the `usb-watcher` plugin uses `/sys/class/tty/`); Windows is untested.

---

## Run + verify

```bash
./orchestrator/run.sh                                     # builds + launches
curl -s http://localhost:21050/health                     # → {"status":"ok",...}
```

Browse to `http://localhost:21050/webapp/index.html`. You should see:

- Header with the `Apiary` pill + `+ New apiary…` button + `/r2` connection dot (green when connected).
- Centre canvas in empty-canvas mode listing apiaries under `apiaries/` (rocker-rig + anything you've authored).
- Chat tab at the bottom — type a message and Claude responds in markdown.
- USB footer (hidden if nothing's plugged in; populates as devices appear).

---

## Development

| Task | Command |
|---|---|
| Run all orchestrator tests | `cargo test -p orchestrator` |
| Run a specific test module | `cargo test -p orchestrator roster` |
| Build the WASM bundle | `./webapp/build-wasm.sh` |
| Refresh the catalogue manifest | `python3 tools/build-catalogue-index.py` |
| Sync vendored crates from r2-core | `./tools/sync-catalogue.sh` |
| Build orchestrator (release) | `cargo build -p orchestrator --release` |
| Debug logging | `RUST_LOG=debug ./orchestrator/run.sh debug` |

The Tera prompt templates that drive Claude Code authoring live at `orchestrator/prompts/*.md.tera`. Edit and rebuild to iterate.

---

## Where to read more

The specs under `specifications/` are RFC-2119-normative and authoritative. Quick map:

- **First-time understanding**: `SPEC-R2-COMPOSER.md` (15 min)
- **Catalogue entry schemas**: `SPEC-CATALOGUE-LAYOUT.md` §3 (board), §4 (ensemble), §5 (plugin), §6 (sentant)
- **Apiary structure**: `SPEC-APIARY-LAYOUT.md`
- **Compose surface**: `SPEC-APIARY-COMPOSE.md` (canvas + targets + compile fan-out)
- **Workflows**: `SPEC-APIARY-CREATE.md` (new apiary), `SPEC-APIARY-FLASH.md` (USB + OTA flash)

Contribution conventions in `AGENTS.md`; secrets / commit / push policy in `PROCESS.md`.

---

## Status note

r2-composer is pre-v1 and evolves in conversation with its primary author. The destination architecture is laid out in the specs; the implementation lands chunk-by-chunk against those specs. Commit history is the canonical changelog — each commit message is a focused account of what landed and why.

If you're cloning this to use it: expect rough edges, expect breaking changes between commits, and read the specs before building on top of internal events.
