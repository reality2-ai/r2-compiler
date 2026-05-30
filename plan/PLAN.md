# PLAN.md

Current phasing. This file is overwritten as work progresses (PROCESS.md §3); the per-session transcripts in [`../conversation/`](../conversation/) and per-entry `conversation/` dirs accumulate.

## Status (2026-05-31)

**Phase 0 — scaffolding.** ✅ Complete.
**Phase 1.1 — `tools/sync-catalogue.sh`.** ✅ Complete.
**Phase 1.2 — first sync run.** ✅ Complete — see `crates/_VERSIONS.toml` for the manifest.

```
✅ AGENTS.md / AI-CONTEXT.md / README.md / PROCESS.md
✅ specifications/SPEC-R2-COMPILER.md  (v0.1)
✅ specifications/SPEC-CATALOGUE-LAYOUT.md  (v0.2 — restructured for two-part canvas)
✅ catalogue/boards/_README.md + catalogue/ensembles/_README.md
✅ catalogue/boards/esp32-c6-dfr1117/{board.toml, BOARD.md, AI-CONTEXT.md}  ← first concrete entry
✅ catalogue/boards/{esp32-s3-devkitc,esp32-s3-xiao}/AI-CONTEXT.md  ← still placeholder; board.toml/BOARD.md pending
✅ catalogue/boards/<each>/templates/* (synced)
✅ catalogue/boards/<each>/datasheets/HARDWARE-WIRING-*.md (synced)
✅ catalogue/ensembles/rocker-sensor/ensemble.yaml (synced)
✅ crates/{r2-engine,r2-fnv,r2-cbor,r2-wire,r2-trust,r2-route,r2-def,r2-ensemble,r2-wasm}/ (synced)
✅ crates/r2-plugin-crypto-software-ed25519/ (synced, path deps patched)
✅ crates/_VERSIONS.toml
✅ scores/rocker-sensor.yaml (synced)
✅ tools/sync-catalogue.sh
✅ Cargo.toml (empty workspace)
✅ .gitignore
✅ conversation/2026-05-31-r2-compiler-design-01.md
```

Two boards (devkitc, xiao) still need `board.toml` + `BOARD.md` — same pattern as the dfr1117 entry, manually-authored as practice runs before AuthorPilot exists. The rocker-sensor ensemble still needs per-plugin and per-sentant entries scaffolded under its own directory (Phase 1.3 below).

## Phase 1 — catalogue seed + spec-driven build path

Goal: round-trip the three r2-workshop carriers per [`SPEC-R2-COMPILER.md`](../specifications/SPEC-R2-COMPILER.md) §6.

| Step | Output | Dep |
|---|---|---|
| 1.1 | `tools/sync-catalogue.sh` — script to populate `crates/`, `catalogue/boards/<each>/templates/`, `catalogue/ensembles/rocker-sensor/ensemble.yaml` from sibling repos | — | ✅ |
| 1.2 | First sync run | 1.1 | ✅ |
| 1.3 | `board.toml` + `BOARD.md` + completed `AI-CONTEXT.md` for the remaining two carriers (`esp32-s3-devkitc`, `esp32-s3-xiao`) — same manual pattern as `esp32-c6-dfr1117/board.toml` | 1.2 | ⏳ |
| 1.4 | `catalogue/ensembles/rocker-sensor/` per-plugin + per-sentant entries — extracted from `ensemble.yaml` declarations and the r2-workshop firmware code (each becomes a R2-PLUGIN §12 / R2-DEF §2 conformant directory) | 1.2 | ⏳ |
| 1.5 | `testing/round-trip/<carrier>.expected.toml` — recorded R2-WIRE traffic from a running r2-workshop firmware, captured as the conformance baseline | 1.3, 1.4 | ⏳ |
| 1.6 | `orchestrator/` scaffolding — axum WSS + static serve on port 21050; `CatalogueServer` + `Compiler` + `Sync` sentants stubbed | 1.2 | ⏳ |
| 1.7 | `orchestrator/prompts/compile.md` — Tera template for the Claude Code build brief (must emit direct-Rust FSMs per [[feedback-aot-optimisation-constraint]]) | 1.6 | ⏳ |
| 1.8 | End-to-end: orchestrator reads `scores/rocker-sensor.yaml` + carrier choice → spawns `claude -p` → produces `out/<carrier>/` → `cargo build --release --target <triple>` exits 0 | 1.5, 1.7 | ⏳ |
| 1.9 | Conformance gate: behavioural-equivalence test passes against `testing/round-trip/` vectors for all three carriers | 1.8 | ⏳ |

