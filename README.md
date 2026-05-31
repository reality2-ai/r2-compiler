# r2-composer

**A visual composer for Reality2 firmware.** Pick a carrier board, a set of plugins, and a set of sentants from the catalogue around the canvas; the tool drives a Claude Code session to produce a flashable per-carrier firmware image.

Status: design phase (2026-05-31). The spec at [`specifications/SPEC-R2-COMPOSER.md`](specifications/SPEC-R2-COMPOSER.md) is the contract; everything else is in flux.

## What this is

r2-composer is the **B2 step** of the R2-COMPILE roadmap (per `r2-workshop/specifications/SPEC-R2-WORKSHOP-ENSEMBLE.md` §4). Today, each carrier under `r2-workshop/firmware/<arch>/<carrier>/` is hand-authored. This tool replaces that loop:

```
visual canvas (browser hive)
    │ R2-WIRE event: r2.composer.build.start
    ▼
orchestrator hive (workstation, Rust)
    │ subprocess: claude -p '<brief>' --output-format=stream-json
    ▼
per-carrier crate generated under out/<carrier>/
    │ cargo build --target <triple> --release
    ▼
flashable .bin
```

The visual layer is a browser-side WASM R2 hive (same architecture as `r2-workshop/webapp/`). The compile work is delegated to Claude Code on the operator's workstation — exactly the workflow we use to author r2-workshop sessions today, but driven by the visual composition instead of free-form chat.

## What this is NOT

- **Not a from-scratch Rust compiler.** The compiler is `cargo` + `esptool`. The intelligence sitting between "operator's composition" and "buildable crate" is Claude Code, not a code generator.
- **Not a consumer GUI.** Developer-grade: every Plugin and Sentant in the catalogue is openable in a Rust source viewer. No hidden magic.
- **Not a replacement for r2-workshop.** It produces the same kind of per-carrier crate r2-workshop ships today. First success metric is round-tripping r2-workshop's existing `devkitc`, `xiao`, and `dfr1117` builds.

## Repo layout

| Path | Purpose |
|---|---|
| [`AGENTS.md`](AGENTS.md) | Orientation for AI agents (Claude Code, Codex, …) — **read first** |
| [`AI-CONTEXT.md`](AI-CONTEXT.md) | Fresh-session entry point for any AI assistant resuming work |
| [`PROCESS.md`](PROCESS.md) | The five working rules (spec-first, conversation-as-data, …) |
| [`specifications/`](specifications/) | Normative specs for this tool (SPEC-R2-COMPOSER, SPEC-CATALOGUE-LAYOUT) |
| [`catalogue/boards/`](catalogue/boards/) | One directory per carrier — `board.toml` + datasheets + templates + AI-CONTEXT. The canvas's first opt-in part type. |
| [`catalogue/ensembles/`](catalogue/ensembles/) | One directory per ensemble (R2-ENSEMBLE / R2-DEF §7). Plugins + sentants live **inside** their ensemble. The canvas's second opt-in part type. |
| [`scores/`](scores/) | Complete R2-DEF §7 ensembles (the visual composer's serialised output for a specific build) |
| [`crates/`](crates/) | R2 protocol crates vendored from `r2-core/`, kept in sync |
| [`orchestrator/`](orchestrator/) | Rust binary — the workstation R2 hive; spawns Claude Code per build |
| [`webapp/`](webapp/) | Browser-side WASM R2 hive + drag-and-drop UI |
| [`testing/round-trip/`](testing/round-trip/) | Vectors proving the tool reproduces r2-workshop's existing carriers |
| [`conversation/`](conversation/) | Per-session transcripts (r2-workshop convention) |
| [`plan/PLAN.md`](plan/PLAN.md) | Current phasing |

## Canonical R2 specifications (upstream contracts)

Every artefact this tool produces must conform to:

- `r2-specifications/specs/r2-core/R2-PLUGIN.md` — esp. §12 (Dual-Mode Rust Plugin Authoring)
- `r2-specifications/specs/r2-core/R2-DEF.md` — esp. §2 (sentant schema) and §7 (ensemble score schema)
- `r2-specifications/specs/r2-core/R2-COMPILE.md` — esp. §3 (compilable subset) and §4 (compilation targets)
- `r2-specifications/specs/r2-core/R2-ENSEMBLE.md` — composition model
- `r2-specifications/specs/r2-core/R2-BUILD.md` — toolchains and target triples

If a spec is silent or ambiguous, that's a spec bug — raise it against `r2-specifications`, do not paper over it in this repo.

## Quick start

Not yet — the orchestrator binary and webapp bundle don't exist yet. Watch [`plan/PLAN.md`](plan/PLAN.md) for milestone status.
