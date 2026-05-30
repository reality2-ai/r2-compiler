# AGENTS.md — Orientation for AI Agents working in `r2-compiler`

This file is the entry point for AI agents (Claude Code, Codex, Cursor, …) operating in this repository. Read this first, then [`AI-CONTEXT.md`](AI-CONTEXT.md), then the spec at [`specifications/SPEC-R2-COMPILER.md`](specifications/SPEC-R2-COMPILER.md).

> **One-paragraph orientation:** r2-compiler is a visual composer for R2 firmware that drives Claude Code as its compile backend. The operator picks a carrier board + plugins + sentants from a catalogue around a canvas; the tool serialises that composition into an R2-DEF §7 ensemble score and dispatches a Claude Code session that produces a per-carrier firmware crate. The catalogue is **authorable through dialog with the agent** — new boards, plugins, sentants are created by the same conversational pattern Roy uses with r2-workshop today, and every entry leaves behind enough material (datasheets, AI-CONTEXT.md, conversation transcripts) that a fresh CC instance can pick the entry up cold. Everything in this repo must conform to the canonical R2 specs at `../r2-specifications/`.

## 1. The upstream contracts you are bound by

This repo does **not** invent its own plugin / sentant / ensemble model. It implements + automates the model defined upstream:

| Concept | Authoritative spec |
|---|---|
| Plugin | `../r2-specifications/specs/r2-core/R2-PLUGIN.md` — esp. §12 (Dual-Mode Rust Plugin Authoring), §12.3 (`plugin.toml`), §12.4 (`r2_engine::Plugin` trait), §12.8 (mandatory README) |
| Sentant | `../r2-specifications/specs/r2-core/R2-DEF.md` §2 + `R2-SENTANT.md` |
| Ensemble (the score) | `../r2-specifications/specs/r2-core/R2-DEF.md` §7 + `R2-ENSEMBLE.md` |
| AOT compilation | `../r2-specifications/specs/r2-core/R2-COMPILE.md` — esp. §3 (compilable subset), §4 (targets), §5 (compiled plugins) |
| Cross-platform build | `../r2-specifications/specs/r2-core/R2-BUILD.md` — esp. §2 (target triples) |
| Trust / signing | `../r2-specifications/specs/r2-core/R2-TRUST.md` |
| Hardware tiers | `../r2-specifications/specs/r2-core/R2-HW.md` and `R2-HW-REF.md` |

If your work touches any of these areas, **read the relevant section first** and cite it. The five-line section-quote in a commit message is much better than an unattributed claim.

## 2. The two working principles (carried over from r2-specifications/AGENTS.md)

This repo inherits the discipline of `r2-specifications/AGENTS.md`. The short form:

- **Conjecture-and-refutation.** Treat every implementation decision as a conjecture on trial. Actively try to refute it. "I couldn't find anything against this" is neutral, not positive.
- **Occam's razor.** Simplest implementation that meets the requirement wins. Complexity earns its place by demonstrating it solves a problem simpler alternatives cannot.
- **Disagree with the operator when they are wrong.** Politely. Confirming a wrong claim is worse than presenting the contradiction.
- **Citation discipline.** Don't fabricate file paths, line numbers, datasheet URLs, or spec section numbers. Read it / grep it / fetch it before citing.
- **Cheaper honest move.** When tempted to overclaim, downgrade instead. "Found supporting evidence but did not actively try to refute" beats "verified."
- **Autonomy stop.** When about to take a hard-to-reverse action (force-push, rm -rf, dropping a catalogue entry), STOP and surface the question.

The full treatment lives at `../r2-specifications/AGENTS.md` §2.

## 3. The catalogue is authorable — that's the central feature

The boards / plugins / sentants in [`catalogue/`](catalogue/) are NOT a fixed inventory. They grow through dialog with you (the agent). The operator says "add a new sensor plugin for the BME280 over I²C" — you ask the right clarifying questions, fetch the datasheet, scaffold the directory per §4 below, write source that conforms to R2-PLUGIN §12, and leave the entry in a state where the NEXT fresh CC session can pick it up.

Concretely, when adding any catalogue entry, you MUST produce:

