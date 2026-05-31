# SPEC-R2-COMPOSER: r2-composer as an R2 ensemble

**Version:** 0.1 Draft
**Date:** 2026-05-31
**Status:** Normative Draft
**Depends on:**
- **Upstream (canonical):** R2-ENSEMBLE, R2-COMPILE, R2-DEF, R2-PLUGIN, R2-BUILD, R2-CAP, R2-WIRE, R2-TRUST (all at `../r2-specifications/specs/r2-core/`)
- **r2-workshop:** `SPEC-R2-WORKSHOP-ENSEMBLE.md` (the B0 pattern this tool automates)
- **r2-composer:** companion [`SPEC-CATALOGUE-LAYOUT.md`](SPEC-CATALOGUE-LAYOUT.md)

---

## 1. Introduction

This specification declares **what kind of thing r2-composer is** in the canonical R2 vocabulary, and pins the identity of the tool — its class string, its hive composition, and the events it accepts and emits.

**r2-composer is two things at once:**

1. **An R2 ensemble.** A two-hive composition (browser + workstation) that members of an operator's trust group join when they want to compose, build, and flash R2 firmware. Class string: `ai.reality2.ensemble.r2-composer` (Reality2 namespace per `SPEC-R2-WORKSHOP-ENSEMBLE` §2.2; this is Reality2's own tooling, not a deployment fork).

2. **A meta-tool for R2-COMPILE.** It implements the B2 step of the R2-COMPILE roadmap (`r2-workshop/specifications/SPEC-R2-WORKSHOP-ENSEMBLE` §4): replace hand-coded per-carrier firmware crates with score-driven ones. The "compiler" is Claude Code; r2-composer composes briefs, manages the catalogue, and orchestrates builds.

### 1.1 Scope

In scope:

- The ensemble identity (name, class, version) for the r2-composer tool itself.
- The two-hive composition (webapp-hive + orchestrator-hive) and the R2 events they exchange.
- The sentants and plugins each hive performs.
- The event vocabulary for build orchestration (`r2.composer.build.*`) and catalogue authoring (`r2.composer.author.*`).
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
- **Round-trip vector** — a test artefact that pins behavioural equivalence between r2-composer output and an existing r2-workshop carrier (see §6).

---

## 2. Ensemble identity

| Field | Value |
|---|---|
| **name** | `r2-composer` |
| **class** | `ai.reality2.ensemble.r2-composer` |
| **class hash (FNV-1a-32)** | *to be computed by `r2-fnv` at first build* |
| **ensemble_version** | `0.1` (per R2-DEF §7 schema) |
| **version** | `0.1.0-draft` |

Namespace policy: `ai.reality2.ensemble.*` per `SPEC-R2-WORKSHOP-ENSEMBLE` §2.2 — this is Reality2's own tooling, NOT a deployment fork by a third party. A future user-specific fork (e.g. `nz.ac.auckland.r2-composer`) is possible if a site needs site-bound catalogue policies.

---

## 3. Composition (two hives, one ensemble)

### 3.1 Hives

| Hive | Role | Tier | Notes |
|---|---|---|---|
| **webapp-hive** | Visual canvas, catalogue browser, source viewer, score-preview, build console | Tier 2 (browser via WASM) | Same shape as `r2-workshop/webapp/`'s `R2WorkshopHive`. Built from `crates/r2-wasm`. |
| **orchestrator-hive** | Catalogue host, score store, Claude Code invocation, cargo/esptool execution, optional flasher | Tier 2 (workstation, Linux/macOS/Windows) | Rust binary; serves the WASM bundle on the same port it listens on for R2-WIRE WSS (R2-WIRE §13.5 peek-protocol-detect). |

Both hives are **members of the operator's trust group**. The webapp obtains its TG cert via the QR/link enrolment flow inherited from r2-notekeeper / r2-workshop (`R2-PROVISION-UX`). The orchestrator is the KeyHolder for an `r2-composer`-scoped subgroup.

### 3.2 Sentants (per hive)

**webapp-hive** SHALL perform at least the following sentants (R2-DEF §2):

