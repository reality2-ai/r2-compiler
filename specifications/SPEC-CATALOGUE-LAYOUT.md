# SPEC-CATALOGUE-LAYOUT: directory shape and authoring rules for r2-compiler's catalogue

**Version:** 0.3 Draft
**Date:** 2026-06-01
**Status:** Normative Draft
**Depends on:**
- **Upstream (canonical):** R2-PLUGIN §12 (plugin manifest, README mandatory sections), R2-DEF §2 (sentant schema), R2-DEF §7 (ensemble score), R2-ENSEMBLE §2.1.2 (hive-shared vs ensemble-owned), R2-COMPILE §4 (compile targets), R2-BUILD §2 (target triples), RFC 2119 + RFC 8174 (normative keywords)
- **r2-compiler:** companion [`SPEC-R2-COMPILER.md`](SPEC-R2-COMPILER.md)

## Conventions

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**, and **OPTIONAL** in this document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when, and only when, they appear in all capitals, as shown here.

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

A carrier-board catalogue entry **MUST** conform to the layout below. Files marked **REQUIRED** **MUST** be present; files marked **OPTIONAL** **MAY** be present.

```
catalogue/boards/<arch>-<chip>-<carrier>/
  board.toml                  # REQUIRED — canonical artefact (§3.3)
  BOARD.md                    # REQUIRED — narrative (§3.4)
  AI-CONTEXT.md               # REQUIRED — fresh-CC brief (§3.5)
  pinout.svg                  # OPTIONAL in v0.1; REQUIRED once Phase 4 lands
  plugins/                    # OPTIONAL — hive-shared singleton plugins (§5)
    <category>/<name>/        # e.g. comms/ble-radio, comms/wifi-radio
  templates/                  # REQUIRED — per-carrier firmware-crate seed files (§3.6)
    Cargo.toml.tera           # REQUIRED — Tera template; rendered per build
    .cargo/config.toml        # REQUIRED — target triple + linker
    build.rs                  # REQUIRED — env stamping, partition staging
    rust-toolchain.toml       # REQUIRED — pinned toolchain
    sdkconfig.defaults        # REQUIRED for ESP-IDF carriers
    partitions.csv            # REQUIRED for ESP-IDF carriers
    wifi_config.toml.example  # REQUIRED if the carrier supports WiFi
  datasheets/                 # REQUIRED if vendor PDFs / wiring docs were consulted
    HARDWARE-WIRING-<X>.md    # REQUIRED — wiring guide; filename SHOULD match
                              # `references.wiring_guide` in board.toml
    *.pdf                     # OPTIONAL — vendor datasheets / schematic exports
  material/                   # OPTIONAL — raw uploaded references awaiting processing
                              # (per [[project-material-collection-and-processing]])
  conversation/               # REQUIRED — ≥1 transcript per authoring session
    YYYY-MM-DD-<topic>-NN.md
```

The directory name **MUST** be `<arch>-<chip>-<carrier>` in kebab-case:

| Segment | Value |
|---|---|
| `<arch>` | R2-COMPILE §4 platform tag (`esp32`, `nrf`, `rp2`, `avr`, `linux-embedded`) |
| `<chip>` | Chip family slug (`s3`, `c6`, `nrf52840`, `rp2040`, …) |
| `<carrier>` | Board / module model (`devkitc`, `xiao`, `dfr1117`, …) |

### 3.2 Class string and hash

A carrier board's R2 class string **MUST** be `ai.reality2.board.<carrier>` (e.g. `ai.reality2.board.dfr1117`). The class hash is `FNV-1a-32(class_string_utf8_bytes)` per R2-FNV §2. The orchestrator computes the hash; `board.toml` **SHOULD NOT** include a hard-coded hash.

### 3.3 `board.toml` — REQUIRED canonical artefact

Every key below is normatively scoped. Unknown keys **MUST NOT** be silently accepted by validators — they **MUST** be either accepted (after a spec amendment) or reported as `E_BOARD_UNKNOWN_KEY`.

#### 3.3.1 `[board]` — REQUIRED

