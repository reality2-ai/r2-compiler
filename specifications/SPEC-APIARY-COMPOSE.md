# SPEC-APIARY-COMPOSE: canvas surface, target model, and compile fan-out for an apiary

**Version:** 0.1 Draft
**Date:** 2026-06-01
**Status:** Normative Draft
**Depends on:**
- **Companions in r2-composer/specifications/:** [`SPEC-APIARY-LAYOUT.md`](SPEC-APIARY-LAYOUT.md) (apiary directory + `apiary.toml` schema), [`SPEC-CATALOGUE-LAYOUT.md`](SPEC-CATALOGUE-LAYOUT.md) (boards / ensembles / plugins / sentants), [`SPEC-R2-COMPOSER.md`](SPEC-R2-COMPOSER.md) (event vocabulary, two-hive architecture)
- **Upstream R2:** R2-PLUGIN §10 (capability-based plugin swap), R2-ENSEMBLE §2.1.2 (hive-shared vs ensemble-owned), R2-COMPILE §4 (compile targets), R2-BUILD §2 (target triples), R2-TRUST §2.3 (one TG per hive), RFC 2119 + RFC 8174

## Conventions

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**, and **OPTIONAL** in this document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when, and only when, they appear in all capitals, as shown here.

---

## 1. Scope + Motivation

This spec defines:

- The **compose model** the canvas presents — an apiary as a tree of role-ensembles, each with one-or-more targets, each target a `(target_type, host, plugin_overrides)` tuple.
- The **target taxonomy** — MCU firmware / Native binary / BEAM release / WASM bundle.
- The **canvas surface** — how the tree is rendered and manipulated, with calls forward to compose-stack and transient overlays.
- **Compile fan-out** — events + semantics by which one operator action (`Compile`) produces N firmware/bundle artefacts.
- **`apiary.toml` extensions** to capture per-target plugin overrides + explicit target-types.

It builds on the multi-board-apiary insight (per [[project-multi-board-apiary-compose]] in memory): r2-workshop's rocker rig is **ONE apiary** (`nz.ac.auckland.rocker`) with **MULTIPLE role-ensembles** (sensor, controller, viewer, keyholder), and the SENSOR role alone compiles for **THREE carriers** (devkitc, xiao, dfr1117) with the appropriate accelerometer plugin per board. The current `{board, ensemble}` v0.1 canvas models a single `(role-ensemble, carrier)` tuple — useful as a stepping stone, but the destination is the apiary tree.

Out of scope (covered elsewhere):

- The apiary directory layout, top-level `apiary.toml` keys, TG management — see SPEC-APIARY-LAYOUT.
- Per-target stack visualisation (which protocol layers go into one specific artefact) — referenced in §10, formal spec follow-up.
- Transient runtime view (live mesh state) — referenced in §11, formal spec follow-up.

## 2. The compose tree

An apiary is composed by populating a four-level tree:

```
APIARY            (one — defines purpose, TG, class)
  │
  ├─ ROLE-ENSEMBLE    (1..N — each is a catalogue ensemble bound to a logical role)
  │     │
  │     ├─ TARGET     (1..M per role — each produces one compile artefact)
  │     │     │
  │     │     └─ plugin_overrides  (0..K — per-capability concrete plugin choices)
  │     │
  │     └─ …more targets…
  │
  └─ …more roles…
```

A **TARGET** is the unit of one compile invocation. The orchestrator's compose-step produces exactly one firmware/bundle artefact per target. An apiary with R role-ensembles, with `n_r` targets each, produces `Σ n_r` artefacts on Compile-all.

### 2.1 Apiary-level

Defined in `apiary.toml [apiary]` + `[tg]` per SPEC-APIARY-LAYOUT §3. The apiary's `class` string and TG bind every artefact produced from this apiary; **every** firmware/bundle artefact compiled from this apiary's tree is enrolled in the apiary's TG and carries the apiary's class hash in its R2-BEACON advertisements.

