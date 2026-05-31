# SPEC-CATALOGUE-LAYOUT: directory shape and authoring rules for r2-composer's catalogue

**Version:** 0.4 Draft
**Date:** 2026-06-01
**Status:** Normative Draft
**Depends on:**
- **Upstream (canonical):** R2-PLUGIN §12 (plugin manifest, README mandatory sections), R2-DEF §2 (sentant schema), R2-DEF §7 (ensemble score), R2-ENSEMBLE §2.1.2 (hive-shared vs ensemble-owned), R2-COMPILE §4 (compile targets), R2-BUILD §2 (target triples), RFC 2119 + RFC 8174 (normative keywords)
- **r2-composer:** companion [`SPEC-R2-COMPOSER.md`](SPEC-R2-COMPOSER.md)

## Conventions

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**, and **OPTIONAL** in this document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when, and only when, they appear in all capitals, as shown here.

---

## 1. The two-part canvas model

r2-composer's visual canvas exposes exactly two kinds of opt-in part:

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

Per SPEC-R2-COMPOSER §12.1 and [[project-compulsory-plugins-and-virgin-boards]].

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
| `## Role in r2-composer` | Where this carrier fits in the catalogue — reference variant, peer alternative, RISC-V vs Xtensa, etc. |
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
  ensemble.yaml               # REQUIRED — canonical artefact (§4.3)
  ENSEMBLE.md                 # REQUIRED — narrative (§4.4)
  AI-CONTEXT.md               # REQUIRED — fresh-CC brief (§4.5)
  plugins/                    # OPTIONAL — ensemble-owned plugins (layout per §5)
    <category>/<name>/
  sentants/                   # REQUIRED ≥1 — the ensemble's sentants (layout per §6)
    <Name>/
  datasheets/                 # OPTIONAL — ensemble-level reference PDFs (rare)
  material/                   # OPTIONAL — raw uploaded references awaiting processing
  conversation/               # REQUIRED ≥1 — authoring transcripts
    YYYY-MM-DD-<topic>-NN.md
```

The directory name **MUST** equal the ensemble's `name` field in kebab-case (e.g. `rocker-sensor`).

### 4.2 Class string and hash

An ensemble's R2 class string is the `class:` field in `ensemble.yaml` (reverse-DNS per R2-CAP §3). Examples: `nz.ac.auckland.rocker`, `ai.reality2.notekeeper`. The class hash is `FNV-1a-32(class_string_utf8_bytes)`; `ensemble.yaml` **SHOULD NOT** include a hard-coded hash.

### 4.3 `ensemble.yaml` — REQUIRED canonical artefact

Conforms to R2-DEF §7. The top-level key **MUST** be `ensemble:`. The orchestrator **MUST** run R2-DEF §7.10 load-time validation in addition to the r2-composer rules below.

#### 4.3.1 Keys under `ensemble:` — REQUIRED

| Key | Type | Status | Notes |
|---|---|---|---|
| `name` | string | **REQUIRED** | Kebab-case. **MUST** equal the directory name. |
| `description` | string | **REQUIRED** | Paragraph describing the ensemble's role. |
| `version` | string | **REQUIRED** | Semver of the ensemble entry. |
| `class` | string | **REQUIRED** | Reverse-DNS class string per R2-CAP §3. |
| `ensemble_version` | string | **REQUIRED** | Schema version of `ensemble.yaml` (R2-DEF §7). |
| `compile_target` | array&lt;string&gt; | **REQUIRED** | Carrier tags (R2-DEF §7.7) this ensemble works on. **MUST** overlap with at least one carrier in the catalogue. |
| `sentants` | array | **REQUIRED**, **MUST** have ≥1 entry | Each entry's `name` **MUST** match a directory under `sentants/`. |
| `plugins` | array | **OPTIONAL** | Each entry's `name` **MUST** match a directory under `plugins/` OR resolve via the carrier / `crates/` (§5.1). |
| `trust_group` | table | **OPTIONAL** | `roles_allowed: ["member" \| "owner" \| ...]`. |
| `registrations` | table | **OPTIONAL** | Hive-shared-singleton registrations (R2-DEF §7.4). |
| `capabilities` | table | **RECOMMENDED** | `emits` + `consumes` aggregate lists for the whole ensemble. |
| `signatures` | array | **RECOMMENDED** | Trust signatures per R2-DEF §7.9. **MAY** be `[]` until the ensemble is shipped. |

#### 4.3.2 Sentant entries (inside `sentants:` array)

Each sentant entry under `ensemble.sentants[]` is a lightweight reference that the full `sentants/<Name>/sentant.yaml` (§6) elaborates. The in-yaml entry **MUST** contain at minimum:

```yaml
- name: <PascalCase>
  description: <paragraph>
  class: <reverse-DNS>
  storage: ephemeral | durable | durable-state
  plugins: [ { name: <slug> }, { capability: <cap-string> }, … ]   # OPTIONAL
  automations: [ { name: main, … } ]                                # OPTIONAL — full FSM lives in sentants/<Name>/
