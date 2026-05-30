# SPEC-CATALOGUE-LAYOUT: directory shape and authoring rules for r2-compiler's catalogue

**Version:** 0.2 Draft
**Date:** 2026-05-31
**Status:** Normative Draft
**Depends on:**
- **Upstream (canonical):** R2-PLUGIN §12 (plugin manifest, README mandatory sections), R2-DEF §2 (sentant schema), R2-DEF §7 (ensemble score), R2-ENSEMBLE §2.1.2 (hive-shared vs ensemble-owned), R2-COMPILE §4 (compile targets), R2-BUILD §2 (target triples)
- **r2-compiler:** companion [`SPEC-R2-COMPILER.md`](SPEC-R2-COMPILER.md)

---

## 1. The two-part canvas model

r2-compiler's visual canvas exposes exactly two kinds of opt-in part:

1. **Carrier board** — exactly one per build. The carrier IS a plugin per R2-PLUGIN §1 (it provides hardware capabilities to the hive), but in the UI taxonomy it occupies its own category because it is the substrate everything else runs on.
2. **Ensembles** — one or more per build. An ensemble is a complete R2-ENSEMBLE / R2-DEF §7 unit: a class string + a set of sentants + a set of ensemble-owned plugins + UI registrations + capability declarations.

Plugins and sentants are NOT separate top-level catalogue trees. They live **inside** an ensemble (for ensemble-owned plugins + sentants) or **inside** a carrier board (for hive-shared singleton plugins like radios).

Always-available infrastructure — the R2 protocol stack, FNV/CBOR/wire/trust crates, crypto primitives — lives under `crates/` and is linked into every build unconditionally. It does NOT appear in the catalogue.

```
crates/                              vendored R2 stack + core plugins (always-linked)
├── r2-engine/  r2-fnv/  r2-cbor/  r2-wire/  r2-trust/  r2-route/
├── r2-def/  r2-ensemble/  r2-wasm/
└── r2-plugin-crypto-software-ed25519/   ← core plugin per [[feedback-core-vs-optin-plugins]]

catalogue/                           opt-in parts shown on the canvas
├── boards/
│   └── <arch>-<chip>-<carrier>/     one entry per carrier — pick exactly one per build
└── ensembles/
    └── <name>/                      one entry per ensemble — pick one or more per build
```

## 2. Scope

This spec normatively defines:

- The on-disk shape of every catalogue entry (board, ensemble).
- The on-disk shape of the plugins + sentants nested inside an ensemble.
- The role of `AI-CONTEXT.md` at each level.
- Conformance checks the orchestrator MUST run before publishing an entry.
- Decommissioning rules.

Out of scope:

- Plugin trait + AOT/NIF mode semantics — see R2-PLUGIN §12.
- Sentant FSM semantics — see R2-DEF §2.
- Per-carrier firmware crate shape — see R2-COMPILE and r2-workshop's per-carrier examples.

## 3. Boards

### 3.1 Directory layout

```
catalogue/boards/<arch>-<chip>-<carrier>/
  board.toml                  # REQUIRED — see §3.2
  BOARD.md                    # REQUIRED — narrative
  AI-CONTEXT.md               # REQUIRED — fresh-CC brief
  pinout.svg                  # OPTIONAL in v0.1; REQUIRED once Phase 4 (pin visualisation) lands
  plugins/                    # OPTIONAL — hive-shared singleton plugins for this carrier
    <category>/<name>/        # e.g. comms/ble-radio, comms/wifi-radio, comms/lora-radio
  templates/                  # REQUIRED — per-carrier firmware-crate seed files
    Cargo.toml.tera           # REQUIRED — Tera template; rendered per build
    sdkconfig.defaults        # OPTIONAL — ESP-IDF carriers
    partitions.csv            # OPTIONAL — ESP-IDF carriers
    .cargo/config.toml        # REQUIRED — target triple + linker
    rust-toolchain.toml       # OPTIONAL — pins toolchain
    wifi_config.toml.example  # OPTIONAL — dev fallback for WiFi creds
  datasheets/                 # REQUIRED if vendor PDFs were consulted
    *.pdf                     # one or more vendor datasheets / schematic exports
  conversation/               # REQUIRED — one transcript per authoring session
    YYYY-MM-DD-<topic>-NN.md
```