| Key | Type | Status | Notes |
|---|---|---|---|
| `name` | string | **REQUIRED** | **MUST** equal the directory name. |
| `arch` | string | **REQUIRED** | **MUST** be a R2-COMPILE §4 platform tag. |
| `chip` | string | **REQUIRED** | ESP-IDF / vendor chip-family slug (`esp32s3`, `esp32c6`, `nrf52840`, …). |
| `carrier` | string | **REQUIRED** | Carrier model slug (`devkitc`, `xiao`, `dfr1117`). **MUST** match the `<carrier>` segment of the directory name. |
| `version` | string | **REQUIRED** | Semver. Tracks the board entry's own version, not the chip's revision. |
| `description` | string | **REQUIRED** | Multi-line freeform paragraph. **SHOULD** describe physical form factor, key peripherals, USB topology, role in the catalogue. |

#### 3.3.2 `[build]` — REQUIRED

| Key | Type | Status | Notes |
|---|---|---|---|
| `target_triple` | string | **REQUIRED** | **MUST** appear in R2-BUILD §2 table. |
| `toolchain` | string | **REQUIRED** | Toolchain identifier (`esp` via `espup install`, `nightly`, `stable`, etc.). |
| `esp_idf_version` | string | **REQUIRED** if `chip` starts with `esp32` | Quoted from `templates/.cargo/config.toml`. |
| `flash_size_mb` | integer | **REQUIRED** | **MUST** match `CONFIG_ESPTOOLPY_FLASHSIZE_*MB` in `sdkconfig.defaults` for ESP-IDF carriers. |
| `psram` | boolean | **REQUIRED** | |
| `psram_mode` | string | **REQUIRED** if `psram = true` | `"octal"` or `"quad"`. |
| `psram_speed_mhz` | integer | **REQUIRED** if `psram = true` | Typically `80` or `120`. |
| `usb_serial_jtag` | boolean | **REQUIRED** for ESP-IDF carriers | Whether the chip exposes native USB-Serial-JTAG. |
| `uart_cp2102` | boolean | **REQUIRED** for ESP-IDF carriers | Whether the carrier has a CP2102 (or equivalent) USB-UART bridge **in addition to** the native USB. **MUST** be explicitly `false` rather than omitted if absent. |

Other `[build]` keys **MAY** be added for non-ESP families (e.g. `bootloader_kind = "mcuboot"` for nRF); such additions **MUST** be reflected in an amendment to this spec.

#### 3.3.3 `[compile_target]` — REQUIRED

| Key | Type | Status | Notes |
|---|---|---|---|
| `tag` | string | **REQUIRED** | **MUST** appear in R2-DEF §7.7 list. Ensemble scores match against this tag. Multiple carriers **MAY** share a tag (e.g. esp32-s3-devkitc and esp32-s3-xiao both use `esp32-s3`). |

#### 3.3.4 `[capabilities]` — REQUIRED

| Key | Type | Status | Notes |
|---|---|---|---|
| `provides` | array&lt;string&gt; | **REQUIRED** | Hardware capabilities the carrier exposes (`r2.hw.*` namespace). Ensemble plugins' `capabilities.requires` **MUST** be satisfiable from this list or from a hive-shared plugin under this board's `plugins/`. |
| `absent` | array&lt;string&gt; | **RECOMMENDED** | Capabilities deliberately NOT present, listed when their absence is surprising (e.g. `r2.hw.psram` on a chip variant most people assume has PSRAM). |

#### 3.3.5 `[pinout]` — REQUIRED (empty header) + per-GPIO sub-tables

The empty `[pinout]` header **MUST** precede the per-GPIO sub-tables. For each GPIO the board exposes that has a documented role or is reserved:

```toml
[pinout.gpio.GPIO<N>]
description = "…"
silk        = "…"
header      = "…"            # OPTIONAL
functions   = [...]
role_hint   = "…"            # OPTIONAL
```