```

Sentants **MAY** be referenced by `capability:` rather than `name:` per R2-PLUGIN §10 — the orchestrator resolves to a concrete plugin at compile time.

#### 4.3.3 Plugin entries (inside `plugins:` array)

```yaml
- name: <slug>
  description: <paragraph>
  kind: native | webapp
  compile_target: [...]
  capabilities:
    provides: [...]
    requires: [...]
  events:
    handled: [...]
    emitted: [...]
```

#### 4.3.4 Validation

| Rule | Error |
|---|---|
| `ensemble.name` equals the directory name | `E_ENS_NAME` |
| `ensemble.class` follows R2-CAP §3 reverse-DNS convention | `E_ENS_CLASS` |
| `ensemble.compile_target` overlaps with at least one carrier in `catalogue/boards/` | `E_ENS_NO_TARGET` |
| Every `sentants[].name` resolves to a directory under `sentants/` | `E_ENS_SENTANT_UNRESOLVED` |
| Every `plugins[].name` resolves under (this ensemble's `plugins/`) ∪ (any carrier's `plugins/`) ∪ (`crates/r2-plugin-*`) | `E_ENS_PLUGIN_UNRESOLVED` |
| Every sentant uses only the R2-COMPILE §3.1 compilable subset for AOT carriers | `E_ENS_NOT_COMPILABLE` |

### 4.4 `ENSEMBLE.md` — REQUIRED narrative

An ENSEMBLE.md **MUST** open with an `# H1` heading naming the ensemble (long form acceptable).

The following `## H2` sections **MUST** appear in this order:

| Section | Purpose |
|---|---|
| `## At a glance` | Bulleted summary — role, deployment context, primary capability. |
| `## Composition` | Sentant + plugin inventory at a paragraph level; **MAY** include a composition diagram (Mermaid `flowchart` **RECOMMENDED**). |
| `## Sentants` | One sub-heading or table-row per sentant — name + one-sentence role. |
| `## Plugins` | Ensemble-owned plugins (NOT hive-shared singletons). One sub-heading or row per plugin. |
| `## Compile targets` | Which carriers the ensemble compiles on; rationale for any restrictions. |
| `## Coupling` | Other ensembles in the catalogue this one interacts with (shared events, entanglement). |
| `## Authoring history` | Pointer into `conversation/`. |
| `## See also` | Sibling ensembles, relevant upstream specs. |

### 4.5 `AI-CONTEXT.md` — REQUIRED fresh-CC brief

Opens with an `# H1` heading `AI-CONTEXT.md — <ensemble name>`.

REQUIRED `## H2` sections in this order:

