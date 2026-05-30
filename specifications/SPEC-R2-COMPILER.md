# SPEC-R2-COMPILER: r2-compiler as an R2 ensemble

**Version:** 0.1 Draft
**Date:** 2026-05-31
**Status:** Normative Draft
**Depends on:**
- **Upstream (canonical):** R2-ENSEMBLE, R2-COMPILE, R2-DEF, R2-PLUGIN, R2-BUILD, R2-CAP, R2-WIRE, R2-TRUST (all at `../r2-specifications/specs/r2-core/`)
- **r2-workshop:** `SPEC-R2-WORKSHOP-ENSEMBLE.md` (the B0 pattern this tool automates)
- **r2-compiler:** companion [`SPEC-CATALOGUE-LAYOUT.md`](SPEC-CATALOGUE-LAYOUT.md)

---

## 1. Introduction

This specification declares **what kind of thing r2-compiler is** in the canonical R2 vocabulary, and pins the identity of the tool — its class string, its hive composition, and the events it accepts and emits.

**r2-compiler is two things at once:**

1. **An R2 ensemble.** A two-hive composition (browser + workstation) that members of an operator's trust group join when they want to compose, build, and flash R2 firmware. Class string: `ai.reality2.ensemble.r2-compiler` (Reality2 namespace per `SPEC-R2-WORKSHOP-ENSEMBLE` §2.2; this is Reality2's own tooling, not a deployment fork).

2. **A meta-tool for R2-COMPILE.** It implements the B2 step of the R2-COMPILE roadmap (`r2-workshop/specifications/SPEC-R2-WORKSHOP-ENSEMBLE` §4): replace hand-coded per-carrier firmware crates with score-driven ones. The "compiler" is Claude Code; r2-compiler composes briefs, manages the catalogue, and orchestrates builds.

### 1.1 Scope

In scope:

- The ensemble identity (name, class, version) for the r2-compiler tool itself.
- The two-hive composition (webapp-hive + orchestrator-hive) and the R2 events they exchange.
- The sentants and plugins each hive performs.
- The event vocabulary for build orchestration (`r2.compiler.build.*`) and catalogue authoring (`r2.compiler.author.*`).
- The first-success behavioural-equivalence gate against r2-workshop's existing carriers.

Out of scope (defined elsewhere):

- The catalogue directory shape — see [`SPEC-CATALOGUE-LAYOUT.md`](SPEC-CATALOGUE-LAYOUT.md).
- The shape of the firmware crates emitted — see `R2-COMPILE.md` and `r2-workshop`'s per-carrier examples.
- The R2-WIRE / R2-TRUST / R2-ROUTE plumbing — inherited from upstream.

### 1.2 Terminology

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHOULD**, **MAY** are interpreted per RFC 2119.

Defined upstream:

- **Ensemble**, **Part**, **Performer**, **Score** — see R2-ENSEMBLE §1.4.
- **Hive**, **Sentant**, **Plugin**, **Class**, **Capability** — see R2-INTRO, R2-SENTANT, R2-PLUGIN, R2-CAP.
- **Carrier**, **AOT** — see R2-COMPILE §4.
- **Catalogue** — informal upstream; pinned in [`SPEC-CATALOGUE-LAYOUT.md`](SPEC-CATALOGUE-LAYOUT.md) §1.

Defined here:

- **Composition** — the operator-curated set of (board, plugins, sentants) currently on the visual canvas. Serialises to an R2-DEF §7 ensemble score.
- **Brief** — the prompt and supporting material the orchestrator hands to a `claude -p` subprocess to author or compile something.
- **Round-trip vector** — a test artefact that pins behavioural equivalence between r2-compiler output and an existing r2-workshop carrier (see §6).

---

## 2. Ensemble identity