| Key | Type | Status | Notes |
|---|---|---|---|
| `description` | string | **REQUIRED** | Human-readable purpose, including pin number on header where relevant. |
| `silk` | string | **REQUIRED** | Silkscreen label on the physical board (e.g. `"D0"`, `"LP_RX"`, `"(on-board)"` for non-exposed pins). |
| `header` | string | **OPTIONAL** | Header-pin identifier (e.g. `"J1.4"`) for boards with named headers. |
| `functions` | array&lt;string&gt; | **REQUIRED** | Per-pin capability tags (`"digital"`, `"adc"`, `"spi"`, `"i2c"`, `"rtc"`, `"rmt"`, `"ledc"`, `"uart"`, `"spi-cs"`). |
| `role_hint` | string | **OPTIONAL** | Suggested binding (`"status-led"`, `"accel-cs"`, `"battery-sense"`, `"i2c-sda"`). Drives default pin assignments when the compiler plugin resolves ensemble plugin requirements. |

Two further tables **SHOULD** be present:

```toml
[pinout.reserved]
"GPIO<N>" = "<reason>"            # strapping pin, USB D±, PSRAM, etc.

[pinout.free]
gpio = ["GPIO<N>", "GPIO<M>", …]  # informative list of pins available for expansion
```

| Sub-table | Status | Notes |
|---|---|---|
| `[pinout.reserved]` | **RECOMMENDED** | Pins **MUST NOT** be wired to peripherals (strapping pins, USB D±, console UART, PSRAM lines). |
| `[pinout.free]` | **RECOMMENDED** | Informative — pins not currently assigned a role and safe for future bindings. |

#### 3.3.6 `[references]` — REQUIRED

| Key | Type | Status | Notes |
|---|---|---|---|
| `vendor_url` | string | **REQUIRED** | Carrier vendor's product / wiki page. |
| `chip_vendor_url` | string | **REQUIRED** | Chip vendor's product page (e.g. Espressif SoC page). |
| `product_url` | string | **OPTIONAL** | Retail SKU page where the carrier is sold. |
| `arduino_variant` | string | **OPTIONAL** | GitHub URL of the Arduino-ESP32 variant definition, when one exists. |
| `wiring_guide` | string | **REQUIRED** | Filename (relative to `datasheets/`) of the wiring guide markdown. The file **MUST** exist. Naming convention: `HARDWARE-WIRING-<CARRIER-UPPERCASE>.md`. |

Additional `*_url` keys **MAY** be added at the author's discretion; validators **MUST** accept them.

#### 3.3.7 `[compulsory_plugins]` — REQUIRED

Per SPEC-R2-COMPILER §12.1 and [[project-compulsory-plugins-and-virgin-boards]].

| Key | Type | Status | Notes |
|---|---|---|---|
| `capabilities` | array&lt;string&gt; | **REQUIRED** | Capabilities every build for this carrier **MUST** satisfy. **MUST** contain `"ai.reality2.deploy.ota"`. |
| `prefer` | array&lt;string&gt; | **REQUIRED** | Plugin slugs preferred when multiple plugins satisfy a compulsory capability. |

#### 3.3.8 `[notes]` — RECOMMENDED

| Key | Type | Status | Notes |
|---|---|---|---|
| `gotchas` | array&lt;string&gt; | **RECOMMENDED** | One string per surprise / quirk a future operator or fresh CC session would benefit from. AGENTS.md §3 governs what belongs here. |

#### 3.3.9 Validation

The orchestrator's `catalogue` plugin **MUST** enforce:

| Rule | Error |
|---|---|
| `board.name` equals the directory name | `E_BOARD_NAME` |
| `board.carrier` equals the `<carrier>` segment of the directory name | `E_BOARD_CARRIER` |
| `build.target_triple` is in R2-BUILD §2 | `E_BOARD_TRIPLE` |
| `compile_target.tag` is in R2-DEF §7.7 | `E_BOARD_TAG` |
| `build.psram_mode` and `build.psram_speed_mhz` are present iff `build.psram` is `true` | `E_BOARD_PSRAM_FIELDS` |
| `references.wiring_guide` resolves to an existing file under `datasheets/` | `E_BOARD_DS` |
| `templates/Cargo.toml.tera`, `.cargo/config.toml`, `build.rs`, `rust-toolchain.toml` exist | `E_BOARD_TPL` |
| `templates/sdkconfig.defaults` + `partitions.csv` exist for ESP-IDF carriers | `E_BOARD_TPL_ESP` |
| `compulsory_plugins.capabilities` contains `"ai.reality2.deploy.ota"` | `E_BOARD_NO_OTA` |
| Every plugin under `plugins/` validates per §5 | propagated |
| Every capability in `compulsory_plugins.capabilities` is provided by a plugin in scope at build time (composition-time check, not sync-time) | `E_COMPULSORY_PLUGIN_MISSING` |