1. **`## Purpose`** — one paragraph.
2. **`## Class + version`** — class string, FNV hash, `version`, `ensemble_version`.
3. **`## Where the canonical artefact lives`** — `ensemble.yaml`.
4. **`## Composition summary`** — sentants + plugins in plain language, with file-reference pointers into `sentants/<Name>/AI-CONTEXT.md` and `plugins/<cat>/<name>/AI-CONTEXT.md`.
5. **`## Compile targets`** — carriers + rationale.
6. **`## Known coupling`** — interactions with sibling ensembles.
7. **`## Known gotchas`** — things a future CC session would benefit from knowing.
8. **`## Read these files in this order (cold-start resume)`** — ordered file list.
9. **`## Authoring status`** — state of completion.

## 5. Plugins

### 5.1 Where plugins live

Plugin entries appear in **three** locations in the source tree:

1. **Ensemble-owned** — `catalogue/ensembles/<ensemble>/plugins/<category>/<slug>/` — the plugin belongs to a single ensemble (R2-ENSEMBLE §2.1.2 "ensemble-owned"). Visible on the canvas only through the ensemble.
2. **Hive-shared singletons on a carrier** — `catalogue/boards/<carrier>/plugins/<category>/<slug>/` — the plugin is a one-per-hive resource (BLE radio, WiFi radio, R2-WEB, audio mixer) belonging to the carrier. Per R2-ENSEMBLE §2.1.2. Visible on the canvas only through the carrier.
3. **Always-linked core plugins** — `crates/r2-plugin-<category>-<slug>/` — infrastructure (crypto, FNV/CBOR, …) linked into every build. **NOT** visible on the canvas; per [[feedback-core-vs-optin-plugins]].

The directory layout (§5.2), `plugin.toml` schema (§5.3), `PLUGIN.md` structure (§5.4), and `AI-CONTEXT.md` structure (§5.5) are identical across all three locations.

### 5.2 Directory layout

A plugin entry **MAY** exist in two stages of completeness:

**Stage 1 — Metadata-only** (minimum; the state most catalogue plugins are in for v0.1):

```
<slug>/
  plugin.toml                 # REQUIRED — §5.3
  PLUGIN.md                   # REQUIRED — §5.4
  AI-CONTEXT.md               # REQUIRED — §5.5
  conversation/               # REQUIRED ≥1 — authoring transcripts
    YYYY-MM-DD-<topic>-NN.md
```

A metadata-only plugin **MUST** declare `[plugin].status = "metadata-only"` so validators and the canvas can flag it as "source extraction pending" (Phase 1.4-source) and prevent it from being selected for a compile.

**Stage 2 — Fully realised**:

```
<slug>/
  plugin.toml                 # REQUIRED — §5.3 with [plugin].status = "ready"
  PLUGIN.md                   # REQUIRED — §5.4
  AI-CONTEXT.md               # REQUIRED — §5.5
  Cargo.toml                  # REQUIRED — feature flags matching declared modes (§5.3.2)
  README.md                   # REQUIRED — crate-level doc
  src/                        # REQUIRED — lib.rs + driver.rs / plugin.rs as applicable
    lib.rs
    …
  tests/                      # OPTIONAL — integration tests
  datasheets/                 # REQUIRED if the plugin wraps a specific chip / SDK
  assets/                     # REQUIRED for category=webapp — static bundle (§5.3.4)
  material/                   # OPTIONAL — raw uploads awaiting processing
  conversation/               # REQUIRED — authoring transcripts
```

Category **MUST** be one of R2-PLUGIN §12.2's categories. Crate name in `Cargo.toml` **MUST** be `r2-plugin-<category>-<slug>` per R2-PLUGIN §12.5.

### 5.3 `plugin.toml` — REQUIRED canonical artefact

Conforms to R2-PLUGIN §12.3. The orchestrator **MUST** run R2-PLUGIN's load-time validation in addition to the rules below.

#### 5.3.1 `[plugin]` — REQUIRED