| Sentant | Purpose | Compilable? |
|---|---|---|
| `Catalogue` | Reads boards / plugins / sentants from disk via the orchestrator; surfaces them as `r2.composer.catalogue.entry` events for the UI to render. | Yes (R2-COMPILE §3) |
| `Composition` | Holds the canvas state — board choice + selected parts + their wiring. On every change, validates against R2-DEF §7 and emits `r2.composer.composition.changed`. | Yes |
| `SourceViewer` | On `r2.composer.source.request{path}`, fetches the source bytes from the orchestrator and emits `r2.composer.source.delivered`. | Yes |
| `Builder` | On operator "Compile", emits `r2.composer.build.start{score, target}` and consumes `r2.composer.build.*` progress events. | Yes |
| `Author` | On operator "+ New Board/Plugin/Sentant", emits `r2.composer.author.start{kind, description}` and surfaces the resulting agent dialog in the UI. | Yes |

**orchestrator-hive** SHALL perform at least the following sentants. These are thin event-routers; **the actual work happens in plugins** (§3.3) — sentants here are FSMs that receive events, dispatch to a plugin, and emit result events. See `[[feedback-sentants-vs-plugins-terminology]]` in memory for the discipline.

| Sentant | Purpose |
|---|---|
| `Catalogue` | Routes `r2.composer.catalogue.*` events; tracks loaded entries. The actual filesystem watching is the `catalogue` plugin (§3.3). |
| `Builder` | Owns the per-build FSM (`idle → preparing → generating → compiling → done|error`). On `r2.composer.build.start`, dispatches to the `compiler` plugin and emits progress events as the plugin reports them. |
| `Author` | Owns the catalogue-authoring session FSM. On `r2.composer.author.start`, dispatches to the `claude-code` plugin scoped to the target catalogue directory; routes prompts/replies between the plugin and the UI. |
| `Deploy` | On `r2.composer.deploy.start`, dispatches to the `flasher` plugin (USB) or the OTA-push variant of the `claude-code` / direct-TCP path. Tracks per-device deploy state per §12. |
| `Sync` | On `r2.composer.sync.start`, dispatches to the `sync` plugin (which wraps `tools/sync-catalogue.sh`). |
| `Tg` | Routes `r2.composer.tg.*` events; dispatches to the `keyholder` plugin for issuing/revoking certs per §11. |

### 3.3 Plugins (per hive)

**The plugins do the actual work.** Per R2-PLUGIN §1, plugins are "anything that runs on a hive and provides capabilities" — subprocess management, network I/O, file I/O, hardware drivers. They're invoked by the sentants in §3.2.

**webapp-hive:**

| Plugin | Purpose |
|---|---|
| `webapp-canvas` | DOM + drag-and-drop UX. UI plugin per R2-ENSEMBLE §2.1.1. |
| `webapp-source-view` | Rust syntax-highlighted read-only viewer. CodeMirror or shiki under the hood. UI plugin. |

**orchestrator-hive:**

| Plugin | Purpose |
|---|---|
| `compiler` | Materialises the per-carrier crate from the score, invokes `claude -p` (via the `claude-code` plugin) to fill in generated code, runs `cargo build`, parses output, returns the artefact path. The plugin that the `Builder` sentant calls. |
| `claude-code` | Subprocess driver for `claude -p '<brief>' --output-format=stream-json`. Translates between R2 events and JSON-lines on stdin/stdout. Reuses existing local `claude` CLI auth. Used by both `compiler` (for build code-gen) and `Author` sentant (for catalogue authoring). |
| `cargo-runner` | Shells out to `cargo build --target <triple> --release`. Parses output, surfaces diagnostics. Used by `compiler`. |
| `flasher` | Runs `esptool` (NEVER `espflash` — see R2-BUILD §5.1) to write firmware to a USB-connected device. Used by `Deploy` sentant. |
| `ota-push` | TCP push to a device's OTA receiver on port 21043 (R2-DEPLOY). Used by `Deploy` sentant. |
| `webfetch` | Retrieves datasheets / vendor docs from the web during catalogue authoring. Used by the `Author` sentant via `claude-code`. Maps to Claude Code's existing WebFetch tool. |
| `git-runner` | Subprocess wrapper around `git`. Used by `sync`. |
| `sync` | Wraps `tools/sync-catalogue.sh`. Vendors upstream crates and refreshes catalogue templates. Used by `Sync` sentant. |
| `catalogue` | Watches the `catalogue/` tree on disk and serves entry-listing queries. Used by `Catalogue` sentant. |
| `keyholder` | Holds the TG private key, issues `DeviceCertificate`s, manages revocation lists. Used by `Tg` sentant per §11. KeyHolder material lives off-tree per §8. |
| **R2-WEB (hive-shared singleton)** | Serves the webapp bundle + the `/r2` WS endpoint. Same R2-WEB instance as the rest of the hive. |

