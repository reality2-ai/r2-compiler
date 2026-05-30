# PLAN.md

Current phasing. This file is overwritten as work progresses (PROCESS.md §3); the per-session transcripts in [`../conversation/`](../conversation/) and per-entry `conversation/` dirs accumulate.

## Status (2026-05-31)

**Phase 0 — scaffolding.** ✅ Complete.
**Phase 0.5 — GitHub repo + initial push.** ✅ Complete — https://github.com/reality2-ai/r2-compiler
**Phase 1.1 — `tools/sync-catalogue.sh`.** ✅ Complete.
**Phase 1.2 — first sync run.** ✅ Complete — see `crates/_VERSIONS.toml` for the manifest.
**Phase 1.3 — author all three board entries.** ✅ Complete — board.toml + BOARD.md + AI-CONTEXT.md for esp32-s3-devkitc, esp32-s3-xiao, esp32-c6-dfr1117.
**Phase 1.4-metadata (first slice).** ✅ Complete — `[compulsory_plugins]` added to all three board.tomls; SPEC-R2-COMPILER §11 (TG management) + §12 (device lifecycle + deploy paths + compulsory plugins) added; SPEC-CATALOGUE-LAYOUT §4.3 amended with three modes (aot/nif/web); first worked-example plugin (`sensor/lis2dh`) and sentant (`Identity`) fully authored as metadata.
**Phase 1.4-source (first slice).** ✅ Complete — `r2-plugin-sensor-lis2dh` Cargo crate authored from the r2-workshop reference: `no_std` for AOT, generic over `embedded_hal::i2c::I2c`, implements `r2_engine::plugin::Plugin`, **9 unit tests passing** via `embedded-hal-mock`. The vendored R2 crates (r2-engine, r2-fnv, r2-cbor, r2-wire, plus r2-dispatch added by sync) now form a host-buildable workspace. Conjecture **C-5 (the plugin authoring pattern produces a buildable crate) survives its first test.** Workspace deps tidied; terminology corrected throughout (compiler/author/flasher/sync are PLUGINS not sentants, per [[feedback-sentants-vs-plugins-terminology]]).
**Phase 2-preview.** ✅ Complete — minimal browseable static webapp (`webapp/index.html` + `webapp/ui/app.js` + `webapp/styles/main.css`) reads `webapp/dist/manifest.json` (built by `tools/build-catalogue-index.py`) and renders the catalogue: boards panel left, ensembles panel right (with nested plugins + sentants), workspace centre with entry detail + file viewer. Apiary placeholder pill in header. Launch with `webapp/run.sh` → `http://localhost:8080/webapp/`. No WASM hive yet — Phase 2-full is the real visual canvas.
**Second pass (2026-05-31).** ✅ Complete — three coupled decisions ratified:
1. **Core/opt-in reclassification:** `nvs` + `clock` moved from `catalogue/` to `crates/` as always-on core plugins (`crates/r2-plugin-storage-nvs/`, `crates/r2-plugin-time-clock/`). Final core-plugin set in `crates/`: software-ed25519, ble-beacon, ble-l2cap, ota-tcp (switchable), nvs, clock = 6 plugins. Catalogue retains the 8 opt-in plugins (adxl355, lis2dh, battery-adc, sd-card, led, log-tcp, data-tcp, reset-tcp).
2. **Naming "apiary":** broaden R2-APIARY upstream to encompass TG-bound multi-hive deployments (the operator's unit of work); the current multi-processor-single-identity case becomes a labeled specialisation. New `specifications/SPEC-APIARY-LAYOUT.md` (v0.1) defines the r2-compiler-side contract (`apiaries/<name>/` + `apiary.toml`). New `specifications/SPEC-APIARY-AMENDMENT-PROPOSAL.md` (v0.1) is the upstream amendment proposal awaiting Roy's ratification.
3. **r2-compiler is itself structurally an R2 ensemble** (class `ai.reality2.ensemble.r2-compiler`) but at runtime its hives inherit the active apiary's TG context (no standing r2-compiler TG; honours R2-TRUST §2.3). `meta/` self-description deferred to Phase 1.6+ (when the orchestrator + webapp Rust sources are authored). SPEC-R2-COMPILER §13 + §14 added.

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

Two boards (devkitc, xiao) still need `board.toml` + `BOARD.md` — same pattern as the dfr1117 entry, manually-authored as practice runs before the authoring flow exists. The rocker-sensor ensemble still needs per-plugin and per-sentant entries scaffolded under its own directory (Phase 1.3 below).

## Phase 1 — catalogue seed + spec-driven build path

Goal: round-trip the three r2-workshop carriers per [`SPEC-R2-COMPILER.md`](../specifications/SPEC-R2-COMPILER.md) §6.

| Step | Output | Dep |
|---|---|---|
| 1.1 | `tools/sync-catalogue.sh` — script to populate `crates/`, `catalogue/boards/<each>/templates/`, `catalogue/ensembles/rocker-sensor/ensemble.yaml` from sibling repos | — | ✅ |
| 1.2 | First sync run | 1.1 | ✅ |
| 1.3 | `board.toml` + `BOARD.md` + completed `AI-CONTEXT.md` for all three carriers (`esp32-c6-dfr1117`, `esp32-s3-devkitc`, `esp32-s3-xiao`) — same manual pattern across all three | 1.2 | ✅ |
| 1.4-metadata-rest | Remaining plugins (adxl355, sd-card, battery-adc, led, nvs, clock, ble-beacon, ble-l2cap, data-tcp, reset-tcp, log-tcp) + remaining sentants (Accelerometer, WifiProv, Bootstrap, Beacon, Battery, Status, Sync, Recorder, Uplink, Ota, Reset, Health, Capture, Presence) — each gets plugin.toml/sentant.yaml + PLUGIN.md/SENTANT.md + AI-CONTEXT.md. Pattern proven by `sensor/lis2dh` + `sentants/Identity` (✅). | 1.4-metadata | ⏳ |
| 1.4-source | Extract the Rust source for each ensemble plugin from r2-workshop's inline firmware modules into standalone Cargo crates. The heavy lift — requires reading + refactoring ~12 source files. | 1.4-metadata-rest | ⏳ |
| 1.5 | `testing/round-trip/<carrier>.expected.toml` — recorded R2-WIRE traffic from a running r2-workshop firmware, captured as the conformance baseline | 1.3, 1.4 | ⏳ |
| 1.6 | `orchestrator/` scaffolding — axum WSS + static serve on port 21050; `catalogue`, `compiler`, `sync` plugins stubbed (each with a minimal sentant front routing `r2.compiler.*` events) | 1.2 | ⏳ |
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