| Key | Type | Status | Notes |
|---|---|---|---|
| `name` | string | **REQUIRED** | Plugin slug. **MUST** equal the directory name. |
| `category` | string | **REQUIRED** | R2-PLUGIN §12.2 category (`sensor`, `actuator`, `storage`, `comms`, `crypto`, `indicator`, `time`, `webapp`, …). |
| `version` | string | **REQUIRED** | Semver. |
| `description` | string | **REQUIRED** | Paragraph — what the plugin does, what hardware/SDK it wraps, swap-pair notes (per R2-PLUGIN §10). |
| `status` | string | **REQUIRED** | One of `"metadata-only"`, `"ready"`, `"deprecated"`. Validators **MUST** refuse to compile a plugin with `metadata-only` status. |

#### 5.3.2 `[modes]` — REQUIRED

A plugin declares which build modes it supports. Three modes are defined:

| Mode | Target hive | Cargo target | Output |
|---|---|---|---|
| `aot` | MCU (firmware) | per R2-COMPILE §4 (`xtensa-esp32s3-espidf`, `riscv32imac-esp-espidf`, …) | `no_std` static lib linked into the flashed image |
| `nif` | BEAM (workstation) | host triple | `std` cdylib via `r2-nif` wrapper |
| `web` | Browser (WASM hive) | `wasm32-unknown-unknown` | Static bundle served by R2-WEB per R2-PLUGIN §13 |

Each key **MUST** be either `false` (mode not supported) or an inline table:

```toml
[modes]
aot = { targets = ["esp32-s3", "esp32-c6"], no_std = true }
nif = false
web = false
```

For `web` mode (webapp plugins), `[modes.web]` **MAY** carry additional keys: `bundler` (default `"wasm-pack"`), `graphql_fragment` (path to schema fragment per R2-PLUGIN §13.7), `mount` (URL mount, default `/plugin/<plugin-name>`).

#### 5.3.3 `[commands]` — REQUIRED for plugins with a command interface

A flat map of `<command-name> = <opcode>` per R2-PLUGIN §12.3. Opcodes **MUST** be unique within a plugin. Each command **MUST** have a matching opcode constant in `src/lib.rs` (Stage 2).

#### 5.3.4 `[capabilities]` — REQUIRED

| Key | Type | Status | Notes |
|---|---|---|---|
| `provides` | array&lt;string&gt; | **REQUIRED** | Capability strings the plugin supplies (`ai.reality2.cap.*`). |
| `requires` | array&lt;string&gt; | **REQUIRED** | Capabilities the plugin needs at runtime (hardware via `r2.hw.*` or other plugins via `ai.reality2.*`). |

#### 5.3.5 `[events]` — REQUIRED

| Key | Type | Status | Notes |
|---|---|---|---|
| `handled` | array&lt;string&gt; | **REQUIRED** | Event-name strings the plugin processes. **MAY** be `[]` if the plugin is purely command-driven (method-style). |
| `emitted` | array&lt;string&gt; | **REQUIRED** | Event-name strings the plugin produces. **MAY** be `[]`. |

#### 5.3.6 `[credentials]` — RECOMMENDED

A descriptive table listing per-call credential requirements (KeyHolder, member, none). Format follows R2-PLUGIN §12.3 — verifiers consult this to pre-check capability before dispatching commands.

#### 5.3.7 `[notes]` — OPTIONAL

Free-form `gotchas` array (same convention as boards in §3.3.8).

#### 5.3.8 Validation