### 3.4 Registrations with hive-shared singletons

The orchestrator-hive registers with R2-WEB (per R2-DEF §7.4) as follows (informative — exact payload subject to R2-WEB §3):

```yaml
registrations:
  r2-web:
    route_prefix: /r2-composer
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
| `r2.composer.catalogue.list` | webapp → orchestrator | `{ kind: "board"\|"plugin"\|"sentant" }` | Request the catalogue contents for a kind. |
| `r2.composer.catalogue.entry` | orchestrator → webapp | per-entry summary (name, path, version, tags, capabilities) | One emission per entry, plus a terminal `done: true`. |
| `r2.composer.source.request` | webapp → orchestrator | `{ path }` | Request the source bytes for a file under `catalogue/`. |
| `r2.composer.source.delivered` | orchestrator → webapp | `{ path, content_b64, mime }` | Source bytes for the viewer. |

### 4.2 Composition events (webapp-local; not crossing hives)

| Event | Payload |
|---|---|
| `r2.composer.composition.changed` | the canvas state (board choice, parts, wiring) |
| `r2.composer.composition.preview` | the R2-DEF §7 score serialised from the current state |

### 4.3 Build events

| Event | Direction | Payload | Purpose |
|---|---|---|---|
| `r2.composer.build.start` | webapp → orchestrator | `{ score, target }` | Begin a build for the chosen carrier. |
| `r2.composer.build.progress` | orchestrator → webapp | `{ phase, message, line?, percent? }` | Streaming progress (phase ∈ `preparing`, `generating`, `compiling`, `linking`, `verifying`). |
| `r2.composer.build.done` | orchestrator → webapp | `{ artefact_path, sha256, size }` | Build succeeded; artefact ready. |
| `r2.composer.build.error` | orchestrator → webapp | `{ phase, message, log_tail }` | Build failed; details for the console pane. |

### 4.4 Author events (catalogue authoring through dialog)

| Event | Direction | Payload | Purpose |
|---|---|---|---|
| `r2.composer.author.start` | webapp → orchestrator | `{ kind, target_path?, description }` | Begin authoring a new catalogue entry. `kind ∈ "board" \| "plugin" \| "sentant"`. |
| `r2.composer.author.prompt` | orchestrator → webapp | `{ text, requires_response: bool }` | Agent question or status update. |
| `r2.composer.author.reply` | webapp → orchestrator | `{ text }` | Operator's free-text reply, forwarded to the agent. |
| `r2.composer.author.file_added` | orchestrator → webapp | `{ path, reason }` | The agent created/modified a catalogue file. |
| `r2.composer.author.done` | orchestrator → webapp | `{ entry_path, summary }` | Authoring complete; the entry is now in the catalogue. |
| `r2.composer.author.error` | orchestrator → webapp | `{ message }` | Authoring aborted. |

### 4.5 Flash events (Phase 1.5)

| Event | Direction | Payload | Purpose |
|---|---|---|---|
| `r2.composer.flash.start` | webapp → orchestrator | `{ artefact_path, port? }` | Flash an artefact via `esptool`. |
| `r2.composer.flash.progress` | orchestrator → webapp | `{ phase, message }` | Streaming progress. |
| `r2.composer.flash.done` | orchestrator → webapp | `{ port, duration_ms }` | Flash succeeded. |
| `r2.composer.flash.error` | orchestrator → webapp | `{ message }` | Flash failed. |

---

## 5. The compile path (normative)

For each `r2.composer.build.start` the orchestrator's `Compiler` sentant MUST:

1. **Validate the score** against R2-DEF §7.10 (`E_ENS_*` checks). On failure emit `r2.composer.build.error{phase: "preparing", …}` and stop.
2. **Resolve the target** against the board catalogue: locate `catalogue/boards/<carrier>/board.toml`; verify the target triple matches the score's `compile_target`. On mismatch, stop with `phase: "preparing"`.
3. **Resolve plugins**: for each plugin on the canvas, locate `catalogue/plugins/<category>/<name>/plugin.toml`; verify `modes.aot` includes the carrier's compile target tag. On any miss, stop with `phase: "preparing"`.
4. **Resolve sentants**: for each sentant on the canvas, locate `catalogue/sentants/<Name>/sentant.yaml`; verify it passes R2-DEF §2 validation AND the R2-COMPILE §3.1 compilable-subset check. On failure, stop.
5. **Materialise the per-carrier crate** under `out/<carrier>-<timestamp>/` from `catalogue/boards/<carrier>/templates/`, substituting the score, the resolved plugin set, and the resolved sentant set into the templates.
6. **Spawn Claude Code**: `claude -p '<brief>' --output-format=stream-json` with:
   - working directory = `out/<carrier>-<timestamp>/`,
   - environment containing the catalogue root path,
   - the brief constructed from a Tera template under `orchestrator/prompts/compile.md`,
   - tools enabled: Read, Edit, Write, Bash (cargo only), Grep, Agent.
   Stream JSON parsed and re-emitted as `r2.composer.build.progress` events.
7. **Run `cargo build --release --target <triple>`** via the `cargo-runner` plugin. Stream output as `r2.composer.build.progress{phase: "compiling"}`. On non-zero exit, return to step 6 with an error brief once, then stop with `r2.composer.build.error{phase: "compiling"}`.
8. **Verify the artefact** (R2-BUILD §4.4): `file` returns expected ELF arch; SHA-256 computed; record under `out/<carrier>-<timestamp>/releases/`.
9. **Emit `r2.composer.build.done`** with the artefact path, SHA-256, and size.

A build that violates any step MUST emit `r2.composer.build.error` and MUST NOT emit `r2.composer.build.done`. No silent successes.

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

## 7. Catalogue authoring (`r2.composer.author.*` flow)

The authoring flow is the central feature of r2-composer beyond static composition. Detailed in [`SPEC-CATALOGUE-LAYOUT.md`](SPEC-CATALOGUE-LAYOUT.md) §6. Summary of the normative obligations on the orchestrator's `Author` sentant + the `claude-code` plugin it dispatches to:

1. On `r2.composer.author.start`, the Author sentant SHALL dispatch to the `claude-code` plugin to open a fresh `claude -p` session with the catalogue directory as the working directory.
2. The agent SHALL ask clarifying questions through `r2.composer.author.prompt` events; the operator's replies arrive via `r2.composer.author.reply`.
3. The agent SHALL fetch any referenced datasheets via WebFetch and save them under the entry's `datasheets/` directory before referencing them.
4. The agent SHALL produce, at minimum, the five artefacts required by PROCESS.md §4 (canonical artefact, narrative markdown, AI-CONTEXT.md, datasheets, conversation transcript).
5. The agent SHALL validate the produced canonical artefact against its upstream spec (R2-PLUGIN §12.3 for `plugin.toml`, R2-DEF §2 for `sentant.yaml`, this spec §3.1 for `board.toml`) BEFORE emitting `r2.composer.author.done`.
6. The Author sentant SHALL emit `r2.composer.author.error` if validation fails AND remove the partial entry directory — the catalogue MUST NOT contain half-authored entries.

---

## 8. Security

- TG private keys: KeyHolder for the `r2-composer`-scoped subgroup lives off-tree at `~/.config/r2-composer/tg_signer/` (mirrors r2-workshop's policy).
- Claude Code is invoked with `--dangerously-skip-permissions` ONLY if explicitly authorised by the operator in `~/.config/r2-composer/orchestrator.toml`. Default is interactive (operator confirms each tool call). r2-composer's UI MAY surface `claude` permission prompts as `r2.composer.author.prompt` events for the operator to approve.
- The webapp bundle is served with the security headers mandated by R2-PLUGIN §13.9 (Content-Security-Policy, etc.).
- Datasheets fetched via WebFetch are saved to disk only — the agent MUST NOT execute or render fetched content beyond storing the bytes.

---

## 9. Conformance

A repository conforms to this spec when:

1. The two-hive composition of §3 is realised (browser WASM hive + workstation Rust hive).
2. The event vocabulary of §4 is honoured — events out of vocabulary are forbidden on the `r2.composer.*` namespace.
3. The compile path of §5 is followed for every build, with no mocked successes.
4. The three round-trip vectors of §6 pass.
5. The catalogue authoring flow of §7 produces entries that conform to PROCESS.md §4.

---

## 11. Trust Group management

The operator's Trust Group is a first-class surface in the UI. r2-composer's orchestrator hive holds the KeyHolder role for the operator's TG; the canvas state tracks TG membership.

**Implicit operations** (driven by canvas actions, no explicit "enrol" dialog):

| Canvas action | TG effect |
|---|---|
| Drag a carrier board onto canvas AND successfully compile + deploy firmware to a physical device | Implicit ADD: KeyHolder issues a `DeviceCertificate` for the device's freshly-minted Ed25519 keypair; cert is delivered with the firmware (USB sideload first install, L2CAP `#wifi_offer` for re-enrolment); orchestrator's device roster updated. |
| Remove a board from the canvas | Implicit REVOKE: the physical device's cert is added to the KeyHolder's revocation list. The board entry stays in the catalogue. |