### 3.4 `BOARD.md` — REQUIRED narrative

A board's narrative **MUST** open with an `# H1` heading naming the carrier in long form (e.g. `# DFRobot Beetle ESP32-C6 (DFR1117)`).

The following `## H2` sections **MUST** appear in this order:

| Section | Purpose |
|---|---|
| `## At a glance` | Bulleted summary of the carrier — chip, flash/PSRAM, USB topology, key on-board features. |
| `## Role in r2-compiler` | Where this carrier fits in the catalogue — reference variant, peer alternative, RISC-V vs Xtensa, etc. |
| `## Where to wire what` | Tabular summary of `[pinout.gpio.*]`; references `board.toml` as authoritative. |
| `## Build & flash` | How to compile and flash. Cites `templates/` files and R2-BUILD references. |
| `## Templates` | Inventory of `templates/` contents with a one-line purpose per file. |
| `## Known gotchas` | Subset of `[notes].gotchas` (full list cross-referenced to `board.toml`). |
| `## Authoring history` | Pointer into `conversation/` — date + topic + outcome of each session. |
| `## See also` | Sibling carriers, the wiring guide, related catalogue entries. |

A board **MAY** add additional `## H2` sections (e.g. `## Differences vs <peer>`) when they add value. Such sections **SHOULD** appear after `## Known gotchas` and before `## Authoring history`.

### 3.5 `AI-CONTEXT.md` — REQUIRED fresh-CC brief

The brief opens with an `# H1` heading `AI-CONTEXT.md — <board name>`.

The following `## H2` sections **MUST** appear in this order:

1. **`## Purpose`** — one paragraph. What this board is.
2. **`## Class + target`** — class string `ai.reality2.board.<carrier>`, FNV hash, target triple, R2-DEF §7.7 tag.
3. **`## Where the canonical artefact lives`** — `board.toml`.
4. **`## Vendor refs`** — datasheet filenames under `datasheets/`. The bytes **MUST** be on disk, not just URLs.
5. **`## Hive-shared plugins on this carrier`** — list any `plugins/<category>/<name>/` entries; if none, state explicitly.
6. **`## Templates`** — one line per file under `templates/`.
7. **`## Quick differences vs siblings`** — concise contrast with the closest peer carrier(s) in the catalogue.
8. **`## Known gotchas (quick read — full list in `board.toml [notes].gotchas`)`** — 3–6 highest-impact gotchas.
9. **`## Read these files in this order (cold-start resume)`** — ordered list of file paths a fresh CC session should consume before touching this board.
10. **`## Authoring status`** — current state (e.g. `synced from r2-workshop`, `divergence pending upstream catch-up`, `partial — Phase X work outstanding`).

### 3.6 `templates/` — REQUIRED firmware-crate seed files

Every carrier **MUST** ship a `templates/` subtree that the compiler plugin renders into a firmware crate per R2-COMPILE.

| File | Status | Purpose |
|---|---|---|
| `Cargo.toml.tera` | **REQUIRED** | Tera template; the compiler plugin renders this per build, substituting in the resolved plugin/sentant set and the active class string. |
| `.cargo/config.toml` | **REQUIRED** | Target triple, linker, runner, build-std flags. |
| `build.rs` | **REQUIRED** | Stamps env vars consumed by the firmware: class string, git SHA, build timestamp, partition table staging, WiFi config load. Function names **SHOULD** match across carriers (`stamp_sensor_class`, `track_git_state`, `stamp_build_metadata`, `stage_partitions_csv`, `load_wifi_config`). |
| `rust-toolchain.toml` | **REQUIRED** | Pins toolchain channel + components. |
| `sdkconfig.defaults` | **REQUIRED** for ESP-IDF carriers | ESP-IDF configuration. `CONFIG_ESPTOOLPY_FLASHSIZE_*MB` **MUST** match `[build].flash_size_mb`. |
| `partitions.csv` | **REQUIRED** for ESP-IDF carriers | Partition table. Layout sized for the carrier's flash. |
| `wifi_config.toml.example` | **REQUIRED** if carrier supports WiFi | Committable example; the real `wifi_config.toml` **MUST NOT** be committed (contains credentials). |