| Field | Value |
|---|---|
| **name** | `r2-compiler` |
| **class** | `ai.reality2.ensemble.r2-compiler` |
| **class hash (FNV-1a-32)** | *to be computed by `r2-fnv` at first build* |
| **ensemble_version** | `0.1` (per R2-DEF §7 schema) |
| **version** | `0.1.0-draft` |

Namespace policy: `ai.reality2.ensemble.*` per `SPEC-R2-WORKSHOP-ENSEMBLE` §2.2 — this is Reality2's own tooling, NOT a deployment fork by a third party. A future user-specific fork (e.g. `nz.ac.auckland.r2-compiler`) is possible if a site needs site-bound catalogue policies.

---

## 3. Composition (two hives, one ensemble)

### 3.1 Hives

| Hive | Role | Tier | Notes |
|---|---|---|---|
| **webapp-hive** | Visual canvas, catalogue browser, source viewer, score-preview, build console | Tier 2 (browser via WASM) | Same shape as `r2-workshop/webapp/`'s `R2WorkshopHive`. Built from `crates/r2-wasm`. |
| **orchestrator-hive** | Catalogue host, score store, Claude Code invocation, cargo/esptool execution, optional flasher | Tier 2 (workstation, Linux/macOS/Windows) | Rust binary; serves the WASM bundle on the same port it listens on for R2-WIRE WSS (R2-WIRE §13.5 peek-protocol-detect). |

Both hives are **members of the operator's trust group**. The webapp obtains its TG cert via the QR/link enrolment flow inherited from r2-notekeeper / r2-workshop (`R2-PROVISION-UX`). The orchestrator is the KeyHolder for an `r2-compiler`-scoped subgroup.

### 3.2 Sentants (per hive)

**webapp-hive** SHALL perform at least the following sentants (R2-DEF §2):

| Sentant | Purpose | Compilable? |
|---|---|---|
| `Catalogue` | Reads boards / plugins / sentants from disk via the orchestrator; surfaces them as `r2.compiler.catalogue.entry` events for the UI to render. | Yes (R2-COMPILE §3) |
| `Composition` | Holds the canvas state — board choice + selected parts + their wiring. On every change, validates against R2-DEF §7 and emits `r2.compiler.composition.changed`. | Yes |
| `SourceViewer` | On `r2.compiler.source.request{path}`, fetches the source bytes from the orchestrator and emits `r2.compiler.source.delivered`. | Yes |
| `Builder` | On operator "Compile", emits `r2.compiler.build.start{score, target}` and consumes `r2.compiler.build.*` progress events. | Yes |
| `Author` | On operator "+ New Board/Plugin/Sentant", emits `r2.compiler.author.start{kind, description}` and surfaces the resulting agent dialog in the UI. | Yes |

**orchestrator-hive** SHALL perform at least the following sentants:

| Sentant | Purpose |
|---|---|
| `CatalogueServer` | Watches the `catalogue/` tree on disk; emits `r2.compiler.catalogue.entry` events on demand and on file-system change. |
| `Compiler` | Owns the build FSM (`idle → preparing → generating → compiling → done|error`). On `r2.compiler.build.start`, materialises the per-carrier crate, invokes `claude -p`, runs `cargo build`, streams progress. |
| `AuthorPilot` | Owns the catalogue-authoring FSM. On `r2.compiler.author.start`, opens a Claude Code session scoped to the target catalogue directory and forwards prompts/responses between the operator (via the UI) and the agent. |
| `Flasher` | On `r2.compiler.flash.start`, runs `esptool` against the built artefact (NEVER `espflash` — see R2-BUILD §5.1). Optional; not part of v0.1's success gate. |
| `Sync` | Pulls upstream `r2-core/plugins/` and `r2-workshop/ensemble/` into the local catalogue. Manual-invocation only in v0.1; CI-driven in later versions. |

### 3.3 Plugins (per hive)

Plugins are ensemble-owned (R2-ENSEMBLE §2.1.2) unless noted otherwise.

**webapp-hive:**