**Explicit operations** (TG-pane surface, separate from the canvas):

| Action | Event | Notes |
|---|---|---|
| Inspect TG status | `r2.composer.tg.status` | Headline: name, class_hash, KeyHolder fingerprint, member count |
| List members | `r2.composer.tg.list_members` → `r2.composer.tg.member` ×N + `done: true` | Streamed device roster |
| Revoke a member directly | `r2.composer.tg.revoke_device { device_pk, reason }` | When the physical device is lost but the carrier entry stays |
| Rotate KeyHolder | `r2.composer.tg.rotate_keyholder { new_keyholder_target }` | Per R2-TRUST §5.5 |
| Export / import KeyHolder material | `r2.composer.tg.export_keyholder` / `r2.composer.tg.import_keyholder` | Operator-managed backup of `tg_priv.bin` |
| **Reset TG** | `r2.composer.tg.reset { confirm: "I-understand-this-invalidates-all-devices" }` | DESTRUCTIVE: generates a new TG keypair + class hash, invalidates every existing device cert. UI MUST require exact-string confirmation. Per r2-workshop's class-rotation procedure (`SPEC-R2-WORKSHOP-ENSEMBLE` §2.3). |

**Storage:** TG public material at `trust_keys/` in the repo (`tg_pub.bin`, `tg_cert.bin`, device roster). TG private material off-tree at `~/.config/r2-composer/tg_signer/` (`tg_priv.bin`).