1. The **canonical artefact** — `plugin.toml` (R2-PLUGIN §12.3) for plugins, `sentant.yaml` (R2-DEF §2) for sentants, `board.toml` for boards.
2. A **narrative `*.md`** — `PLUGIN.md` (R2-PLUGIN §12.8 mandates 10 sections) / `SENTANT.md` / `BOARD.md`.
3. An **`AI-CONTEXT.md`** specific to that entry — what it is, which specs it conforms to, where the canonical artefact lives, hardware/vendor refs, gotchas, read-in-order list.
4. **Reference material** — datasheets under `datasheets/` (fetched, saved, not just linked).
5. A **`conversation/YYYY-MM-DD-<topic>-NN.md`** transcript of the session that created it.

A catalogue entry without all five is incomplete; do not announce it as done.

## 4. The catalogue entry shape (normative)

**Two-part canvas model:** the canvas exposes only (a) carrier boards and (b) ensembles. Plugins and sentants are NOT separate catalogue trees — they live **inside** an ensemble (ensemble-owned per R2-ENSEMBLE §2.1.2) or **inside** a carrier (for hive-shared singletons like radios).

Boards:
```
catalogue/boards/<arch>-<chip>-<carrier>/
  board.toml                  # target triple, chip, flash/PSRAM, GPIO map, sdkconfig profile
  BOARD.md                    # narrative
  AI-CONTEXT.md               # fresh-CC brief for this carrier
  pinout.svg                  # for Phase 4 hardware-wiring view (deferred)
  plugins/                    # OPTIONAL — hive-shared singletons for this carrier
    <category>/<name>/        # e.g. comms/ble-radio, comms/wifi-radio
  templates/                  # Cargo.toml.tera, sdkconfig.defaults, partitions.csv, .cargo/config.toml
  datasheets/                 # PDFs / schematic PNGs
  conversation/               # transcripts
```

Ensembles (sentants + ensemble-owned plugins nested inside):
```
catalogue/ensembles/<name>/
  ensemble.yaml               # R2-DEF §7 — the canonical artefact
  ENSEMBLE.md                 # narrative + composition diagram
  AI-CONTEXT.md
  plugins/                    # ensemble-owned plugins (R2-ENSEMBLE §2.1.2)
    <category>/<name>/
      plugin.toml             # R2-PLUGIN §12.3
      PLUGIN.md               # R2-PLUGIN §12.8 — all 10 sections mandatory
      Cargo.toml              # mutually-exclusive features: aot, nif
      AI-CONTEXT.md
      src/                    # lib.rs / plugin.rs / driver.rs
      datasheets/
      tests/
      conversation/
  sentants/
    <Name>/
      sentant.yaml            # R2-DEF §2
      SENTANT.md              # narrative + FSM diagram
      AI-CONTEXT.md
      conversation/
  datasheets/
  conversation/
```

Always-available R2 stack and core plugins (crypto, etc.) live under [`crates/`](crates/), not in the catalogue.

The normative spec is at [`specifications/SPEC-CATALOGUE-LAYOUT.md`](specifications/SPEC-CATALOGUE-LAYOUT.md). When that and this disagree, the spec wins and this is a bug.

## 5. The compile path

For each build the operator triggers from the webapp:

1. Webapp serialises canvas state → R2-DEF §7 ensemble score under `scores/<name>-<timestamp>.yaml`.
2. Webapp emits `r2.compiler.build.start { score, target }` over R2-WIRE to the orchestrator hive.
3. Orchestrator's `Compiler` sentant materialises a per-carrier crate under `out/<carrier>/` from `catalogue/boards/<board>/templates/`.
4. Orchestrator spawns `claude -p '<brief>' --output-format=stream-json` with the catalogue root as working directory. The brief specifies: the score, the carrier, the plugins to link, the success criteria (e.g. `cargo build --release --target <triple>` returns 0).
5. Claude Code does the work — authors the per-carrier `main.rs`, wires plugin glue, runs cargo, debugs failures by reading datasheets + similar existing crates in `catalogue/`.
6. Orchestrator streams `r2.compiler.build.progress` events back to the webapp for display.
7. Output: a flashable `.bin` under `out/<carrier>/releases/` + optional `esptool` flash via a second sentant.