The `templates/` files **MUST** lint cleanly: a `cargo build` from a fresh checkout — using the template files unchanged with placeholder values for Tera variables — **SHOULD** succeed.

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
  plugin.toml                 # REQUIRED — R2-PLUGIN §12.3, with [modes] table
  PLUGIN.md                   # REQUIRED — R2-PLUGIN §12.8 — all 10 sections mandatory
  README.md                   # REQUIRED — crate-level doc
  Cargo.toml                  # REQUIRED — feature flags one per declared mode
  AI-CONTEXT.md               # REQUIRED — fresh-CC brief
  src/                        # REQUIRED — lib.rs + plugin.rs + driver.rs (as applicable)
  assets/                     # REQUIRED for category=webapp — static bundle content (§4.3.1)
  datasheets/                 # REQUIRED if the plugin wraps a specific chip / SDK
  tests/                      # OPTIONAL — native integration tests
  conversation/               # REQUIRED — authoring transcripts
```

Category MUST be one of R2-PLUGIN §12.2's categories. Crate name in `Cargo.toml` MUST be `r2-plugin-<category>-<name>` per R2-PLUGIN §12.5.

#### 4.3.1 Modes — `aot`, `nif`, `web`

A plugin declares which build modes it supports via `plugin.toml [modes]`. Three modes are defined:

| Mode | Target hive | Cargo target | Output | Toolchain |
|---|---|---|---|---|
| `aot` | MCU (firmware) | `xtensa-esp32s3-espidf`, `riscv32imac-esp-espidf`, `thumbv8m.main-none-eabihf`, … (per R2-COMPILE §4) | `no_std` static lib linked into a flashed .bin | `cargo build --release --target=<triple>` |
| `nif` | BEAM (workstation) | host triple | `std` cdylib via `r2-nif` wrapper | `cargo build --release` |
| `web` | Browser (WASM hive) | `wasm32-unknown-unknown` | Static bundle directory (HTML + CSS + JS + .wasm + assets) served by R2-WEB per R2-PLUGIN §13 | `wasm-pack build --target web` (or Trunk / esbuild equivalent) |

`web` mode generalises R2-PLUGIN §11's future-work WASM mode + §13's web-plugin runtime contract. A plugin MAY declare multiple modes; modes are NOT mutually-exclusive at the manifest level (only at the Cargo build level, where mode-specific feature flags ARE mutually exclusive per R2-PLUGIN §12.5).

Example `[modes]` declarations:

```toml
# A sensor driver — MCU only.
[modes]
aot = { targets = ["esp32-s3", "esp32-c6"], no_std = true }
nif = false
web = false

# A crypto primitive — works everywhere.
[modes]
aot = { targets = ["esp32-s3", "esp32-c6", "linux-embedded"], no_std = true }
nif = { targets = ["linux-embedded", "server"] }
web = { targets = ["wasm32-unknown-unknown"] }

# A webapp dashboard — browser only.
[modes]
aot = false
nif = false
web = { targets = ["wasm32-unknown-unknown"], bundler = "wasm-pack", graphql_fragment = "graphql/schema.graphql" }
```

For webapp plugins (category `webapp`), `[modes.web]` MAY carry extra keys:

| Key | Purpose |
|---|---|
| `bundler` | Which build tool emits the bundle (`wasm-pack`, `trunk`, `esbuild`, …). Default `wasm-pack`. |
| `graphql_fragment` | Path (relative to plugin dir) of a GraphQL schema fragment per R2-PLUGIN §13.7. |
| `mount` | Default URL mount; can be overridden per R2-PLUGIN §13.2. Default: `/plugin/<plugin-name>`. |

#### 4.3.2 The `assets/` directory (webapp plugins only)

Required when `[modes.web]` is declared. Holds the static bundle content per R2-PLUGIN §13.3:

```
assets/
  index.html                  # REQUIRED — bundle root
  app.js                      # JS entry (loads the .wasm)
  styles.css
  images/                     # optional
  …
