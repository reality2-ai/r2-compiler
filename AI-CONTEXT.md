# AI-CONTEXT.md — fresh-session entry point

If you are an AI assistant being asked to continue work on this project with no prior conversation memory, **read this file first** (it should take under 2 minutes), then the files it points at. Do not relitigate binding decisions — if you think one needs reopening, raise it with the user explicitly.

---

## What this project is

**r2-composer** is a visual composer for Reality2 (R2) firmware. The operator picks a carrier board (e.g. DFR1117 ESP32-C6), a set of R2 plugins, and a set of R2 sentants from a catalogue arrayed around a canvas, and the tool drives a Claude Code session that produces a flashable per-carrier firmware crate.

The compile work is delegated to Claude Code (`claude -p '<brief>' --output-format=stream-json`) running on the operator's workstation — the **same way Roy already works with r2-workshop manually**, but driven by the visual composition instead of free-form chat. r2-composer does NOT contain a from-scratch code generator; it composes briefs and orchestrates CC sessions.

## Architecture (target shape)

```
Browser tab (WASM R2 hive)                       Workstation (Rust orchestrator hive)
─────────────────────────                        ───────────────────────────────────
catalogue browser                  R2-WIRE       compiler plugin
visual canvas (drag-and-drop)   ─── over ────►   claude-code plugin ──► claude -p ...
source viewer (Rust read-only)    WSS            flasher plugin    ──► esptool ...
score-preview pane                               sync plugin       ──► git pull r2-core
build-progress console
                                                 Hosts: catalogue/, scores/, crates/, out/
```

Both sides are R2 hives — TG members. The architecture mirrors `r2-workshop/webapp/` ↔ `r2-workshop/dashboard/`.

## What "the catalogue" is

The catalogue is a **growing, authorable knowledge base**, not a fixed inventory. **The canvas exposes exactly two opt-in part types: carrier boards and ensembles.** Plugins and sentants are NOT separate catalogue trees — they live inside the ensemble (ensemble-owned) or carrier (hive-shared singletons) that uses them.

| Branch | What lives here | Authoritative spec |
|---|---|---|
| `catalogue/boards/<arch>-<chip>-<carrier>/` | Per-carrier `board.toml`, templates, datasheets, narrative, AI-CONTEXT, conversation. May contain `plugins/` for hive-shared singletons on this carrier (BLE/WiFi radio, R2-WEB). | R2-COMPILE §4, R2-BUILD §2, R2-HW |
| `catalogue/ensembles/<name>/` | Per-ensemble `ensemble.yaml` (R2-DEF §7) + `plugins/<cat>/<name>/` (ensemble-owned plugins) + `sentants/<Name>/` + narrative + AI-CONTEXT + conversation. | R2-ENSEMBLE, R2-DEF §7, R2-PLUGIN §12, R2-DEF §2 |

Always-available infrastructure (the R2 stack + core plugins like Ed25519 crypto) lives in [`crates/`](crates/) and is linked into every build unconditionally — it does NOT appear in the catalogue UI.