| Plugin | Purpose |
|---|---|
| `webapp-canvas` | DOM + drag-and-drop UX. UI plugin per R2-ENSEMBLE §2.1.1. |
| `webapp-source-view` | Rust syntax-highlighted read-only viewer. CodeMirror or shiki under the hood. UI plugin. |

**orchestrator-hive:**

| Plugin | Purpose |
|---|---|
| `claude-code` | Subprocess driver for `claude -p '<brief>' --output-format=stream-json`. Translates between R2 events and JSON-lines on stdin/stdout. Reuses existing local `claude` CLI auth. |
| `cargo-runner` | Shells out to `cargo build --target <triple> --release`. Parses output, surfaces diagnostics as events. |
| `git-runner` | Used by `Sync` for upstream pulls. |
| `webfetch` | Used by `AuthorPilot` to retrieve datasheets when authoring new catalogue entries. Maps to Claude Code's existing WebFetch tool. |
| **R2-WEB (hive-shared singleton)** | Serves the webapp bundle + the `/r2` WS endpoint. Same R2-WEB instance as the rest of the hive. |

### 3.4 Registrations with hive-shared singletons

The orchestrator-hive registers with R2-WEB (per R2-DEF §7.4) as follows (informative — exact payload subject to R2-WEB §3):

```yaml
registrations:
  r2-web:
    route_prefix: /r2-compiler
    static_bundle: ./webapp/dist/    # WASM + JS + HTML built by `wasm-pack` + bundler
    subscriptions:
      - name: r2
        target_sentant: Compiler     # or a routing sentant that demuxes
```

The default listener port is **`21050`** — distinct from r2-workshop's `21042` so the two tools can coexist on one workstation.

---

## 4. Event vocabulary

All events are FNV-1a-32 hashed (R2-FNV) for wire transport. The text names below are the canonical strings.

### 4.1 Catalogue events

| Event | Direction | Payload | Purpose |
|---|---|---|---|
| `r2.compiler.catalogue.list` | webapp → orchestrator | `{ kind: "board"\|"plugin"\|"sentant" }` | Request the catalogue contents for a kind. |
| `r2.compiler.catalogue.entry` | orchestrator → webapp | per-entry summary (name, path, version, tags, capabilities) | One emission per entry, plus a terminal `done: true`. |
| `r2.compiler.source.request` | webapp → orchestrator | `{ path }` | Request the source bytes for a file under `catalogue/`. |
| `r2.compiler.source.delivered` | orchestrator → webapp | `{ path, content_b64, mime }` | Source bytes for the viewer. |

### 4.2 Composition events (webapp-local; not crossing hives)

| Event | Payload |
|---|---|
| `r2.compiler.composition.changed` | the canvas state (board choice, parts, wiring) |
| `r2.compiler.composition.preview` | the R2-DEF §7 score serialised from the current state |

### 4.3 Build events

| Event | Direction | Payload | Purpose |
|---|---|---|---|
| `r2.compiler.build.start` | webapp → orchestrator | `{ score, target }` | Begin a build for the chosen carrier. |
| `r2.compiler.build.progress` | orchestrator → webapp | `{ phase, message, line?, percent? }` | Streaming progress (phase ∈ `preparing`, `generating`, `compiling`, `linking`, `verifying`). |
| `r2.compiler.build.done` | orchestrator → webapp | `{ artefact_path, sha256, size }` | Build succeeded; artefact ready. |
| `r2.compiler.build.error` | orchestrator → webapp | `{ phase, message, log_tail }` | Build failed; details for the console pane. |

### 4.4 Author events (catalogue authoring through dialog)