Full design detail in `[[project-tg-management-workflow]]` (memory).

---

## 12. Device lifecycle and deploy paths

Two distinct deploy paths per device, gated by the device's current state in the orchestrator's per-device roster:

| Device state | UI label | Deploy mechanism |
|---|---|---|
| Never built | "Compile" | Compile only; produce artefact; no deploy. |
| Built but never flashed | "First install (USB)" | `Flasher` sentant runs `esptool --chip <chip> --port <port> write_flash 0x0 bootloader 0x8000 partition-table 0x10000 app` (offsets per partitions.csv for the carrier — 0x20000 for ESP32-C6). Operator picks the USB serial port from `r2.composer.flash.devices`. |
| Flashed once, currently reachable | "OTA update" | Compile produces app-only `.bin`; orchestrator pushes to the device's TCP port 21043 over WiFi; device's compulsory OTA plugin writes the inactive slot, swaps, reboots. Bootloader rollback protects against bricks. |
| Flashed but unreachable | "OTA update (offline — will retry)" | Compile + queue; retry on reachability events. Fall back to USB on operator override. |
| Operator override | "Force USB flash" | Always USB regardless of state. |

OTA-receiver-on-device is **compulsory** per §12.1 below — every build for an MCU carrier MUST include an OTA plugin provider, otherwise the device becomes unmanageable.

### 12.1 Compulsory plugins

Per [[project-compulsory-plugins-and-virgin-boards]], some capabilities are non-negotiable for any deployable build. Each `board.toml` carries a `[compulsory_plugins]` table (see `SPEC-CATALOGUE-LAYOUT` §3.2) listing the capabilities that MUST be satisfied at build time. The compiler plugin's resolve step (§5 step 3) MUST verify each compulsory capability is provided by a plugin in scope. Otherwise the build fails with `E_COMPULSORY_PLUGIN_MISSING`.

Initial v0.1 compulsory capabilities:
- `ai.reality2.deploy.ota` — declared per-carrier in each `board.toml`.
- `r2.crypto.ed25519.*` — implicitly satisfied by `crates/r2-plugin-crypto-software-ed25519` (always linked).