If you ARE the Claude Code session inside step 4–5: read the brief, the score, the board's `AI-CONTEXT.md`, then the relevant `plugin.toml` files and `sentant.yaml` files for each part on the canvas. Cite the spec sections you rely on. Don't invent — when something is missing from the catalogue, ask via a `r2.compiler.brief.question` event back to the orchestrator rather than guessing.

## 6. The first success gate (binding)

The tool must round-trip the three existing r2-workshop carriers from `r2-workshop/ensemble/sensor.yaml`:

| Carrier | Target triple | Reference dir |
|---|---|---|
| `esp32-s3-devkitc` | `xtensa-esp32s3-espidf` | `../r2-workshop/firmware/esp32-s3/devkitc/` |
| `esp32-s3-xiao` | `xtensa-esp32s3-espidf` | `../r2-workshop/firmware/esp32-s3/xiao/` |
| `esp32-c6-dfr1117` | `riscv32imac-esp-espidf` | `../r2-workshop/firmware/esp32-c6/dfr1117/` |

Behavioural equivalence (per R2-COMPILE §8) is the gate — same `r2.sensor.announce` payload, same R2-WIRE traffic against the test vectors at [`testing/round-trip/`](testing/round-trip/). Byte-identical binaries are not required (build timestamps differ).

## 7. Things you will probably miss first time

- **Two repo paths.** `/mnt/data/Development/R2/r2-compiler` and `/home/roycdavies/Development/R2/r2-compiler` resolve to the same inode (one is a symlink). Don't be confused.
- **The orchestrator IS a hive**, not a plain HTTP server. Same R2-WIRE / R2-TRUST membership story as r2-workshop's `dashboard/` binary. It happens to also serve static files for the webapp bundle on the same port (peek-based protocol detection per R2-WIRE §13.5).
- **The webapp IS a hive too**, running in the browser via WASM. Look at `r2-workshop/webapp/` for the working pattern.
- **The catalogue is self-contained.** Don't add `path = "../r2-core"` dependencies. Vendor what you need into [`crates/`](crates/), kept in sync via `tools/sync-catalogue.sh`.
- **Compile target tags are not free-form.** They come from R2-COMPILE §4 and R2-DEF §7.7: `esp32-s3`, `esp32-c6`, `nrf52`, `linux`, etc. The board catalogue maps a carrier slug to the relevant tag.
- **`plugin.toml` is the contract** — `PLUGIN.md`, `README.md`, and code that disagree with `plugin.toml` are bugs in the disagreeing artefact, not in `plugin.toml`. Per R2-PLUGIN §12.9.

## 8. Workflow rules (must not break)

1. **Spec before code.** Touching a new behaviour without a driving spec in [`specifications/`](specifications/) (or upstream in `r2-specifications/`)? Stop and write the spec first.
2. **Conversation is research data.** Every working session appends a new file `conversation/YYYY-MM-DD-<topic>-NN.md` — verbatim user, faithful AI, decisions table at the end. Never edit a closed session retroactively. Same rule applies per-entry: a board / plugin / sentant authoring session lives in that entry's `conversation/`.
3. **Catalogue conformance gate.** A plugin without a valid `plugin.toml` is not a plugin. A sentant whose `sentant.yaml` fails R2-DEF §2 validation is not a sentant. Block additions that don't conform.
4. **No secrets.** No TG private keys, WiFi credentials, API tokens, or device UUIDs in this repo. `.gitignore` blocks the patterns; don't put them there in the first place.
5. **No mocking of compile output.** If the tool claims a build succeeded, it must have called `cargo build` and observed exit 0 — not synthesised a "looks-good" result.

## 9. When you don't know

- Read `../r2-specifications/AGENTS.md` for the broader R2-project discipline.
- Read `../r2-workshop/AI-CONTEXT.md` for the working pattern this tool automates.
- Look at `../r2-core/plugins/crypto/software-ed25519/` for a complete dual-mode plugin worked example.
- Look at `../r2-workshop/ensemble/sensor.yaml` for a complete ensemble score worked example.
- Look at `../r2-workshop/firmware/esp32-c6/dfr1117/` for a complete per-carrier firmware crate worked example.
- If the spec is genuinely silent, raise a sharper question against `r2-specifications` — do not invent.

---

*AGENTS.md — orientation for AI agents working in r2-compiler.*
