# PLAN.md

Current phasing. This file is overwritten as work progresses (PROCESS.md §3); the per-session transcripts in [`../conversation/`](../conversation/) and per-entry `conversation/` dirs accumulate.

## Status (2026-05-31)

**Phase 0 — scaffolding.** ✅ Complete.
**Phase 0.5 — GitHub repo + initial push.** ✅ Complete — https://github.com/reality2-ai/r2-composer
**Phase 1.1 — `tools/sync-catalogue.sh`.** ✅ Complete.
**Phase 1.2 — first sync run.** ✅ Complete — see `crates/_VERSIONS.toml` for the manifest.
**Phase 1.3 — author all three board entries.** ✅ Complete — board.toml + BOARD.md + AI-CONTEXT.md for esp32-s3-devkitc, esp32-s3-xiao, esp32-c6-dfr1117.
**Phase 1.4-metadata (first slice).** ✅ Complete — `[compulsory_plugins]` added to all three board.tomls; SPEC-R2-COMPOSER §11 (TG management) + §12 (device lifecycle + deploy paths + compulsory plugins) added; SPEC-CATALOGUE-LAYOUT §4.3 amended with three modes (aot/nif/web); first worked-example plugin (`sensor/lis2dh`) and sentant (`Identity`) fully authored as metadata.
**Phase 1.4-source (first slice).** ✅ Complete — `r2-plugin-sensor-lis2dh` Cargo crate authored from the r2-workshop reference: `no_std` for AOT, generic over `embedded_hal::i2c::I2c`, implements `r2_engine::plugin::Plugin`, **9 unit tests passing** via `embedded-hal-mock`. The vendored R2 crates (r2-engine, r2-fnv, r2-cbor, r2-wire, plus r2-dispatch added by sync) now form a host-buildable workspace. Conjecture **C-5 (the plugin authoring pattern produces a buildable crate) survives its first test.** Workspace deps tidied; terminology corrected throughout (compiler/author/flasher/sync are PLUGINS not sentants, per [[feedback-sentants-vs-plugins-terminology]]).
**Phase 1.4-source for NVS.** ✅ Complete — `crates/r2-plugin-storage-nvs` authored: generic over a `KvStore` trait (no upstream embedded-hal equivalent for NVS, so the plugin defines its own minimal trait surface), `no_std` for AOT, `InMemoryKvStore` for tests under `std`. Implements R2-PLUGIN §12 with 6 commands (init / read / write / erase / list / commit). **12 unit tests passing.** Identity sentant's `E_SENT_PLUGIN_UNRESOLVED` flag now cleared (both `nvs` + `software-ed25519` plugin references resolve to vendored core crates). Conjecture **C-5 survives second test.**
**Phase 2-preview.** ✅ Complete — minimal browseable static webapp (`webapp/index.html` + `webapp/ui/app.js` + `webapp/styles/main.css`) reads `webapp/dist/manifest.json` (built by `tools/build-catalogue-index.py`) and renders the catalogue: boards panel left, ensembles panel right (with nested plugins + sentants), workspace centre with entry detail + file viewer. Apiary placeholder pill in header. Launch with `webapp/run.sh` → `http://localhost:8080/webapp/`. No WASM hive yet — Phase 2-full is the real visual canvas.
**Second pass (2026-05-31).** ✅ Complete — three coupled decisions ratified:
1. **Core/opt-in reclassification:** `nvs` + `clock` moved from `catalogue/` to `crates/` as always-on core plugins (`crates/r2-plugin-storage-nvs/`, `crates/r2-plugin-time-clock/`). Final core-plugin set in `crates/`: software-ed25519, ble-beacon, ble-l2cap, ota-tcp (switchable), nvs, clock = 6 plugins. Catalogue retains the 8 opt-in plugins (adxl355, lis2dh, battery-adc, sd-card, led, log-tcp, data-tcp, reset-tcp).
2. **Naming "apiary":** broaden R2-APIARY upstream to encompass TG-bound multi-hive deployments (the operator's unit of work); the current multi-processor-single-identity case becomes a labeled specialisation. New `specifications/SPEC-APIARY-LAYOUT.md` (v0.1) defines the r2-composer-side contract (`apiaries/<name>/` + `apiary.toml`). New `specifications/SPEC-APIARY-AMENDMENT-PROPOSAL.md` (v0.1) is the upstream amendment proposal awaiting Roy's ratification.
3. **r2-composer is itself structurally an R2 ensemble** (class `ai.reality2.ensemble.r2-composer`) but at runtime its hives inherit the active apiary's TG context (no standing r2-composer TG; honours R2-TRUST §2.3). `meta/` self-description deferred to Phase 1.6+ (when the orchestrator + webapp Rust sources are authored). SPEC-R2-COMPOSER §13 + §14 added.
**Phase 2 WASM foundation.** ✅ Complete — `webapp/crate/` Rust crate compiled to `wasm32-unknown-unknown` via `webapp/build-wasm.sh` (wraps `wasm-pack build --target web`). Exposes class-hash computation (`fnv1a_32`, `class_hash_hex`, `verify_class_hash`, `version`) via wasm-bindgen — every class string in the UI now shows its FNV-1a-32 hash next to it (the WASM module uses the vendored `r2-fnv` crate). 5/5 host-side unit tests passing. The rocker class `nz.ac.auckland.rocker` hashes to `0x624c47bc` — matches SPEC-R2-WORKSHOP-ENSEMBLE §2.1 byte-for-byte. WASM bundle (36 KB, .gitignored override) is committed for direct GitHub-Pages serving. Phase 2-full's R2 hive in the browser builds on this foundation.
**Phase 1.4-source for clock.** ✅ Complete — `crates/r2-plugin-time-clock` authored: generic over a `Clock` trait (platform tick source), `no_std` for AOT, `InMemoryClock` for tests. Implements R2-PLUGIN §12 with 5 commands (init / monotonic_ms / now_ms / set_offset / get_offset). **10 unit tests passing.** Saturating arithmetic on `now_ms` clamps negative-offset edges. Conjecture **C-5 survives third test** — the platform-abstract trait pattern (matched between lis2dh `embedded_hal::i2c::I2c`, nvs custom `KvStore` trait, and clock `Clock` trait) is repeatable.
**Phase 1.6 orchestrator scaffolding.** ✅ Complete — `orchestrator/` Rust binary serving the webapp + opening a `/r2` WebSocket. Routes: `/`, `/health`, `/webapp/...`, `/catalogue/...`, `/crates/...`, `/scores/...`, `/apiaries/...`, `/r2` (WS). axum 0.8 + tokio + tower-http; structured logging via `tracing`; graceful Ctrl-C / SIGTERM. The `/r2` endpoint is a Phase 1.6 stub (accepts connections, echoes acks); Phase 1.7+ wires the real R2-WIRE event bus + plugin set per SPEC-R2-COMPOSER §3.3. Smoke-tested: all routes return 200; webapp loads through the orchestrator unchanged. `orchestrator/run.sh` is a drop-in replacement for `webapp/run.sh` for operators who want the `/r2` endpoint.
**Phase 1.7a r2-engine wired into the orchestrator.** ✅ Complete — `r2-engine::EventBus` running on a dedicated OS thread (per the r2-forge pattern; the bus is `!Send`-friendly). mpsc/broadcast channel bridges between the async axum WS handler and the synchronous engine. JSON envelope ↔ `QueuedEvent` translation via `bridge.rs` (FNV-hashing event names; registry of 38 known `r2.composer.*` names). First sentant landed: `BuilderSentant` (Idle/Working FSM) subscribes to `r2.composer.build.start` and emits 3 synthetic `r2.composer.build.progress` events + 1 `r2.composer.build.done` per build request. **5/5 orchestrator unit tests pass** (bridge round-trip + builder behaviour). **End-to-end WS round-trip verified:** browser → /r2 → bridge → engine → BuilderSentant → 3 progress events + 1 done → bridge → /r2 → browser. Phase 1.7b wires in the real `claude-code` plugin so the progress events come from a `claude -p` subprocess rather than synthesised text.
**F-series — substrate buildout (L0–L6).** The device-scoped, always-running, TG-agnostic core of the hive ([[reference-r2-substrate]]), authored in `orchestrator/src/substrate/`. Tracks the stack-view (S-series) work in the webapp.
- **F3** ✅ — `keyholder` + `provision` substrate components + `Provision` sentant: cert + `#wifi_offer` chain (`e2fcfa8`).
- **F4** ✅ — `beacon-observer`: BLE scan + R2-BEACON parse → `beacon_observed` (`ae90144`).
- **F4b** ✅ — `provision_handshake`: L2CAP CoC + R2-PROVISION join; retired the F3 fake cert (`01e75d1`).
- **F4c** ✅ — `tg_state`: persist the apiary `TrustGroup` (members, revocations, sequence counter) across orchestrator restarts per R2-TRUST §5.6. New `orchestrator/src/substrate/tg_state.rs` — versioned binary format (`R2TG` v1), atomic write-temp+rename, in-tree under `apiaries/<name>/devices/tg_state.bin` (public cert/revocation material only; signing key stays off-tree). `run_handshake` now restores the TG from disk and persists it after a member joins, **before** the device sees the JoinResponse — keeping the GROUP_MGMT sequence counter monotonic so a restart can't collide with member-side replay protection. Corrupt/version-mismatched state is a hard failure, not a silent rebuild. **5/5 tg_state unit tests pass.** Closes the restart-amnesia gap (without it every restart forgot enrolments, lost revocations, reset the sequence).
- **F5** ✅ — `ota_push`: L6 Management — wire-v1 OTA firmware push over TCP/21043 per **R2-UPDATE §3.1.2.2**. New `orchestrator/src/substrate/ota_push.rs` (worker-thread + `poll()`, same shape as `composer/flasher`): connects, sends `[0x01][size u32 LE][sha256 32 raw bytes][firmware…]` + half-close, parses the `[status u8][len u16 LE][utf8]` reply, streams `deploy.device.progress` (connecting→sending→awaiting-ack→rebooting) → `deploy.device.done`/`.error`, and appends `deploy_log.jsonl` (§6.5). **The wire was corrected during F5:** SPEC-APIARY-FLASH §6.2/§6.4 wrongly described a text-line `[u32 BE len]`/`OK <sha>`/`REBOOTING` shape; both the specs peer (R2-UPDATE §3.1.2.2) and r2-workshop's device source (`crates/r2-esp/src/ota_tcp.rs`, via the workshop peer) independently confirmed the real binary wire (LE size, raw-byte SHA, half-close EOF, no `REBOOTING` frame). Spec amended to match. **Deploy sentant extended** with `deploy.batch.start`: §6.1 reachability gate (refuses non-`reachable` rows) + §6.3 sequential queue (one device at a time, advances on each `device.done`/`.error`, emits `deploy.batch.done`). Wired into `hive.rs` (`substrate/ota-push`). **12 new tests** (9 ota_push incl. a byte-exact wire-conformance test against a loopback fake device; 3 deploy batch). **102/102 orchestrator tests pass.** Deferred to **F5b**: post-OTA roster `firmware_sha` update; parallel batch pushes (the single plugin instance serialises, so F5 is always sequential).
- **F5b** ✅ — post-OTA confirmation → roster `firmware_sha`. **Investigation found the originally-planned mechanism (90 s beacon-confirm carrying the new firmware sha) is infeasible:** R2-BEACON's Legacy 28-byte AD carries no firmware identity, R2-UPDATE confirms via TG-scoped `r2.update.applied`/`rollback`/`rejected` events (§3.1.4/§7.1) not beacons, and current r2-workshop wire-v1 devices emit none of those (they just `esp_restart()`). Per a Roy ruling, v0.1 adopts the **ack-as-confirmation** model: the device returns `STATUS_OK` only after streaming the firmware through SHA-256 and matching the preamble hash, so the ack cryptographically proves the written bytes. **Roster sentant** now subscribes to `deploy.device.done` (Plugin-sourced only, mirroring the `first_install.done` guard) and records `firmware_sha` = the pushed sha + a `flashed_ota` history entry, leaving `state` unchanged (`reachable`) and emitting `device.entry` so the webapp row updates. `firmware_ver` left unset (no semantic version exists in the v0.1 build pipeline yet). SPEC-APIARY-FLASH §6.2 steps 5–7 rewritten to the ack model (dated correction note); §12 forward-path gains the R2-UPDATE event model (true running-version + rollback detection), `firmware_ver` population, and the enrolled→reachable beacon-liveness state machine. **3 new tests** (2 roster: records-sha + local-source-ignored; the e2e bus test extended to register the Roster sentant and assert `firmware_sha` lands end-to-end). **105/105 orchestrator tests pass.** Rollback detection remains out of reach until the R2-UPDATE event model lands (needs r2-workshop firmware changes).

**Phase 1.7b claude-code plugin.** ✅ Complete — first real plugin in the orchestrator. `orchestrator/src/plugins/claude_code.rs` is a subprocess driver: `Plugin::execute(CMD_START, brief)` spawns `claude -p '--output-format=stream-json'` in worker threads (writer to stdin, reader on stdout, wait-and-report-exit); `Plugin::poll()` drains the worker channel one message at a time and returns each as a `(event_hash, payload)` tuple. Stream-json `type` field surfaces in the progress payload's `kind`. Non-zero exit code → `r2.composer.build.error`; clean exit → `r2.composer.build.done`. **BuilderSentant updated** to dispatch via `Action::PluginCall` instead of synthesising text; on plugin-sourced progress/done/error events it re-broadcasts with `Target::Broadcast` so the outbound queue + WS layer pick them up. Loop-prevention via `EventSource::Plugin(_)` guard. **14/14 orchestrator unit tests pass** (5 plugin tests including `sh -c printf '...'` stream-json fixture, 3 bridge, 6 builder). Conjecture **C-1 (Claude Code drives an autonomous build cycle) survives its first test** — subprocess spawn + stream parsing + per-line event surface all work end-to-end with a printf-fixture command; real `claude -p` invocation just changes the command path.

```
✅ AGENTS.md / AI-CONTEXT.md / README.md / PROCESS.md
✅ specifications/SPEC-R2-COMPOSER.md  (v0.1)
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
✅ conversation/2026-05-31-r2-composer-design-01.md
```

Two boards (devkitc, xiao) still need `board.toml` + `BOARD.md` — same pattern as the dfr1117 entry, manually-authored as practice runs before the authoring flow exists. The rocker-sensor ensemble still needs per-plugin and per-sentant entries scaffolded under its own directory (Phase 1.3 below).

## Phase 1 — catalogue seed + spec-driven build path

Goal: round-trip the three r2-workshop carriers per [`SPEC-R2-COMPOSER.md`](../specifications/SPEC-R2-COMPOSER.md) §6.

| Step | Output | Dep |
|---|---|---|
| 1.1 | `tools/sync-catalogue.sh` — script to populate `crates/`, `catalogue/boards/<each>/templates/`, `catalogue/ensembles/rocker-sensor/ensemble.yaml` from sibling repos | — | ✅ |
| 1.2 | First sync run | 1.1 | ✅ |
| 1.3 | `board.toml` + `BOARD.md` + completed `AI-CONTEXT.md` for all three carriers (`esp32-c6-dfr1117`, `esp32-s3-devkitc`, `esp32-s3-xiao`) — same manual pattern across all three | 1.2 | ✅ |
| 1.4-metadata-rest | Remaining plugins (adxl355, sd-card, battery-adc, led, nvs, clock, ble-beacon, ble-l2cap, data-tcp, reset-tcp, log-tcp) + remaining sentants (Accelerometer, WifiProv, Bootstrap, Beacon, Battery, Status, Sync, Recorder, Uplink, Ota, Reset, Health, Capture, Presence) — each gets plugin.toml/sentant.yaml + PLUGIN.md/SENTANT.md + AI-CONTEXT.md. Pattern proven by `sensor/lis2dh` + `sentants/Identity` (✅). | 1.4-metadata | ⏳ |
| 1.4-source | Extract the Rust source for each ensemble plugin from r2-workshop's inline firmware modules into standalone Cargo crates. The heavy lift — requires reading + refactoring ~12 source files. | 1.4-metadata-rest | ⏳ |
| 1.5 | `testing/round-trip/<carrier>.expected.toml` — recorded R2-WIRE traffic from a running r2-workshop firmware, captured as the conformance baseline | 1.3, 1.4 | ⏳ |
| 1.6 | `orchestrator/` scaffolding — axum WSS + static serve on port 21050; `catalogue`, `compiler`, `sync` plugins stubbed (each with a minimal sentant front routing `r2.composer.*` events) | 1.2 | ⏳ |
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
| 3.2 | `r2.composer.flash.*` events surfaced in the UI |
| 3.3 | Live USB device detection (libudev / hotplug) |

## Phase 4 — pin-connection visualisation (deferred)

See [memory: project_phase2_pin_visualisation.md](../../../home/roycdavies/.claude/projects/-mnt-data-Development-R2-r2-composer/memory/project_phase2_pin_visualisation.md) for the design intent. Not started; not blocking earlier phases.

## Open conjectures

- **C-1**: `claude -p` in a non-interactive subprocess with `--output-format=stream-json` is enough to drive an autonomous build cycle. Falsifies if: tool-permission prompts can't be answered from outside the CLI without `--dangerously-skip-permissions`.
- **C-2**: A behavioural-equivalence test (recorded R2-WIRE traffic) is a sufficient conformance gate. Falsifies if: there's a state we care about that doesn't manifest on the wire (e.g. SD ring layout) and only shows up in field testing.
- **C-3**: Vendoring `crates/` from `r2-core` is cheap enough to do per-release. ✅ **Survived** at Phase 1.1 — sync ran in <1 s, no churn. Re-check when r2-core's crate tree restructures.
- **C-4**: One per-carrier crate per build under `out/<carrier>-<timestamp>/` keeps build state hermetic. Falsifies if: `esp-idf-sys`'s caching reaches across timestamped dirs and produces stale binaries.
- **C-5**: Claude Code can reliably generate compact Rust FSMs from sentant.yaml inputs that fit within 80% of the OTA-slot budget for each target carrier (per [[feedback-aot-optimisation-constraint]]). Falsifies if: generated output is larger than the hand-written r2-workshop equivalents by more than 20%, or if compilation requires more than three CC retry cycles to converge on a passing build.

Each conjecture should be either survived or refuted explicitly during Phase 1.

## Architectural constraints worth remembering

- **AOT-compile to direct Rust, NOT a generic engine.** OTA flash budget is load-bearing — 1.875 MB per slot on dfr1117. The Compiler sentant's main job is generating Rust source code that implements each sentant.yaml FSM as direct match arms + static structs, NOT shipping `r2-engine` with `dyn Sentant` dispatch + a preemption scheduler. See [memory: feedback-aot-optimisation-constraint](../../../home/roycdavies/.claude/projects/-mnt-data-Development-R2-r2-composer/memory/feedback_aot_optimisation_constraint.md). The pragmatic firmware shape r2-workshop already ships IS the target — r2-composer mechanises producing that shape from a YAML score.