| Event | Direction | Payload | Purpose |
|---|---|---|---|
| `r2.compiler.author.start` | webapp → orchestrator | `{ kind, target_path?, description }` | Begin authoring a new catalogue entry. `kind ∈ "board" \| "plugin" \| "sentant"`. |
| `r2.compiler.author.prompt` | orchestrator → webapp | `{ text, requires_response: bool }` | Agent question or status update. |
| `r2.compiler.author.reply` | webapp → orchestrator | `{ text }` | Operator's free-text reply, forwarded to the agent. |
| `r2.compiler.author.file_added` | orchestrator → webapp | `{ path, reason }` | The agent created/modified a catalogue file. |
| `r2.compiler.author.done` | orchestrator → webapp | `{ entry_path, summary }` | Authoring complete; the entry is now in the catalogue. |
| `r2.compiler.author.error` | orchestrator → webapp | `{ message }` | Authoring aborted. |

### 4.5 Flash events (Phase 1.5)

| Event | Direction | Payload | Purpose |
|---|---|---|---|
| `r2.compiler.flash.start` | webapp → orchestrator | `{ artefact_path, port? }` | Flash an artefact via `esptool`. |
| `r2.compiler.flash.progress` | orchestrator → webapp | `{ phase, message }` | Streaming progress. |
| `r2.compiler.flash.done` | orchestrator → webapp | `{ port, duration_ms }` | Flash succeeded. |
| `r2.compiler.flash.error` | orchestrator → webapp | `{ message }` | Flash failed. |

---

## 5. The compile path (normative)

For each `r2.compiler.build.start` the orchestrator's `Compiler` sentant MUST:

1. **Validate the score** against R2-DEF §7.10 (`E_ENS_*` checks). On failure emit `r2.compiler.build.error{phase: "preparing", …}` and stop.
2. **Resolve the target** against the board catalogue: locate `catalogue/boards/<carrier>/board.toml`; verify the target triple matches the score's `compile_target`. On mismatch, stop with `phase: "preparing"`.
3. **Resolve plugins**: for each plugin on the canvas, locate `catalogue/plugins/<category>/<name>/plugin.toml`; verify `modes.aot` includes the carrier's compile target tag. On any miss, stop with `phase: "preparing"`.
4. **Resolve sentants**: for each sentant on the canvas, locate `catalogue/sentants/<Name>/sentant.yaml`; verify it passes R2-DEF §2 validation AND the R2-COMPILE §3.1 compilable-subset check. On failure, stop.
5. **Materialise the per-carrier crate** under `out/<carrier>-<timestamp>/` from `catalogue/boards/<carrier>/templates/`, substituting the score, the resolved plugin set, and the resolved sentant set into the templates.
6. **Spawn Claude Code**: `claude -p '<brief>' --output-format=stream-json` with:
   - working directory = `out/<carrier>-<timestamp>/`,
   - environment containing the catalogue root path,
   - the brief constructed from a Tera template under `orchestrator/prompts/compile.md`,
   - tools enabled: Read, Edit, Write, Bash (cargo only), Grep, Agent.
   Stream JSON parsed and re-emitted as `r2.compiler.build.progress` events.
7. **Run `cargo build --release --target <triple>`** via the `cargo-runner` plugin. Stream output as `r2.compiler.build.progress{phase: "compiling"}`. On non-zero exit, return to step 6 with an error brief once, then stop with `r2.compiler.build.error{phase: "compiling"}`.
8. **Verify the artefact** (R2-BUILD §4.4): `file` returns expected ELF arch; SHA-256 computed; record under `out/<carrier>-<timestamp>/releases/`.
9. **Emit `r2.compiler.build.done`** with the artefact path, SHA-256, and size.

A build that violates any step MUST emit `r2.compiler.build.error` and MUST NOT emit `r2.compiler.build.done`. No silent successes.

---

## 6. First success gate (binding)

The tool conforms to v0.1 when, for each of the three target carriers below, given the corresponding inputs, it produces a behaviourally-equivalent firmware to what r2-workshop currently ships:

| Carrier (id) | Target triple | Input score | r2-workshop reference dir |
|---|---|---|---|
| `esp32-s3-devkitc` | `xtensa-esp32s3-espidf` | `scores/rocker-sensor.yaml` (synced from `r2-workshop/ensemble/sensor.yaml`) | `../r2-workshop/firmware/esp32-s3/devkitc/` |
| `esp32-s3-xiao` | `xtensa-esp32s3-espidf` | same | `../r2-workshop/firmware/esp32-s3/xiao/` |
| `esp32-c6-dfr1117` | `riscv32imac-esp-espidf` | same | `../r2-workshop/firmware/esp32-c6/dfr1117/` |

**Behavioural equivalence** is checked against test vectors at [`testing/round-trip/<carrier>.expected.toml`](../testing/round-trip/) per R2-COMPILE §8 — same `r2.sensor.announce` payload, same R2-WIRE frame bytes for a recorded input sequence, same plugin set advertised in the R2-CAP bloom.

**Byte-identical binaries are NOT required** — build timestamps, embedded git SHAs, and the order of dynamic-init sections legitimately differ. Byte-identity is a stretch goal that can be revisited if reproducibility becomes a separate requirement (R2-BUILD §9.3).

---

## 7. Catalogue authoring (`r2.compiler.author.*` flow)

The authoring flow is the central feature of r2-compiler beyond static composition. Detailed in [`SPEC-CATALOGUE-LAYOUT.md`](SPEC-CATALOGUE-LAYOUT.md) §6. Summary of the normative obligations on the orchestrator's `AuthorPilot` sentant:

1. On `r2.compiler.author.start`, AuthorPilot SHALL open a fresh `claude -p` session with the catalogue directory as the working directory.
2. The agent SHALL ask clarifying questions through `r2.compiler.author.prompt` events; the operator's replies arrive via `r2.compiler.author.reply`.
3. The agent SHALL fetch any referenced datasheets via WebFetch and save them under the entry's `datasheets/` directory before referencing them.
4. The agent SHALL produce, at minimum, the five artefacts required by PROCESS.md §4 (canonical artefact, narrative markdown, AI-CONTEXT.md, datasheets, conversation transcript).
5. The agent SHALL validate the produced canonical artefact against its upstream spec (R2-PLUGIN §12.3 for `plugin.toml`, R2-DEF §2 for `sentant.yaml`, this spec §3.1 for `board.toml`) BEFORE emitting `r2.compiler.author.done`.
6. AuthorPilot SHALL emit `r2.compiler.author.error` if validation fails AND remove the partial entry directory — the catalogue MUST NOT contain half-authored entries.

---

## 8. Security

- TG private keys: KeyHolder for the `r2-compiler`-scoped subgroup lives off-tree at `~/.config/r2-compiler/tg_signer/` (mirrors r2-workshop's policy).
- Claude Code is invoked with `--dangerously-skip-permissions` ONLY if explicitly authorised by the operator in `~/.config/r2-compiler/orchestrator.toml`. Default is interactive (operator confirms each tool call). r2-compiler's UI MAY surface `claude` permission prompts as `r2.compiler.author.prompt` events for the operator to approve.
- The webapp bundle is served with the security headers mandated by R2-PLUGIN §13.9 (Content-Security-Policy, etc.).
- Datasheets fetched via WebFetch are saved to disk only — the agent MUST NOT execute or render fetched content beyond storing the bytes.

---

## 9. Conformance

A repository conforms to this spec when:

1. The two-hive composition of §3 is realised (browser WASM hive + workstation Rust hive).
2. The event vocabulary of §4 is honoured — events out of vocabulary are forbidden on the `r2.compiler.*` namespace.
3. The compile path of §5 is followed for every build, with no mocked successes.
4. The three round-trip vectors of §6 pass.
5. The catalogue authoring flow of §7 produces entries that conform to PROCESS.md §4.

---

## 10. Change log

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1 | Initial draft. Establishes the two-hive ensemble, the `r2.compiler.*` event vocabulary, the v0.1 success gate (round-trip the three r2-workshop carriers), and the catalogue authoring obligations. |