| Rule | Error |
|---|---|
| `plugin.name` equals the directory name | `E_PLUG_NAME` |
| `plugin.category` is in R2-PLUGIN §12.2 | `E_PLUG_CATEGORY` |
| `plugin.status` is `"ready"` for any plugin selected for a compile | `E_PLUG_NOT_READY` |
| At least one `[modes]` entry is **not** `false` | `E_PLUG_NO_MODE` |
| For Stage 2: `Cargo.toml` declares feature flags matching declared modes, mutually exclusive per R2-PLUGIN §12.5 | `E_PLUG_FEATURES` |
| For Stage 2: `src/lib.rs` exports a type implementing `r2_engine::plugin::Plugin` (aot/nif) and/or `wasm-bindgen` entry points (web) | `E_PLUG_TRAIT` |
| Every `[commands]` entry has a matching opcode constant in `src/lib.rs` (Stage 2) | `E_PLUG_COMMAND_CONST` |
| Every datasheet referenced in `PLUGIN.md` §7 exists under `datasheets/` (Stage 2) | `E_PLUG_DS` |
| For `web` mode (Stage 2): `assets/index.html` exists; no symlinks escape `assets/` | `E_PLUG_WEB_ASSETS` |

### 5.4 `PLUGIN.md` — REQUIRED narrative

Defers to R2-PLUGIN §12.8. PLUGIN.md **MUST** contain all 10 numbered sections in this exact order:

```
# <slug>
## 1. Purpose
## 2. Modes & Platforms
## 3. Events Handled
## 4. Events Emitted
## 5. Configuration
## 6. Example Sentants
## 7. Hardware / Host Requirements
## 8. Credentials
## 9. Known Limitations
## 10. Changelog
```

Sections **MAY** be amplified (e.g. `## 3. Events Handled (Inbound)`) but the numbering + order **MUST** be preserved.

### 5.5 `AI-CONTEXT.md` — REQUIRED fresh-CC brief

Opens with an `# H1` heading `AI-CONTEXT.md — <plugin slug>`.

REQUIRED `## H2` sections in this order:

1. **`## Purpose`** — one paragraph.
2. **`## Class + status`** — category, FNV hash, mode set, status (`metadata-only` / `ready`).
3. **`## Where the canonical artefact lives`** — `plugin.toml`.
4. **`## Capability surface`** — `[capabilities].provides` + `requires` summary.
5. **`## Where this plugin lives`** — ensemble-owned vs hive-shared-singleton vs core-crate; rationale.
6. **`## Vendor refs`** — datasheets / SDK references; filenames under `datasheets/` if present.
7. **`## Swap pairs`** — other plugins providing the same capability per R2-PLUGIN §10 (e.g. `lis2dh` for `adxl355`).
8. **`## Known gotchas`** — concise list.
9. **`## Read these files in this order (cold-start resume)`** — ordered file list.
10. **`## Authoring status`** — completion state, source-extraction status.

## 6. Sentants

### 6.1 Directory layout

```
catalogue/ensembles/<ensemble>/sentants/<Name>/
  sentant.yaml                # REQUIRED — canonical artefact (§6.3)
  SENTANT.md                  # REQUIRED — narrative (§6.4)
  AI-CONTEXT.md               # REQUIRED — fresh-CC brief (§6.5)
  examples/                   # OPTIONAL — example event sequences / FSM traces
  material/                   # OPTIONAL — raw uploads awaiting processing
  conversation/               # REQUIRED ≥1 — authoring transcripts
    YYYY-MM-DD-<topic>-NN.md
```

The directory name **MUST** be PascalCase and **MUST** equal `sentant.name`. Sentants live exclusively inside an ensemble; there is no top-level sentant tree.

### 6.2 Class string and hash

A sentant's R2 class string is the `class:` field in `sentant.yaml` (reverse-DNS per R2-CAP §3). The class hash is `FNV-1a-32(class_string_utf8_bytes)`; `sentant.yaml` **SHOULD NOT** include a hard-coded hash.

### 6.3 `sentant.yaml` — REQUIRED canonical artefact

Conforms to R2-DEF §2. The top-level key **MUST** be `sentant:`. The orchestrator **MUST** run R2-DEF §2 load-time validation in addition to the rules below.

#### 6.3.1 Keys under `sentant:` — REQUIRED