The operator adds new entries by dialog with the agent (you, when invoked through r2-composer's compile-console pane). Every entry MUST be a self-contained directory — code + datasheets + AI-CONTEXT — that a future fresh CC session can pick up cold.

See [`specifications/SPEC-CATALOGUE-LAYOUT.md`](specifications/SPEC-CATALOGUE-LAYOUT.md) for the normative directory shape and [`AGENTS.md`](AGENTS.md) §3–4 for the authoring discipline.

## Read these in this order

1. [`README.md`](README.md) — what this is, in plain language.
2. [`AGENTS.md`](AGENTS.md) — orientation for AI agents, especially **§1 upstream contracts** and **§4 catalogue entry shape**.
3. [`PROCESS.md`](PROCESS.md) — the five working rules.
4. [`specifications/SPEC-R2-COMPOSER.md`](specifications/SPEC-R2-COMPOSER.md) — what r2-composer IS, in canonical R2 vocabulary (class string, hive role, plugin shape).
5. [`specifications/SPEC-CATALOGUE-LAYOUT.md`](specifications/SPEC-CATALOGUE-LAYOUT.md) — normative directory shape for catalogue entries.
6. [`plan/PLAN.md`](plan/PLAN.md) — current phasing + open work.
7. The latest file in [`conversation/`](conversation/) — most recent design rationale.
8. Sibling repos when relevant: `../r2-specifications/AGENTS.md`, `../r2-workshop/AI-CONTEXT.md`, `../r2-core/README.md`.

## Working conventions (binding)

| # | Rule |
|---|---|
| 1 | **Spec before code.** Every behaviour change has a driving spec in [`specifications/`](specifications/) or upstream in `r2-specifications/`. The spec wins disagreements unless the user re-opens. |
| 2 | **Conversation is research data.** Every session appends a new file `conversation/YYYY-MM-DD-<topic>-NN.md` (verbatim user, faithful AI, decisions table). Per-catalogue-entry conversations live in that entry's `conversation/`. Never edit a closed session retroactively. |
| 3 | **Catalogue conformance gate.** A catalogue entry without a valid canonical artefact (`plugin.toml` / `sentant.yaml` / `board.toml`) and a complete `AI-CONTEXT.md` is incomplete, not published. |
| 4 | **Secrets stay out.** No TG private keys, no WiFi creds, no API tokens. `.gitignore` blocks the patterns; *don't put them there in the first place* is the real rule. |
| 5 | **Cite sources.** Spec section, datasheet page, vendor URL, file:line for code refs. Don't fabricate. |

## Current state (2026-05-31)

**Scaffolding phase.** The directory tree, top-level docs (AGENTS.md, AI-CONTEXT.md, PROCESS.md, README.md), and the two normative specs (SPEC-R2-COMPOSER, SPEC-CATALOGUE-LAYOUT) exist. No Rust has been written yet. No catalogue entries are populated yet — the three target boards (`esp32-s3-devkitc`, `esp32-s3-xiao`, `esp32-c6-dfr1117`) have placeholder dirs only.

Next steps live in [`plan/PLAN.md`](plan/PLAN.md).

## Decisions worth knowing (locked in this design session)

1. **Compile backend is Claude Code**, invoked as `claude -p '<brief>' --output-format=stream-json` per build. Not the Anthropic SDK directly.
2. **Webapp is a WASM R2 hive in the browser**, same shape as `r2-workshop/webapp/`. Not Tauri / Electron / native.
3. **Orchestrator is a workstation Rust binary** that serves the WASM bundle from localhost AND hosts an R2 hive on the same port (peek-protocol-detect per R2-WIRE §13.5).
4. **Catalogue is self-contained but synced** from `r2-core/` (plugins) and `r2-workshop/` (sentants + boards). `tools/sync-catalogue.sh` keeps it in step. Drift = bug, not backlog.
5. **First success metric** = round-trip the three existing r2-workshop carriers behaviourally (same R2-WIRE traffic against the round-trip vectors).
6. **Phase 2 deferred:** graphical pin-connection diagram showing how to wire the chosen board + plugin set.

These are not to be relitigated without explicit user re-opening.

## When the user says…

- **"Add a new plugin/board/sentant"** → open the relevant category, scaffold the directory per [`AGENTS.md`](AGENTS.md) §4, fetch datasheets, author the canonical artefact + narrative + AI-CONTEXT, validate against the upstream spec, commit a `conversation/` transcript.
- **"Show me the source of <plugin>"** → that's the source viewer working — `catalogue/plugins/<category>/<name>/src/lib.rs`. The webapp surfaces it; you can Read it directly.
- **"Compile firmware for <board>"** → the visual flow: drag board onto canvas, add plugins/sentants, hit Compile. Under the hood, you (Claude Code) get a brief + the score + the board's templates and produce `out/<board>/`.
- **"Save the conversation"** → append a new file in `conversation/` (repo-wide or per-entry as appropriate) with today's date and a `-NN.md` suffix.

## What NOT to do

- Don't invent a "r2-composer-specific" plugin schema. `plugin.toml` per R2-PLUGIN §12.3 is the only source of truth.
- Don't add `path = "../r2-core"` deps. The repo is self-contained; sync via `tools/sync-catalogue.sh`.
- Don't mock a successful compile. If the tool claims build success, it must have observed `cargo build` exit 0.
- Don't relitigate binding decisions without explicit user re-opening.
- Don't summarise what you just did at the end of every response — the user reads the diff.

---

*Last touched 2026-05-31 — design session 01, scaffolding committed.*