```

`wasm-pack` (or the configured bundler) emits the .wasm + glue JS into a `dist/` subdirectory at build time, which the compiler plugin copies alongside the `assets/` content into the bundle directory the orchestrator hands to R2-WEB.

`assets/` MUST NOT contain symlinks that escape it (R2-PLUGIN §13.3). The compiler plugin rejects such builds.

#### 4.3.3 Conformance checks (run at sync + composition time)

1. `plugin.toml` parses cleanly with all REQUIRED fields per R2-PLUGIN §12.3.
2. `Cargo.toml` declares feature flags matching every declared mode (e.g. `[features] aot = []`, `web = []`, `nif = ["std"]`); they MUST be mutually exclusive per R2-PLUGIN §12.5.
3. `src/lib.rs` exports a type implementing `r2_engine::plugin::Plugin` (for `aot` and `nif`) and/or `wasm-bindgen` entry points (for `web`).
4. `PLUGIN.md` contains all 10 mandatory sections per R2-PLUGIN §12.8.
5. Every command listed in `plugin.toml` `[commands]` has a matching opcode constant in `src/lib.rs`.
6. Every datasheet referenced in `PLUGIN.md` §7 exists under `datasheets/`.
7. For `web` mode: `assets/index.html` exists (R2-PLUGIN §13.3).
8. No symlinks escape `assets/`.

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

When the operator initiates `r2.compiler.author.start{kind, description}`, the orchestrator's `the Author sentant (calling the `claude-code` plugin)` MUST:

1. **Create the entry directory** per §3.1 / §4.1 — empty, just the shell.
2. **Spawn `claude -p`** with the entry's parent directory as cwd + a system prompt template from `orchestrator/prompts/author-<kind>.md` that cites the relevant upstream spec.
3. **Stream the agent's clarifying questions** via `r2.compiler.author.prompt` to the operator. Operator replies via `r2.compiler.author.reply`.
4. **Allow the agent to use WebFetch** for vendor datasheets and Write for files under the new entry directory ONLY. Writes outside MUST be surfaced to the operator, not silently performed.
5. **Inside an ensemble authoring session**, the operator MAY further request "add a new plugin/sentant to this ensemble" — the the Author sentant (calling the `claude-code` plugin) spawns a nested authoring flow scoped to `catalogue/ensembles/<name>/plugins/...` or `.../sentants/...`.
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

The orchestrator's `catalogue plugin` MUST report any non-conforming entry as `degraded` in catalogue listings — degraded entries cannot be selected on the canvas until repaired.

---

## 9. Change log

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1 | Initial draft. Three-branch catalogue (boards/plugins/sentants). |
| 2026-05-31 | 0.2 | **Restructured to two-part canvas model** per `[[feedback-two-part-canvas]]`. Plugins and sentants are no longer top-level catalogue trees — they live inside ensembles (ensemble-owned) or boards (hive-shared singletons). Always-available infrastructure including the crypto plugin lives in `crates/`. |
| 2026-06-01 | 0.3 | **§3 (Boards) rewritten** as the canonical board-authoring spec, with RFC 2119 (BCP 14) normative keywords throughout. Full `board.toml` schema documented (every section + key with REQUIRED / OPTIONAL / CONDITIONAL status); canonical BOARD.md + AI-CONTEXT.md section structure; canonical `templates/` file list; expanded validation rules. Authored by reading the three existing carrier entries (esp32-s3-devkitc, esp32-s3-xiao, esp32-c6-dfr1117) and codifying the conventions they collectively used. Sweep of those three entries to bring them into full conformance committed alongside this revision. |