| Key | Type | Status | Notes |
|---|---|---|---|
| `name` | string | **REQUIRED** | PascalCase. **MUST** equal the directory name. |
| `class` | string | **REQUIRED** | Reverse-DNS class string per R2-CAP §3. |
| `description` | string | **REQUIRED** | Paragraph. |
| `storage` | string | **REQUIRED** | One of `ephemeral`, `durable`, `durable-state`. |
| `data` | mapping | **REQUIRED** if `storage = durable-state`, otherwise **OPTIONAL** | Initial values for state variables persisted across reboots. |
| `plugins` | array | **OPTIONAL** | Plugin references — each entry is `{name: <slug>}` OR `{capability: <cap-string>}` per R2-PLUGIN §10. **MAY** be absent for sentants that emit/consume only and don't drive plugins. |
| `automations` | array | **REQUIRED**, **MUST** have ≥1 entry | State machine definition per R2-DEF §2.4. Each automation **MUST** have a `name` (typically `main`) and a `transitions` array. |

#### 6.3.2 `automations[].transitions[]` schema

Each transition declares: `from`, `event`, `to`, optional `to_states` (multi-target), optional `actions[]`, optional `guards[]`. Actions support `{plugin, command, parameters}` for plugin calls and `{command: "send", parameters: {event, …}}` for bus emissions per R2-DEF §2.5.

#### 6.3.3 Validation

| Rule | Error |
|---|---|
| `sentant.name` equals the directory name | `E_SENT_NAME` |
| `sentant.class` follows R2-CAP §3 reverse-DNS convention | `E_SENT_CLASS` |
| `storage = durable-state` ⇒ `data` is present and non-empty | `E_SENT_DATA_MISSING` |
| Compilable subset per R2-COMPILE §3.1 (for AOT targets) — no API plugins, no dynamic sentant creation, no swarm loading | `E_SENT_NOT_COMPILABLE` |
| Every `plugin:` reference resolves to (this ensemble's `plugins/`) ∪ (any carrier's `plugins/`) ∪ (`crates/r2-plugin-*`) | `E_SENT_PLUGIN_UNRESOLVED` |
| Every `capability:` reference resolves to **at least one** plugin in scope providing that capability | `E_SENT_CAP_UNRESOLVED` |

### 6.4 `SENTANT.md` — REQUIRED narrative

A SENTANT.md **MUST** open with an `# H1` heading `<Name> sentant`.

The following `## H2` sections **MUST** appear in this order:

| Section | Status | Purpose |
|---|---|---|
| `## Purpose` | **REQUIRED** | One paragraph — what role this sentant plays in the ensemble. |
| `## FSM` | **REQUIRED** | State diagram + transition descriptions. **RECOMMENDED**: Mermaid `stateDiagram-v2`. If the sentant has no FSM beyond `start → idle` (one-shot / always-on / pure-emitter), state that explicitly. |
| `## Plugins used` | **REQUIRED** | One row per plugin or capability reference — by name or capability — with role. **MAY** be "(none — pure router)" for sentants that emit/consume only. |
| `## Events emitted / consumed` | **REQUIRED** | Lists with one line per event name + one-sentence purpose. |
| `## Reference` | **REQUIRED** | Pointers into upstream specs (R2-DEF, R2-WORKSHOP-SENSOR, etc.) and sibling sentants. |
| `## Authoring status` | **REQUIRED** | Completion state. |

The following sub-sections are **OPTIONAL** and **SHOULD** appear between `## Reference` and `## Authoring status` when relevant:

- `## Storage semantics` — for `durable-state` sentants, what's persisted and why.
- `## Context exposed` — for sentants that populate the hive context (Identity, …).
- `## Health behaviour` — for sentants with degraded-mode fall-backs.
- `## FSM gotchas` — non-obvious transition rules, framing edge-cases.
- `## Platform-extension tokens` — bindings into the per-carrier firmware (`{{platform.*}}` Tera tokens).
- `## AOT compilation notes` — sentant-specific R2-COMPILE §3.1 considerations.