Compulsory plugins are NOT surfaced as opt-in catalogue items on the canvas. The UI may display them as greyed-out / pre-checked to indicate their presence, but operators cannot deselect them.

### 12.2 Deploy events (extending §4.5)

The earlier `r2.composer.flash.*` set is generalised to a `r2.composer.deploy.*` namespace that supports both USB and OTA modes:

| Event | Direction | Payload | Purpose |
|---|---|---|---|
| `r2.composer.flash.devices` | orchestrator → webapp | `[{port, chip_guess}]` | List detected USB serial devices for First-install |
| `r2.composer.deploy.start` | webapp → orchestrator | `{ artefact_path, target_ip?, port?, mode: "usb"\|"ota"\|"auto" }` | Unified deploy entry; orchestrator dispatches based on `mode` + per-device state |
| `r2.composer.deploy.progress` | orchestrator → webapp | `{ phase, message, bytes_sent, bytes_total }` | Streaming progress |
| `r2.composer.deploy.done` | orchestrator → webapp | `{ device_pk, duration_ms, bytes }` | Update per-device deploy state to "Flashed". TRIGGERS implicit TG add per §11. |
| `r2.composer.deploy.error` | orchestrator → webapp | `{ phase, message }` | Deploy failed |

---

## 13. r2-composer is itself structurally an R2 ensemble

r2-composer is declared (§2.1) as an R2 ensemble with class `ai.reality2.ensemble.r2-composer`, composed of two role-ensembles (orchestrator + webapp) bound by R2-WIRE. This is a **structural** ensemble-ness, not a runtime-TG one:

- **Structural** (asserted): r2-composer's own architecture is R2-DEF §7 conformant. A `meta/r2-composer-as-ensemble.yaml` self-description will be authored in Phase 1.6+ when the orchestrator + webapp Rust sources are written, alongside the role-ensemble's sentants + plugins. The self-description lives under `meta/`, distinct from `catalogue/` (which holds parts r2-composer builds firmware for) and `apiaries/` (operator's deployments).
- **Runtime** (constraint): r2-composer's hive instances do NOT have a standing "r2-composer TG." Per R2-TRUST §2.3, every R2 hive is in exactly one TG at a time. At runtime, r2-composer's orchestrator + webapp hives take on the **active apiary's** TG context — see SPEC-APIARY-LAYOUT.md §6. Apiary switch = TG-context switch for the whole stack including the tooling.

Multi-operator collaboration on one apiary is free under this model: both operators' r2-composer instances are members of the apiary's TG; they see each other via R2-WIRE within that TG.

The meta self-description is dogfood: r2-composer's catalogue browser will be able to INSPECT r2-composer itself once `meta/` is populated. See `[[project-r2-composer-self-as-ensemble]]` in memory for the full reasoning.

## 14. Companion specs

| Spec | Purpose |
|---|---|
| [`SPEC-CATALOGUE-LAYOUT.md`](SPEC-CATALOGUE-LAYOUT.md) | Layout + validation of `catalogue/boards/` + `catalogue/ensembles/<name>/` |
| [`SPEC-APIARY-LAYOUT.md`](SPEC-APIARY-LAYOUT.md) | Layout + validation of `apiaries/<name>/` + `apiary.toml` schema |
| [`SPEC-APIARY-AMENDMENT-PROPOSAL.md`](SPEC-APIARY-AMENDMENT-PROPOSAL.md) | Proposed upstream amendment to R2-APIARY (broaden scope to include the TG-bound multi-hive case) |

## 15. Change log

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1 | Initial draft. Establishes the two-hive ensemble, the `r2.composer.*` event vocabulary, the v0.1 success gate (round-trip the three r2-workshop carriers), and the catalogue authoring obligations. |
| 2026-05-31 | 0.2 | Added §11 Trust Group management (implicit add/revoke from canvas + explicit `r2.composer.tg.*` events). Added §12 Device lifecycle and deploy paths (USB First-install vs OTA update flows; compulsory plugins; `r2.composer.deploy.*` event set generalising the earlier flash-only events). |
| 2026-05-31 | 0.3 | Second-pass decisions: §13 acknowledges r2-composer's structural-ensemble nature with deferred `meta/` self-description (Phase 1.6+); §14 companion-spec index added pointing at the new SPEC-APIARY-LAYOUT + SPEC-APIARY-AMENDMENT-PROPOSAL. Apiary terminology adopted throughout. |