### 2.2 Role-ensembles

Each role-ensemble is a catalogue ensemble (per SPEC-CATALOGUE-LAYOUT §4) bound to a logical role within the apiary. The role string is human-readable (`"sensor"`, `"controller"`, `"viewer"`, `"keyholder"`, …) and **MUST** be unique within the apiary. The same catalogue ensemble **MAY** be reused for multiple roles by suffixing the role string (e.g. two `"keyholder-primary"` + `"keyholder-secondary"` roles both pointing at the `rocker-keyholder` ensemble).

### 2.3 Targets

A **target** is `(target_type, host, plugin_overrides)`:

- **`target_type`** — see §3.
- **`host`** — the concrete carrier board slug (for MCU targets) or host identifier (for native/BEAM/WASM targets). Resolves to either a `catalogue/boards/<slug>/` entry (MCU) or a logical host string (native: `linux-x86_64`, `darwin-arm64`; BEAM: `linux-x86_64`; WASM: `wasm32-browser`).
- **`plugin_overrides`** — a map `{<capability_string>: <plugin_slug>}` choosing which concrete plugin satisfies each capability requirement on THIS target. The role-ensemble declares capability *requirements* (per R2-PLUGIN §10's swap-lever pattern); the target's overrides choose which concrete plugin provides each. Targets that need no overrides use `{}`.

### 2.4 Co-located targets

When two targets share the same physical host (e.g. r2-workshop's linux box hosting both the controller hive AND the webapp-server hive), they **MUST** declare `co_located_with = "<other_role>"` so the orchestrator's deploy step knows to assemble both artefacts onto the one device. Co-located targets **MUST** use the same `host` identifier. Per R2-APIARY's tightly-bound case, co-located hives **MAY** share resources (one BLE radio, one filesystem mount) at deploy time — that's a deploy-step concern outside this spec.

## 3. Target types

Four target types are defined. The orchestrator **MUST** dispatch compile + deploy steps differently per type.

| Type | Slug | Toolchain | Output | Deploy target |
|---|---|---|---|---|
| **MCU firmware** | `mcu-fw` | `cargo build --target=<triple>` via espup / esp-rs / arm-none-eabi (per R2-COMPILE §4) | `.bin` + partition manifest, ready for `esptool write_flash` / OTA | Microcontroller carrier (ESP32-S3, ESP32-C6, nRF52, rp2040, …) |
| **Native binary** | `native` | `cargo build --release` on the host triple | Self-contained ELF / Mach-O / PE binary | Workstation / server (linux-x86_64, darwin-arm64, …) |
| **BEAM release** | `beam` | `mix release` (Elixir) or `rebar3 release` (Erlang) | Self-contained Erlang/OTP release | Workstation / server hosting the BEAM hive |
| **WASM bundle** | `wasm` | `wasm-pack build --target web` (or Trunk / esbuild equivalent) | `dist/` directory: `.wasm` + glue JS + static assets per R2-PLUGIN §13 | Browser; served by an R2-WEB plugin on a serving hive |

Target type **MAY** be inferred from the `host` slug when unambiguous (e.g. `esp32-*` → `mcu-fw`, `wasm32-*` → `wasm`) but **SHOULD** be declared explicitly in `apiary.toml` to disambiguate cases like linux-x86_64 (could be `native` OR `beam`).

### 3.1 Target-type-aware behaviour

| Concern | mcu-fw | native | beam | wasm |
|---|---|---|---|---|
| Cargo workspace member | yes | yes | no (Elixir project) | yes |
| `no_std` permitted | yes | no | n/a | yes |
| First-install path | USB flash via `esptool` (per [[project-compulsory-plugins-and-virgin-boards]]) | `cp ./bin/<slug> <host>:/opt/r2/` | `_build/prod/rel/<slug>` tar + extract | `dist/` upload to serving hive |
| OTA path | over WiFi via `r2.composer.deploy.ota` (post first-install) | systemd unit replace + restart | `mix release upgrade` hot-reload OR full restart | bundle replace on serving hive (HTTP cache bust) |
| Compulsory plugins | OTA (per SPEC-CATALOGUE-LAYOUT §3.3.7) | rolling-restart capability | hot-code-load capability | service-worker cache control (optional) |
| Stack visualisation surface | R2 protocol core + transports + storage + serving (R2-WEB if applicable) | similar, with native transports + native filesystem | similar, with BEAM transports + Mnesia/dets | R2 core (browser flavour) + DOM transport + IndexedDB |

## 4. `apiary.toml` extension

The existing `[[role_ensembles]]` shape from SPEC-APIARY-LAYOUT §3 carries a `carriers` array; this spec extends it to fully describe per-target details. **Both shapes MUST be supported** by the orchestrator; the simple `carriers` array is shorthand that expands to one target per carrier with `target_type` inferred and `plugin_overrides = {}`.

### 4.1 Simple form (existing — SPEC-APIARY-LAYOUT §3)

```toml
[[role_ensembles]]
role     = "sensor"
ensemble = "rocker-sensor"
carriers = ["esp32-s3-devkitc", "esp32-s3-xiao", "esp32-c6-dfr1117"]
```

The orchestrator expands this to three targets, each `target_type = "mcu-fw"` (inferred), `host = <carrier>`, `plugin_overrides = {}` (the ensemble's default plugin set applies).

### 4.2 Full form (this spec)

```toml
[[role_ensembles]]
role     = "sensor"
ensemble = "rocker-sensor"

[[role_ensembles.targets]]
target_type      = "mcu-fw"
host             = "esp32-s3-devkitc"
plugin_overrides = { "ai.reality2.cap.accel.triaxial" = "adxl355" }

[[role_ensembles.targets]]
target_type      = "mcu-fw"
host             = "esp32-s3-xiao"
plugin_overrides = { "ai.reality2.cap.accel.triaxial" = "adxl355" }

[[role_ensembles.targets]]
target_type      = "mcu-fw"
host             = "esp32-c6-dfr1117"
plugin_overrides = { "ai.reality2.cap.accel.triaxial" = "lis2dh" }   # swap per R2-PLUGIN §10
```

```toml
[[role_ensembles]]
role     = "controller"
ensemble = "rocker-controller"

[[role_ensembles.targets]]
target_type     = "native"
host            = "linux-x86_64"
device_count_planned = 1

[[role_ensembles]]
role     = "webapp-server"
ensemble = "rocker-webapp-server"

[[role_ensembles.targets]]
target_type      = "beam"
host             = "linux-x86_64"
co_located_with  = "controller"      # shares the linux box with the controller role
device_count_planned = 1

[[role_ensembles]]
role     = "viewer"
ensemble = "rocker-viewer"

[[role_ensembles.targets]]
target_type      = "wasm"
host             = "wasm32-browser"
device_count_planned = 0             # unbounded — browsers join via QR/link
```

### 4.3 Mixing forms

A single `[[role_ensembles]]` block **MUST** use **either** the simple `carriers = [...]` shorthand **OR** the `[[role_ensembles.targets]]` blocks — never both. The orchestrator **MUST** reject mixed-shape entries with `E_APIARY_ROLE_SHAPE`.

### 4.4 New validation rules (in addition to SPEC-APIARY-LAYOUT §3.1)

| Rule | Error |
|---|---|
| Every `[[role_ensembles.targets]]` entry has a `target_type` from the §3 taxonomy | `E_APIARY_TARGET_TYPE` |
| Every target's `host` resolves: MCU → catalogue/boards/, native/beam → declared host string, wasm → `wasm32-browser` | `E_APIARY_TARGET_HOST` |
| Every target's role-ensemble's `compile_target` overlaps with the target's `host` (or its derived tag) | `E_APIARY_TARGET_MISMATCH` |
| Every `plugin_overrides` capability is in the role-ensemble's `capabilities.requires` | `E_APIARY_OVERRIDE_UNKNOWN_CAP` |
| Every `plugin_overrides` plugin slug resolves under (this ensemble's plugins/) ∪ (the carrier's plugins/) ∪ (`crates/r2-plugin-*`) | `E_APIARY_OVERRIDE_UNRESOLVED` |
| When `co_located_with` is set, the target role MUST exist + the target's `host` MUST equal that role's `host` | `E_APIARY_COLOC_HOST` |
| The role string is unique within the apiary | `E_APIARY_ROLE_DUPLICATE` |

## 5. The canvas surface

The canvas is r2-composer's primary composition surface (per [[project-hybrid-canvas-chat-ux]]: canvas instructs the AI). For an apiary, the canvas **MUST** render the compose tree as a hierarchy with three drill levels.

### 5.1 Apiary header

Persistent at the top of the canvas regardless of drill state:

```
Apiary: <name>           TG: <class string + first 8 chars of hash>      [⚙ apiary settings]
                         Active KeyHolder: <fingerprint>                  [↻ TG status]
```

The apiary header is the calm-computing anchor — visible at a glance, never moves.

### 5.2 Role-ensemble cards

The body of the canvas is a list of role-ensemble cards, one per `[[role_ensembles]]`. Each card shows:

- Role name + ensemble slug + ensemble class hash
- Target count: `3 targets` (sensor) or `1 target` (controller)
- Per-target one-line summary: `esp32-s3-devkitc · adxl355` / `esp32-c6-dfr1117 · lis2dh`
- Status chips per target: `metadata-only` / `ready` / `built` / `flashed`
- An expand affordance to drill into target detail (§5.3)

```
┌─ SENSOR ─────────────────────────────────── rocker-sensor (15 sentants) ─┐
│   ✓ esp32-s3-devkitc   accel=adxl355    [ready]    [last built: 2h ago] │
│   ✓ esp32-s3-xiao      accel=adxl355    [ready]    [last built: 2h ago] │
│   ⚠ esp32-c6-dfr1117   accel=lis2dh     [overrides unresolved]          │
│   ＋ add target                                                          │
└──────────────────────────────────────────────────────────────────────────┘
┌─ CONTROLLER ──────────────────────── rocker-controller (native, 1 host) ─┐
│   ✓ linux-x86_64                        [ready]                          │
│   ＋ add target                                                          │
└──────────────────────────────────────────────────────────────────────────┘
┌─ WEBAPP-SERVER ─────────────────── rocker-webapp-server (BEAM, 1 host) ──┐
│   ✓ linux-x86_64   co-located with controller   [ready]                  │
└──────────────────────────────────────────────────────────────────────────┘
┌─ VIEWER ──────────────────────────────── rocker-viewer (WASM bundle) ────┐
│   ✓ wasm32-browser                      [ready]                          │
└──────────────────────────────────────────────────────────────────────────┘
                                                                  [Compile all →]
```

The card layout **SHOULD** sort roles by deploy-priority (sensors first, then controllers, then webapp-server, then viewers, then keyholders) — the operator's typical mental order. Authoring order **MAY** override.

### 5.3 Target drill-in

Clicking a target opens a per-target detail surface showing:

- The target's `(type, host, plugin_overrides)`.
- The ensemble's `capabilities.requires` with the chosen concrete plugin per capability (and swap-pair alternatives per R2-PLUGIN §10).
- The full **compose-stack visualisation** for this target (Phase 2-canvas-b — formal spec follow-up). This is where REQUIRED/OPTIONAL/UNAVAILABLE protocol layers + transports + storage + serving render as toggleable items, carrier-aware.
- The latest build artefact path + hash + timestamp + deploy state.
- A `Build this target` action that fires a single `r2.composer.target.build.start` (see §6).

### 5.4 Transient overlay

When the apiary has any deployed instances (per `devices/roster.toml` per SPEC-APIARY-LAYOUT §5), each role-ensemble card **SHOULD** show a transient strip beneath it summarising live state per device: TG status, last-beacon-seen, link state, latest sentant FSM snapshot. The transient strip is the calm-computing entry point into the full transient view (Phase 2-canvas-c — formal spec follow-up). When no devices are deployed, the strip is absent.

### 5.5 Canvas + chat coupling

Per the hybrid-canvas UX, every chat prompt **MUST** include the current compose-tree snapshot as context. The brief constructed by the Author sentant (per SPEC-R2-COMPOSER §7) **MUST** serialise:

- The apiary identity (class + TG fingerprint).
- The compose tree (roles + targets + plugin overrides).
- The selected drill level (which role-ensemble / target the operator is focused on).
- The chat history so far.
- The new user message.

A target-level drill-in chat **MAY** scope the chat to that target specifically (the prompt brief includes only that target's section of the tree).

## 6. Compile fan-out

### 6.1 Event vocabulary

| Event | Direction | Payload | Purpose |
|---|---|---|---|
| `r2.composer.apiary.build.start` | webapp → orchestrator | `{ roles: [<role>...] \| null }` (null = all roles) | Operator clicked "Compile all" or "Compile this role" |
| `r2.composer.apiary.build.plan` | orchestrator → webapp | `{ targets: [{ role, host, target_id }] }` | Fan-out plan emitted before any per-target work — webapp uses it to render progress slots |
| `r2.composer.target.build.start` | orchestrator-internal (sentant → plugin) | `{ target_id, role, host, target_type, plugin_overrides, ... }` | Per-target sub-flow trigger |
| `r2.composer.target.build.progress` | orchestrator → webapp | `{ target_id, phase, text, kind }` | Streamed claude-code output per target |
| `r2.composer.target.build.done` | orchestrator → webapp | `{ target_id, artefact_path, sha256, duration_ms }` | One target finished successfully |
| `r2.composer.target.build.error` | orchestrator → webapp | `{ target_id, phase, message }` | One target failed |
| `r2.composer.apiary.build.done` | orchestrator → webapp | `{ ok: <count>, errored: <count>, artefacts: [...] }` | All targets resolved (success and failure both count) |

The single-target debug path from v0.1 (`r2.composer.build.start`) **MAY** remain as a shorthand that maps onto one `r2.composer.target.build.start`. The apiary-level events **SHOULD** be preferred once the apiary canvas lands.

### 6.2 Fan-out semantics

On `r2.composer.apiary.build.start`:

1. The orchestrator's Builder sentant resolves the compose tree against the apiary state (`apiary.toml`, the catalogue, plugin overrides).
2. Emits `r2.composer.apiary.build.plan` listing each target with a stable `target_id` (e.g. `<role>:<host>`).
3. For each target, dispatches `r2.composer.target.build.start` to a per-target claude-code plugin instance (or queues against a shared pool — implementation choice).
4. Per-target sub-flows run in parallel **subject to** a concurrency cap (default 2; configurable per [[feedback-aot-optimisation-constraint]] flash-budget concerns).
5. Each sub-flow streams its own `target.build.progress` / `target.build.done` / `target.build.error` events tagged by `target_id`.
6. When all targets resolve (success or error), emit `r2.composer.apiary.build.done` with the aggregate.
7. On any per-target error, the apiary build is **partially** successful. Operator decides whether to retry the failed target or proceed to deploy.

### 6.3 Per-target work product

Each successful target build emits an artefact at `apiaries/<apiary>/out/<role>-<host>-<timestamp>/` per SPEC-APIARY-LAYOUT §2. The artefact directory **MUST** contain:

- For `mcu-fw`: `firmware.bin`, `partitions.csv`, `metadata.toml` (sha256, target tag, board slug, ensemble version, plugin overrides).
- For `native`: `<role>-<host>` (the binary), `metadata.toml`.
- For `beam`: `<role>-<host>-<version>.tar.gz` (the release), `metadata.toml`.
- For `wasm`: `dist/` (the bundle), `metadata.toml`.

The `metadata.toml` is the deploy-step's authoritative source for what's about to be flashed/installed.

## 7. Build briefs (extension to SPEC-CATALOGUE-LAYOUT §7)

Per the spec-as-brief pattern, the orchestrator's per-target build prompts splice in:

- The role-ensemble's `ensemble.yaml`.
- The target's `host`-side `board.toml` (for MCU) or host-platform notes (for native/beam/wasm).
- The plugin overrides resolved to concrete plugin entries (`plugin.toml` + `PLUGIN.md` for each).
- The apiary's class + TG context.
- The relevant SPEC-CATALOGUE-LAYOUT sections (§3 for boards, §4 for ensembles, §5 for plugins).

This is the spec-as-brief pattern from SPEC-CATALOGUE-LAYOUT §7 applied at compose time: the AI's brief contains the full normative context for producing one conformant artefact.

## 8. The single-target stepping stone (v0.1)

The v0.1 `{board, ensemble}` canvas (per [[project-hybrid-canvas-chat-ux]]) is a **stepping stone** that exercises one `(role-ensemble, target)` tuple in isolation. It **SHOULD** be preserved as a debug entry point even after the apiary canvas lands — useful for catalogue authoring (compile one plugin's example sentant against one board) and for new-carrier bring-up (compile a known-good ensemble against an untested board).

The v0.1 canvas's `Compile` action dispatches the legacy `r2.composer.build.start` event; this spec leaves that path intact and adds the new apiary-level events alongside.

## 9. Conformance

A canvas implementation conforms to this spec when:

1. It renders the apiary header (§5.1) persistent regardless of drill state.
2. It renders one role-ensemble card per `[[role_ensembles]]` entry (§5.2).
3. It renders per-target detail on drill-in (§5.3) including the chosen plugin per capability.
4. It serialises the compose tree into every chat brief per §5.5.
5. It dispatches `r2.composer.apiary.build.start` on the Compile-all action and renders progress per the §6 event vocabulary.
6. It handles partial failures per §6.2 step 7 (per-target retry path).

A `apiary.toml` extension conforms when:

1. Every `[[role_ensembles]]` block uses either the simple `carriers` shorthand or the full `[[role_ensembles.targets]]` form, never both.
2. Every target validates per §4.4.
3. Every `plugin_overrides` capability is in the matching ensemble's `capabilities.requires`.

## 10. Per-target stack visualisation (forward reference)

When the operator drills into a target (§5.3), the per-target detail surface **SHOULD** render the **compose-stack visualisation** — a layered diagram of the R2 protocol stack + transports + storage + serving layers for THAT specific target, carrier-aware. Layer states: REQUIRED-by-board / REQUIRED-by-ensemble / OPTIONAL (toggleable) / UNAVAILABLE.

The stack visualisation's formal data model + UI conventions are a separate spec (target: `SPEC-COMPOSE-STACK.md` or extension to this doc). For v0.2 implementation: render a static stack list (no toggling) showing what gets compiled in.

## 11. Transient view (forward reference)

The transient overlay (§5.4) is the calm-computing entry point to a full transient-mesh view — TG membership, BEACON advertisements, WIRE links, ROUTE topology, sentant FSM states, transport status, OTA progress, per running hive in the apiary's TG. Live data sourced via R2-DASH (`r2.dash.cmd.status`, `r2.dash.cmd.list_sentants`, …) once the orchestrator's r2-composer hive joins the apiary's TG.

Formal spec: `SPEC-TRANSIENT-VIEW.md` (TBD). For v0.2: mock-data skeleton; the schema follows R2-DASH responses verbatim.

## 12. Worked example: r2-workshop's rocker rig

Given r2-workshop's published structure, the full `apiary.toml` would be:

```toml
[apiary]
name        = "rocker-rig"
description = "The Reality2 workshop rocker — accelerometer-driven rocker chair people-counter."
class       = "nz.ac.auckland.rocker"
version     = "0.2.0"
created     = "2026-05-01T10:00:00Z"

[tg]
public_key     = "trust_keys/tg_pub.bin"
keyholder_path = "~/.config/r2-composer/apiaries/rocker-rig/tg_signer/"
keyholder_fp   = "<sha256>"

# Sensor role: 3 MCU targets, accelerometer swapped per board
[[role_ensembles]]
role     = "sensor"
ensemble = "rocker-sensor"

[[role_ensembles.targets]]
target_type      = "mcu-fw"
host             = "esp32-s3-devkitc"
plugin_overrides = { "ai.reality2.cap.accel.triaxial" = "adxl355" }

[[role_ensembles.targets]]
target_type      = "mcu-fw"
host             = "esp32-s3-xiao"
plugin_overrides = { "ai.reality2.cap.accel.triaxial" = "adxl355" }

[[role_ensembles.targets]]
target_type      = "mcu-fw"
host             = "esp32-c6-dfr1117"
plugin_overrides = { "ai.reality2.cap.accel.triaxial" = "lis2dh" }

# Controller role: native binary on the workshop linux box
[[role_ensembles]]
role     = "controller"
ensemble = "rocker-controller"

[[role_ensembles.targets]]
target_type = "native"
host        = "linux-x86_64"

# Webapp-server role: BEAM release on the SAME linux box as the controller
[[role_ensembles]]
role     = "webapp-server"
ensemble = "rocker-webapp-server"

[[role_ensembles.targets]]
target_type     = "beam"
host            = "linux-x86_64"
co_located_with = "controller"

# Viewer role: browser WASM bundle (multiple instances, served by webapp-server)
[[role_ensembles]]
role     = "viewer"
ensemble = "rocker-viewer"

[[role_ensembles.targets]]
target_type = "wasm"
host        = "wasm32-browser"

# Keyholder role: separate tag device for enrolment
[[role_ensembles]]
role     = "keyholder"
ensemble = "rocker-keyholder"

[[role_ensembles.targets]]
target_type = "mcu-fw"
host        = "esp32-s3-keyholder-tag"
```

**Compile-all** fan-out for this apiary produces SIX artefacts:

| target_id | type | host | output |
|---|---|---|---|
| `sensor:esp32-s3-devkitc` | mcu-fw | esp32-s3-devkitc | `firmware.bin` (adxl355) |
| `sensor:esp32-s3-xiao` | mcu-fw | esp32-s3-xiao | `firmware.bin` (adxl355) |
| `sensor:esp32-c6-dfr1117` | mcu-fw | esp32-c6-dfr1117 | `firmware.bin` (lis2dh) |
| `controller:linux-x86_64` | native | linux-x86_64 | `controller` binary |
| `webapp-server:linux-x86_64` | beam | linux-x86_64 | release tarball, co-located with controller |
| `viewer:wasm32-browser` | wasm | wasm32-browser | `dist/` bundle |

Deploy then assembles these onto their devices — three sensors via `esptool` first-flash + WiFi OTA thereafter, controller + webapp-server side-by-side on the linux box, the WASM bundle served from the webapp-server, the keyholder tag flashed once.

## 13. Change log

| Date | Version | Change |
|---|---|---|
| 2026-06-01 | 0.1 | Initial draft. Defines the compose tree (apiary → role-ensembles → targets → plugin-overrides), the four target types (mcu-fw / native / beam / wasm), the canvas surface (apiary header + role cards + target drill-in + transient overlay), compile fan-out events (`r2.composer.apiary.build.*` + `r2.composer.target.build.*`), and an `apiary.toml` extension (`[[role_ensembles.targets]]` blocks). Worked example: r2-workshop's rocker rig as a six-artefact compile fan-out. |