### 6.5 `AI-CONTEXT.md` — REQUIRED fresh-CC brief

Opens with an `# H1` heading `AI-CONTEXT.md — <Name>`.

REQUIRED `## H2` sections in this order:

1. **`## Purpose`** — one paragraph.
2. **`## Class + storage`** — class string, FNV hash, storage mode, and initial `data` snapshot for durable-state sentants.
3. **`## Where the canonical artefact lives`** — `sentant.yaml`.
4. **`## FSM at a glance`** — state list + one-line trigger summary per transition.
5. **`## Plugins / capabilities used`** — name + role.
6. **`## Events`** — emitted + consumed lists.
7. **`## Substrate vs domain`** — explicitly flag whether this sentant is reusable substrate or deployment-specific domain logic (per the rocker-sensor convention).
8. **`## Known gotchas`** — concise list.
9. **`## Read these files in this order (cold-start resume)`** — ordered file list.
10. **`## Authoring status`** — completion state.

## 7. Authoring flow (normative — referenced from SPEC-R2-COMPOSER §7)

**This document IS the authoring brief.** §3 (Boards), §4 (Ensembles), §5 (Plugins), and §6 (Sentants) each define a kind's REQUIRED schema, file shape, narrative structure, and AI-CONTEXT.md structure. When the operator initiates `r2.composer.author.start{kind, description}`, the orchestrator's Author sentant (calling the `claude-code` plugin) constructs a brief from these sections and dispatches it to the AI — there is no separate authoring document.

When the operator initiates `r2.composer.author.start{kind, description}`, the Author sentant **MUST**:

1. **Create the entry directory** per §3.1 / §4.1 / §5.2 / §6.1 — empty, just the shell.
2. **Spawn `claude -p`** with the entry's parent directory as cwd + a Tera-rendered brief from `orchestrator/prompts/author-<kind>.md.tera`. The brief **MUST** splice in the matching SPEC-CATALOGUE-LAYOUT section verbatim — §3 for boards, §4 for ensembles, §5 for plugins, §6 for sentants — plus a pointer to one canonical example already in the catalogue to imitate.
3. **Stream the agent's clarifying questions** via `r2.composer.author.prompt` to the operator. Operator replies via `r2.composer.author.reply`.
4. **Allow the agent to use WebFetch** for vendor datasheets and Write for files under the new entry directory ONLY. Writes outside **MUST** be surfaced to the operator, not silently performed.
5. **Inside an ensemble authoring session**, the operator **MAY** further request "add a new plugin/sentant to this ensemble" — the Author sentant spawns a nested authoring flow scoped to `catalogue/ensembles/<name>/plugins/...` or `.../sentants/...`.
6. **On completion**, validate against the spec's validation table for the kind (§3.3.9 / §4.3.4 / §5.3.8 / §6.3.3). On failure, re-enter the loop with the validation error surfaced to the operator. On success, emit `r2.composer.author.done`.
7. **On error/abandonment**, run §8 (decommission) before emitting `r2.composer.author.error`.

### 7.1 Brief templates

| Kind | Template path | Spec section to splice |
|---|---|---|
| `board` | `orchestrator/prompts/author-board.md.tera` | §3 |
| `ensemble` | `orchestrator/prompts/author-ensemble.md.tera` | §4 |
| `plugin` | `orchestrator/prompts/author-plugin.md.tera` | §5 |
| `sentant` | `orchestrator/prompts/author-sentant.md.tera` | §6 |

Each brief **MUST** include:

- The full RFC 2119 schema for the kind (the relevant §3 / §4 / §5 / §6).
- A pointer to one canonical existing entry to imitate (board → `esp32-c6-dfr1117`, ensemble → `rocker-sensor`, plugin → `sensor/adxl355`, sentant → `Accelerometer`).
- The operator's request (free text — provided in the `description` field of `r2.composer.author.start`).
- The applicable upstream-R2 spec citations (R2-COMPILE §4 + R2-BUILD §2 for boards, R2-ENSEMBLE + R2-DEF §7 for ensembles, R2-PLUGIN §12 + §13 for plugins, R2-DEF §2 + R2-COMPILE §3.1 for sentants).
- A list of files the AI **MUST** produce, with a one-line purpose per file.

### 7.2 Scope enforcement

The plugin and sentant authoring flows are NEVER invoked at top level — they **MUST** be scoped to an existing ensemble (or, for hive-shared plugins, an existing carrier board). The Author sentant enforces this by examining `cwd` before dispatch and refuses to start a nested flow without an open scope.

## 8. Decommissioning

Removing a catalogue entry requires:

1. The entry has no live references — no score under [`../scores/`](../scores/) includes it (for ensembles); no ensemble depends on it (for boards); no other catalogue entry's `AI-CONTEXT.md` cites it.
2. A `conversation/YYYY-MM-DD-decommission-<topic>-NN.md` transcript documenting the reason.
3. `git rm -r catalogue/<branch>/<entry>/` — NOT soft-delete via renaming.

## 9. Conformance

A `catalogue/` tree conforms when:

1. Every entry directory follows §3.1 / §4.1 exactly.
2. Every entry passes the validation suite of §3.2 / §4.2 / §4.3 / §4.4.
3. Every entry has a `conversation/` directory with at least one transcript.
4. Every entry has an `AI-CONTEXT.md` matching §3.3 / §4.5.

The orchestrator's `catalogue plugin` MUST report any non-conforming entry as `degraded` in catalogue listings — degraded entries cannot be selected on the canvas until repaired.

---

## 10. Change log

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1 | Initial draft. Three-branch catalogue (boards/plugins/sentants). |
| 2026-05-31 | 0.2 | **Restructured to two-part canvas model** per `[[feedback-two-part-canvas]]`. Plugins and sentants are no longer top-level catalogue trees — they live inside ensembles (ensemble-owned) or boards (hive-shared singletons). Always-available infrastructure including the crypto plugin lives in `crates/`. |
| 2026-06-01 | 0.3 | **§3 (Boards) rewritten** as the canonical board-authoring spec, with RFC 2119 (BCP 14) normative keywords throughout. Full `board.toml` schema documented (every section + key with REQUIRED / OPTIONAL / CONDITIONAL status); canonical BOARD.md + AI-CONTEXT.md section structure; canonical `templates/` file list; expanded validation rules. Authored by reading the three existing carrier entries (esp32-s3-devkitc, esp32-s3-xiao, esp32-c6-dfr1117) and codifying the conventions they collectively used. Sweep of those three entries to bring them into full conformance committed alongside this revision. |
| 2026-06-01 | 0.4 | **§4 (Ensembles), §5 (Plugins), §6 (Sentants) rewritten** as canonical authoring specs alongside §3 (Boards), with RFC 2119 normative keywords throughout. Full `ensemble.yaml`, `plugin.toml`, `sentant.yaml` schemas documented. Canonical `ENSEMBLE.md` + `PLUGIN.md` (defers to R2-PLUGIN §12.8) + `SENTANT.md` section structures. Per-kind `AI-CONTEXT.md` structures. Plugin two-stage lifecycle (`metadata-only` vs `ready`) codified with a REQUIRED `[plugin].status` field. §7 (Authoring flow) rewritten to make the spec-as-brief connection explicit — the Author sentant splices the relevant SPEC-CATALOGUE-LAYOUT section into the Tera-rendered prompt, so this document IS the authoring instruction document for the AI. Renumbered §6 (Authoring) → §7, §7 (Decommissioning) → §8, §8 (Conformance) → §9, §9 (Change log) → §10. |
