# 2026-05-31 — r2-compiler design session 01

Verbatim user prompts, faithful AI responses summarised, decisions table at the end. PROCESS.md §2 convention.

## Context

The `/mnt/data/Development/R2/r2-compiler/` directory was empty before this session. Roy initiated work to build a tool that lets him select a carrier board (e.g. DFR1117 ESP32-C6), Reality2 Plugins, and Reality2 Sentants and compile firmware to flash. Spec-driven; visual drag-and-drop; developer-grade.

## Verbatim user messages (chronological)

1. > In this folder I want to build a useful tool for Reality2. To get up to speed, read the r2-specifications and r2-core folders / repos. Pay special attention to files designed for instructing AIs. Also, read the r2-workshop project. The task to commit to memory and into this folder is to be able to select a carrier board (eg DFR1117 ESP32-C6, Reality2 Plugins and Reality2 Sentants, and then compile firmware for it to flash to the appropriate board). I want to create a visual interface for this with a drag-and-drop capability with boards, plugins and sentants arrayed around the outside with a work space in the middle.

2. > This is a developer tool, so can be fairly technical. For example, it should be possible to open a Sentant or Plugin to see the code (in rust). Plugins and Sentants must conform to the Reality2 specifications

3. > The process of compilation and interaction will be like talking to Claude, and indeed, Claude code will be the tool at the backend doing the actual work, just as I have been doing with r2-workshop.

4. > the interface will be a web app (just as with r2-workshop) where the web server is a WASM r2-hive running in the browser window

5. > A first measure of success is to be able to compile the same boards, sentants and plugins as for r2-workshop

6. > A secondary feature, once the above is working, will be able to show the pin connections from the actual hardware graphically to simplify building the hardware

7. > In the same way as we have been doing with r2-workshop, it should be possible to create new plugins, boards and sentants as required through discussion with the AI agent. Each of these should result in an approriate file or set of files (including pdfs and other data pulled from the internet) that will include enough info for a fresh instant of claude code to be able to know what to do.

8. > So, we will need to structure folders for carrier boards, plugins and sentants both to hold the code as well as the reference documentation

## Decisions taken (with structured questions Roy confirmed)

| # | Decision | Rationale |
|---|---|---|
| D-1 | r2-compiler is an R2 ensemble — two hives, browser (WASM) + workstation (Rust orchestrator). Class `ai.reality2.ensemble.r2-compiler`. | Mirrors r2-workshop's webapp/dashboard split; same R2-WIRE/R2-TRUST plumbing. |
| D-2 | Compile backend = Claude Code via `claude -p '<brief>' --output-format=stream-json` subprocess. | Same workflow Roy already uses with r2-workshop manually, automated. |
| D-3 | Static webapp bundle served by the orchestrator from localhost (port 21050). Same peek-protocol-detect as r2-workshop's port 21042. | Single binary to run; no separate hosting needed for v0.1. |
| D-4 | First success metric = round-trip the three r2-workshop carriers (`devkitc`, `xiao`, `dfr1117`) behaviourally — same R2-WIRE traffic, not necessarily byte-identical binaries. | R2-COMPILE §8 conformance model. Pins success to an observable outcome. |
| D-5 | Catalogue is standalone but synced from `r2-core/plugins/` and `r2-workshop/ensemble/` — vendored, like Cargo crates. `tools/sync-catalogue.sh` is the only path that touches `crates/`. | Self-contained build (r2-workshop AI-CONTEXT.md pattern); audit gate on upstream change. |
| D-6 | Catalogue is authorable through dialog with the agent. Each new board/plugin/sentant produces a self-contained directory with `AI-CONTEXT.md` so a fresh CC session can resume. | Roy's explicit requirement: r2-workshop's working pattern, scaled out. |
| D-7 | Each catalogue entry's required artefacts = canonical artefact (`plugin.toml`/`sentant.yaml`/`board.toml`) + narrative `*.md` + `AI-CONTEXT.md` + `datasheets/` + `conversation/`. Five-element conformance gate, no exceptions. | PROCESS.md §4. Half-authored entries are bugs not work-in-progress. |
| D-8 | Plugin folder nested by category mirroring `r2-core/plugins/` (`sensor/`, `crypto/`, `display/`, …). | One-to-one sync; matches R2-PLUGIN §12.2's category list. |
| D-9 | Catalogue lives under `catalogue/` (not at top level). Grouped for discoverability. | Three branches `boards/`, `plugins/`, `sentants/`. |
| D-10 | Build skeleton + meta-specs first, then real code. | Spec-first per AGENTS.md §3. |
| D-11 | Pin-visualisation feature deferred to Phase 4. | Roy stated it as secondary. |

## What was produced in this session

All scaffolding files described in [`../plan/PLAN.md`](../plan/PLAN.md) "Status (2026-05-31)". No Rust yet.

Memory entries written under `/home/roycdavies/.claude/projects/-mnt-data-Development-R2-r2-compiler/memory/`:
- `project_r2_compiler_purpose.md`
- `reference_r2_neighbour_repos.md`
- `feedback_conformance_gate.md`
- `reference_carrier_firmware_pattern.md`
- `project_phase2_pin_visualisation.md`
- `project_catalogue_authoring.md`
- `user_profile.md`

## Next session

Per `plan/PLAN.md` Phase 1: write `tools/sync-catalogue.sh`, populate the three boards' template trees + the two upstream dual-mode plugins + the 15 sensor sentants from r2-workshop, then start the orchestrator scaffolding.

## Open conjectures (carry forward)

C-1 through C-4 — see PLAN.md "Open conjectures". Each must be either survived or refuted explicitly during Phase 1, not silently assumed.