## Phase 2 — webapp + visual canvas

| Step | Output | Dep |
|---|---|---|
| 2.1 | `webapp/crate/` — Rust crate → wasm32-unknown-unknown; `Catalogue`, `Composition`, `SourceViewer`, `Builder`, `Author` sentants | Phase 1 |
| 2.2 | `webapp/ui/` — plain JS DOM + drag-and-drop canvas + CodeMirror/shiki source viewer | 2.1 |
| 2.3 | Operator can compose on the canvas, click Compile, and see the same build flow as Phase 1 but driven from the browser | 2.2 |
| 2.4 | Operator can `+ New Plugin` etc. and the `Author` flow produces a valid catalogue entry through agent dialog | 2.3 |

## Phase 3 — flash + iterate

| Step | Output |
|---|---|
| 3.1 | `Flasher` sentant — wraps `esptool` per R2-BUILD §5.1 |
| 3.2 | `r2.compiler.flash.*` events surfaced in the UI |
| 3.3 | Live USB device detection (libudev / hotplug) |

## Phase 4 — pin-connection visualisation (deferred)

See [memory: project_phase2_pin_visualisation.md](../../../home/roycdavies/.claude/projects/-mnt-data-Development-R2-r2-compiler/memory/project_phase2_pin_visualisation.md) for the design intent. Not started; not blocking earlier phases.

## Open conjectures

- **C-1**: `claude -p` in a non-interactive subprocess with `--output-format=stream-json` is enough to drive an autonomous build cycle. Falsifies if: tool-permission prompts can't be answered from outside the CLI without `--dangerously-skip-permissions`.
- **C-2**: A behavioural-equivalence test (recorded R2-WIRE traffic) is a sufficient conformance gate. Falsifies if: there's a state we care about that doesn't manifest on the wire (e.g. SD ring layout) and only shows up in field testing.
- **C-3**: Vendoring `crates/` from `r2-core` is cheap enough to do per-release. ✅ **Survived** at Phase 1.1 — sync ran in <1 s, no churn. Re-check when r2-core's crate tree restructures.
- **C-4**: One per-carrier crate per build under `out/<carrier>-<timestamp>/` keeps build state hermetic. Falsifies if: `esp-idf-sys`'s caching reaches across timestamped dirs and produces stale binaries.
- **C-5**: Claude Code can reliably generate compact Rust FSMs from sentant.yaml inputs that fit within 80% of the OTA-slot budget for each target carrier (per [[feedback-aot-optimisation-constraint]]). Falsifies if: generated output is larger than the hand-written r2-workshop equivalents by more than 20%, or if compilation requires more than three CC retry cycles to converge on a passing build.

Each conjecture should be either survived or refuted explicitly during Phase 1.

## Architectural constraints worth remembering

- **AOT-compile to direct Rust, NOT a generic engine.** OTA flash budget is load-bearing — 1.875 MB per slot on dfr1117. The Compiler sentant's main job is generating Rust source code that implements each sentant.yaml FSM as direct match arms + static structs, NOT shipping `r2-engine` with `dyn Sentant` dispatch + a preemption scheduler. See [memory: feedback-aot-optimisation-constraint](../../../home/roycdavies/.claude/projects/-mnt-data-Development-R2-r2-compiler/memory/feedback_aot_optimisation_constraint.md). The pragmatic firmware shape r2-workshop already ships IS the target — r2-compiler mechanises producing that shape from a YAML score.