Directory NAME = `<arch>-<chip>-<carrier>`, kebab-case:

| Segment | Value |
|---|---|
| `<arch>` | R2-COMPILE §4 platform tag (`esp32`, `nrf`, `rp2`, `avr`, `linux-embedded`) |
| `<chip>` | chip family slug (`s3`, `c6`, …) |
| `<carrier>` | board model (`devkitc`, `xiao`, `dfr1117`, …) |

### 3.2 `board.toml`

```toml
[board]
name        = "esp32-c6-dfr1117"
arch        = "esp32"
chip        = "esp32c6"
carrier     = "dfr1117"
version     = "0.1.0"
description = "DFRobot DFR1117 ESP32-C6 carrier (RISC-V, NimBLE, WiFi 6, BLE 5.3)."

[build]
target_triple   = "riscv32imac-esp-espidf"
toolchain       = "esp"
flash_size_mb   = 4
psram           = false
usb_serial_jtag = true

[compile_target]
tag = "esp32-c6"                               # R2-DEF §7.7

[capabilities]
# Hardware-shared singletons exposed by this carrier.
# Ensemble plugins' `capabilities.requires` MUST be satisfiable from this list
# OR from a hive-shared plugin declared under this board's plugins/ subtree.
provides = [
  "r2.hw.spi", "r2.hw.i2c", "r2.hw.gpio", "r2.hw.adc", "r2.hw.uart",
  "r2.hw.ble", "r2.hw.wifi", "r2.hw.flash", "r2.hw.sdspi",
]

[pinout]                                       # OPTIONAL in v0.1; used by Phase 4

[references]
vendor_url        = "https://wiki.dfrobot.com/..."
chip_datasheet    = "esp32-c6-datasheet.pdf"
carrier_schematic = "dfr1117-schematic.pdf"
```

Validation (orchestrator's `CatalogueServer` MUST enforce):

| Rule | Error |
|---|---|
| `board.name` matches the directory name | `E_BOARD_NAME` |
| `build.target_triple` is in R2-BUILD §2 table | `E_BOARD_TRIPLE` |
| `compile_target.tag` is in R2-DEF §7.7 list | `E_BOARD_TAG` |
| Every `references.*` file exists under `datasheets/` | `E_BOARD_DS` |
| `templates/Cargo.toml.tera` and `.cargo/config.toml` exist | `E_BOARD_TPL` |
| Every plugin under `plugins/` validates per §5 | propagated |

### 3.3 `AI-CONTEXT.md` (per board)

MUST contain, in this order:

1. **Purpose** — one paragraph. What this board is.
2. **Class + target** — `<arch>-<chip>-<carrier>` + target triple + R2-DEF §7.7 tag.
3. **Where the canonical artefact lives** — `board.toml`.
4. **Vendor refs** — datasheet filenames under `datasheets/` (the bytes must be on disk too — not live URLs alone).
5. **Hive-shared plugins on this carrier** — if any are under `plugins/`, list them.
6. **Known gotchas / quirks** — bootstrapping, USB chips, pin remappings, sdkconfig hazards.
7. **Read these files in this order** for a fresh CC session resuming work on this board.

## 4. Ensembles

### 4.1 Directory layout

```
catalogue/ensembles/<name>/
  ensemble.yaml               # REQUIRED — R2-DEF §7 score, the canonical artefact
  ENSEMBLE.md                 # REQUIRED — narrative + composition diagram
  AI-CONTEXT.md               # REQUIRED — fresh-CC brief
  plugins/                    # ensemble-owned plugins (R2-ENSEMBLE §2.1.2)
    <category>/<name>/        # e.g. sensor/adxl355, storage/sd-card
  sentants/                   # the ensemble's sentants
    <Name>/                   # PascalCase, must equal sentant.name
  datasheets/                 # OPTIONAL — ensemble-level reference PDFs (rare)
  conversation/               # REQUIRED — authoring transcripts
```

Directory NAME = the ensemble's `name` field (kebab-case). e.g. `rocker-sensor`, `notekeeper`, `photo-share`.

### 4.2 `ensemble.yaml`

Conforms to R2-DEF §7 exactly. The orchestrator MUST run R2-DEF §7.10 load-time validation.

Extra checks for r2-compiler:

| Rule | Error |
|---|---|
| `class` follows R2-CAP §3 reverse-DNS convention | `E_ENS_CLASS` |
| `compile_target` overlaps with at least one board in the catalogue | `E_ENS_NO_TARGET` |
| Every plugin name referenced in any sentant resolves either to (a) this ensemble's own `plugins/`, (b) the chosen carrier's `plugins/`, or (c) a core crate under `crates/r2-plugin-*` | `E_ENS_PLUGIN_UNRESOLVED` |
| Every sentant uses only the R2-COMPILE §3.1 compilable subset (for AOT carriers) | `E_ENS_NOT_COMPILABLE` |

### 4.3 Nested plugins (`catalogue/ensembles/<name>/plugins/`)

Per R2-PLUGIN §12 — same layout as the upstream catalogue at `r2-core/plugins/`:

```
plugins/<category>/<name>/
  plugin.toml                 # REQUIRED — R2-PLUGIN §12.3
  PLUGIN.md                   # REQUIRED — R2-PLUGIN §12.8 — all 10 sections mandatory
  README.md                   # REQUIRED — crate-level doc
  Cargo.toml                  # REQUIRED — features: aot, nif (mutually exclusive per §12.5)
  AI-CONTEXT.md               # REQUIRED — fresh-CC brief
  src/                        # REQUIRED — lib.rs + plugin.rs + driver.rs (as applicable)
  datasheets/                 # REQUIRED if the plugin wraps a specific chip
  tests/                      # OPTIONAL — native integration tests
  conversation/               # REQUIRED — authoring transcripts
```

Category MUST be one of R2-PLUGIN §12.2's categories. Crate name in `Cargo.toml` MUST be `r2-plugin-<category>-<name>` per R2-PLUGIN §12.5.

Conformance checks (run at sync + composition time):

1. `plugin.toml` parses cleanly with all REQUIRED fields per R2-PLUGIN §12.3.
2. `Cargo.toml` declares `aot` and `nif` mutually-exclusive features per R2-PLUGIN §12.5.
3. `src/lib.rs` exports a type implementing `r2_engine::plugin::Plugin`.
4. `PLUGIN.md` contains all 10 mandatory sections per R2-PLUGIN §12.8.
5. Every command listed in `plugin.toml` `[commands]` has a matching opcode constant in `src/lib.rs`.
6. Every datasheet referenced in `PLUGIN.md` §7 exists under `datasheets/`.

### 4.4 Nested sentants (`catalogue/ensembles/<name>/sentants/`)

```
sentants/<Name>/
  sentant.yaml                # REQUIRED — R2-DEF §2 schema
  SENTANT.md                  # REQUIRED — narrative + FSM diagram (Mermaid recommended)
  AI-CONTEXT.md               # REQUIRED
  conversation/               # REQUIRED — authoring transcripts
  examples/                   # OPTIONAL — example invocations / event sequences
```

`<Name>` is PascalCase and MUST equal `sentant.name`.

Additional r2-compiler checks:

| Rule | Error |
|---|---|
| Compilable subset per R2-COMPILE §3.1 (for AOT targets) — no API plugins, no dynamic sentant creation, no swarm loading | `E_SENT_NOT_COMPILABLE` |
| Every `plugin:` reference resolves to a plugin under this ensemble's `plugins/`, the carrier's `plugins/`, or a core crate | `E_SENT_PLUGIN_UNRESOLVED` |

### 4.5 `AI-CONTEXT.md` (per ensemble)

MUST contain:

1. **Purpose** — one paragraph.
2. **Class + version** — quoted from `ensemble.yaml`.
3. **Where the canonical artefact lives** — `ensemble.yaml`.
4. **Composition summary** — sentants + plugins listed in plain language; reference each `sentants/<Name>/AI-CONTEXT.md` and `plugins/<cat>/<name>/AI-CONTEXT.md`.
5. **Compile targets** — which carriers this ensemble works on.
6. **Known coupling** — other ensembles in the catalogue this one interacts with (entanglement, shared events).
7. **Read these files in this order** for a fresh CC session.

## 5. Hive-shared singleton plugins on a carrier (`catalogue/boards/<carrier>/plugins/`)

Per R2-ENSEMBLE §2.1.2, transports and other "one per hive" resources (BLE radio, WiFi radio, R2-WEB, audio mixer) belong to the carrier, not to any single ensemble. They live under the board entry's `plugins/` subtree.

Same layout as §4.3 (a plugin is a plugin). The orchestrator's compose-step resolves an ensemble's `requires:` against (this ensemble's plugins) ∪ (the chosen carrier's plugins) ∪ (core plugins in `crates/`).

## 6. Authoring flow (normative — referenced from SPEC-R2-COMPILER §7)

When the operator initiates `r2.compiler.author.start{kind, description}`, the orchestrator's `AuthorPilot` MUST:

1. **Create the entry directory** per §3.1 / §4.1 — empty, just the shell.
2. **Spawn `claude -p`** with the entry's parent directory as cwd + a system prompt template from `orchestrator/prompts/author-<kind>.md` that cites the relevant upstream spec.
3. **Stream the agent's clarifying questions** via `r2.compiler.author.prompt` to the operator. Operator replies via `r2.compiler.author.reply`.
4. **Allow the agent to use WebFetch** for vendor datasheets and Write for files under the new entry directory ONLY. Writes outside MUST be surfaced to the operator, not silently performed.
5. **Inside an ensemble authoring session**, the operator MAY further request "add a new plugin/sentant to this ensemble" — the AuthorPilot spawns a nested authoring flow scoped to `catalogue/ensembles/<name>/plugins/...` or `.../sentants/...`.
6. **On completion**, validate against §3.2 / §4.2 / §4.3 / §4.4. On failure, re-enter the loop with the validation error surfaced to the operator. On success, emit `r2.compiler.author.done`.
7. **On error/abandonment**, run the §7 decommission flow before emitting `r2.compiler.author.error`.

### 6.1 New-board flow

System prompt: `orchestrator/prompts/author-board.md`. Cites R2-COMPILE §4, R2-BUILD §2, R2-HW. Outputs: `board.toml`, `BOARD.md`, `templates/*`, `datasheets/*`, `AI-CONTEXT.md`, `conversation/*`.

### 6.2 New-ensemble flow

System prompt: `orchestrator/prompts/author-ensemble.md`. Cites R2-ENSEMBLE, R2-DEF §7. Outputs: `ensemble.yaml`, `ENSEMBLE.md`, optionally seed sentants/plugins, `AI-CONTEXT.md`, `conversation/*`. The agent SHOULD ask whether the operator wants to start with a copy of an existing ensemble (e.g. fork rocker-sensor).

### 6.3 New-plugin / new-sentant flow (always inside an ensemble or board)

System prompts: `orchestrator/prompts/author-plugin.md`, `orchestrator/prompts/author-sentant.md`. These are NEVER invoked at top level — the operator must first have an ensemble (or carrier) open as the scope. The agent enforces this by examining the cwd.

## 7. Decommissioning

Removing a catalogue entry requires:

1. The entry has no live references — no score under [`../scores/`](../scores/) includes it (for ensembles); no ensemble depends on it (for boards); no other catalogue entry's `AI-CONTEXT.md` cites it.
2. A `conversation/YYYY-MM-DD-decommission-<topic>-NN.md` transcript documenting the reason.
3. `git rm -r catalogue/<branch>/<entry>/` — NOT soft-delete via renaming.

## 8. Conformance

A `catalogue/` tree conforms when:

1. Every entry directory follows §3.1 / §4.1 exactly.
2. Every entry passes the validation suite of §3.2 / §4.2 / §4.3 / §4.4.
3. Every entry has a `conversation/` directory with at least one transcript.
4. Every entry has an `AI-CONTEXT.md` matching §3.3 / §4.5.

The orchestrator's `CatalogueServer` MUST report any non-conforming entry as `degraded` in catalogue listings — degraded entries cannot be selected on the canvas until repaired.

---

## 9. Change log

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1 | Initial draft. Three-branch catalogue (boards/plugins/sentants). |
| 2026-05-31 | 0.2 | **Restructured to two-part canvas model** per `[[feedback-two-part-canvas]]`. Plugins and sentants are no longer top-level catalogue trees — they live inside ensembles (ensemble-owned) or boards (hive-shared singletons). Always-available infrastructure including the crypto plugin lives in `crates/`. |
