# RESUME — r2-composer (composer-worker)

Owned by the composer worker (I keep this current). Master fleet save (read-only
reference): `claude-fleet/fleet-context/FLEET-CONTEXT-SAVE.md`.
Last updated: 2026-06-12.

**Role:** the **dynamic fleet tool** — creates/manages a fleet of devices with
plugins + sentants (ensembles) + OTA + the proof UX. It **orchestrates hives; it
is NOT the hive.** North-star: ONE hive codebase everywhere (core's no_std crates
+ thin per-platform host layer: cloud/Linux, ESP32-S3/DFR1195, Uno-Q, wasm-browser).

## Branches (all held — Roy batch-merges; none merged to main yet)
- `phase-3-hardware-tier` ← **current.** Phase-3 Part C/D work. Stacked on the
  phase-1.4 tip (carries the plugin crates) + a fleet checkpoint commit.
- `phase-1.4-plugin-source` — Phase 1.4-source (8/8 catalogue plugin crates). DONE.
- `f5b-ota-ack-confirmation` — F5b OTA ack-as-confirmation (roster firmware_sha).
- `main` — has F5 (ota_push) at 2cdc541; does NOT have F5b or the 1.4 plugins yet.
- Push policy: `git push -u origin <branch>` for backup; **explicit `git add <paths>`
  only — never `-A`/`.`; never stage secrets.**

## Done
- **Phase 1.4-source COMPLETE** — 8/8 catalogue plugin crates (sensor: lis2dh,
  adxl355, battery-adc; indicator: led; storage: sd-card; comms: reset-tcp,
  log-tcp, data-tcp), each host-tested no_std, proven pattern (HAL/protocol core
  behind a platform trait). C-5 fully survived.
- **F5 / F5b** — OTA push wire (R2-UPDATE §3.1.2.2) + ack-as-confirmation.
- **Phase 3 D1** ✅ — DFR1195 carrier board (`catalogue/boards/esp32-s3-dfr1195/`),
  first tri-radio carrier. Pin map in its board.toml (source of truth).
- **Phase 3 D4a** ✅ — `r2-plugin-sensor-simulated` (deterministic synthetic
  triaxial source; the test data feed). `catalogue/ensembles/transient-test/`.

## Resume here — next steps (confirmed sequence)
1. **Part C (i): orchestrator r2-web host** ← IN PROGRESS. Design posted +
   approved-by-default (`specifications/PART-C-I-R2WEB-HOST-DESIGN.md`).
   - ✅ **Slice 1 (foundation):** `orchestrator/src/web.rs` — `registrations.r2-web`
     parser (serde_yaml; handles both notekeeper-graphql + workshop-channels
     shapes) + static-mount builder (ServeDir + SPA fallback; nest for sub-path
     prefixes, fallback_service for root). 7 module tests; orchestrator suite
     110/110. NOT yet wired into main.rs's router (no test-ux ensemble.yaml yet).
   - ✅ **Slice 2a (auth core):** `web.rs::verify_ws_auth` — per-message **Ed25519**
     verify vs the apiary TG (R2-WEB §4.2), `device_id`=DEV_PK, 60 s replay window,
     membership behind an `is_live_member` closure. **Ed25519 FROM THE START — NO
     trusted_local** (Roy's Q1 decision; §10.2-conformant). 6 tests; suite 116/116.
   - ✅ **Slice 2b-i (membership wiring):** `web.rs::roster_is_live_member` —
     backs `verify_ws_auth` with real apiary state (DEV_PK present + cert_status
     valid + state not revoked/retired). End-to-end test: valid sig from a
     non-enrolled key → `NotMember` (§10.2 gate proven). web module 15 tests;
     suite 118/118.
   - ✅ **Slice 2b-ii (channel routing logic):** `web.rs::WireChannel` — the
     `/r2/wire` raw-R2-WIRE channel's routing: frame-size gate (`max_frame_bytes`),
     inbound → `Inject{target_sentant}` / `RejectOversize`, outbound subscription
     matching (FNV-1a-32 of event names). Auth-agnostic by design. web module
     19 tests; suite 122/122. **Channels per specs call 2:** `/ws` = JSON device
     channel (Ed25519 `verify_ws_auth`); `/r2/wire` = raw-WIRE node channel
     (native frame auth), DISTINCT paths, never multiplexed; `/r2/wire` is
     CONFORMANT-PENDING-SPEC (R2-WEB §4.6 forthcoming).
   - ✅ **Slice 2b-ii (frame auth):** `web.rs::verify_wire_frame` — native
     R2-WIRE/R2-TRUST frame auth for `/r2/wire`, **consuming** `r2_wire`
     (`decode_compact`/`verify_compact`) + `r2_trust::GroupHmac` (NOT
     reimplemented; §4.6 greenlit, conformant-pending-authoring). 4 tests
     (valid / wrong-group-key / unsigned / malformed). web module **24 tests**;
     suite **127/127**. Both channel auths now exist: `/ws` = `verify_ws_auth`
     (Ed25519), `/r2/wire` = `verify_wire_frame` (group-HMAC).
   - ✅ **Slice 2b-ii-rest (async /r2/wire WS handler):** `web::wire_socket_loop`
     + the `/r2/wire` route (main.rs). Inbound: size-gate → `verify_wire_frame`
     (apiary group-HMAC) → inject `QueuedEvent` (src 0xFF) via `engine.inbound_tx`;
     bad-auth dropped silently (§4.2). Outbound: bus events → `WireChannel.outbound_for`
     → `encode_signed_wire_frame` → binary. GroupHmac via `derive_apiary_group_hmac`
     (TG SK → `derive_group_keys().hk`, volatile). REFUSES if no apiary/TG key
     (no §10.2 bypass). Route live-verified (400 upgrade-required, not 404);
     encode↔verify round-trip tested. web module 25 tests; suite 139/139. ⏭
     happy-path live round-trip (signed frame→inject→TestCoordinator→signed reply)
     awaits a TG-keyed apiary + a frame-signing client (the wasm-hive, Part C ii);
     `/ws` JSON handler + replay-on-connect deferred (the wasm-hive uses /r2/wire);
     mesh leg → core **D3a**.
   - ⏭ **Slice 2b-iii:** browser-identity **enrolment** (mint/enrol a DEV_PK via
     software-ed25519 so the wasm-hive is a provisioned TG member it can auth as).
   - ⏭ **Slice 3:** wire the registration set into main.rs (mount each hosted
     ensemble's bundle@prefix + its `/r2` channel), arriving with the D5 test-ux
     ensemble.yaml. Bundle MUST set `<base href="<prefix>/">` (trailing-slash
     papercut is a bundle concern, not routing — axum can't nest + redirect the
     same prefix).
   Built to **R2-WEB v0.3** (read in-place) + workshop two-hive recipe.
2. **Part C (ii): browser wasm-hive** — a FULL R2 hive via `crates/r2-wasm`
   (retire the toy `webapp/crate` wasm), **TCP-only transport** via a WS↔TCP
   bridge to `r2-transport/tcp.rs`; pluggable. AUTH = **Ed25519, not HMAC**
   (R2-WEB v0.3 §4.2; `device_id` = DEV_PK). Serverless WASM-hive = §8.4.
3. **D4 b/c/d are NOT composer's** (hive placement confirmed 2026-06-09):
   button(IO18)+LCD(ST7735) = hive's no_std firmware test instrumentation;
   LoRa = core's no_std SX1262 sync transport (D3b). composer's remaining
   Part-D = **(a)** the **D5 test ensemble + semantics** (what to inject / what
   "delivered" looks like) on the FULL hives, DFR1195s as routing endpoints
   (inject via button-frame, show via LCD). ✅ ensemble SKELETON authored:
   `catalogue/ensembles/transient-test/ensemble.yaml` (class
   `ai.reality2.composer.transient-test`; TestCoordinator sentant decl +
   sim-sensor plugin + the `registrations.r2-web` proof-surface block:
   route_prefix `/proof`, `/r2/wire` channel → TestCoordinator, 6 `r2.tn.*`
   subscriptions). Parser-validated (web.rs include_str! test); catalogue index
   = 2 ensembles. ✅ **TestCoordinator adjudication core**
   (`sentants/test_coordinator.rs`) — aligned 1:1 with the sim/catalogue
   vocabulary (supervisor anti-rework directive): `MsgKey{origin,msg_id}` +
   `DeliveryLedger` + the **A6 assert set** (exactly_once / copy_count /
   no_duplicate / no_drop / delivered_by / reconcile_correct_after_heal),
   structured `AssertFail` (mirrors r2-harness assert.rs). Hardware test = the
   catalogue's `{topology,timeline,expect}` block verbatim → sim+hardware share
   one frame, coverage grid 1:1. 6 tests; suite 133/133. (Ref:
   R2-TRANSIENT-NETWORKING.md §6; catalogue json = 52 records.) ✅ **TestCoordinator Sentant FSM** wrapping the core — handles r2.tn.inject
   {origin,msg_id}→track+emit injected (Running); r2.tn.report {node,origin,msg_id}
   →ledger.record+emit delivered{copies}; r2.tn.assert {kind,...}→adjudicate via
   the A6 asserts+emit result{ok,detail}. r2.tn.* routed by canonical FNV (raw
   /r2/wire channel; no registry entry). 10 tests; suite 137/137. ⏭ remaining:
   register in hive.rs (run live) + wire report-ingest/frame-inject to /r2/wire
   (needs the C(i) WS handler); then the proof-surface web bundle (Part C ii); **(b)** feed a SYNC embedded-hal
   SX1262 trait proposal INTO core D3b (not a parallel composer trait); **(c)**
   ✅ OTA reply-status contract delivered — `specifications/OTA-REPLY-STATUS-CONTRACT.md`
   (status 0x00 OK / 0x01 ERR + CODE-in-msg vocabulary; DFR1195 = 4 MB → TOO_BIG
   bound; folds into SPEC-APIARY-FLASH §6 at merge). ✅ **OTA loop CLOSED
   cross-repo**: hive built the no_std embassy-net receiver to this contract
   (CODE set + framing + bound-check all verified to match my F5 push wire
   byte-for-byte); push side now classifies the CODEs (`ota_push::classify_device_error`
   → kebab error_kind). Reply framing `[status u8][msg_len u16 LE][msg]`
   confirmed byte-exact by hive — **OTA loop fully closed, no open items**.
   Workshop: no reusable ST7735 driver ("yes to semantics") — for hive's D4c
   firmware display.
4. **Part C (iii/iv)** — `TestCoordinator` sentant + `test-ux` ensemble (two
   views) → coverage grid reading specs' published
   `testing/test-vectors/r2-transient-networking-conjectures.json` (fields:
   id/level/scope/plane/payload/status/tier; show tier+status, never a bare tick;
   honour its `dashboard_lift_policy`).

## MCU reality (hive assessment, 2026-06-09) — affects D4/D5 placement
Near-term DFR1195 hive is **routing + transport ONLY** — no on-device
ensemble/sentant hosting (r2-def/ensemble/dispatch are std-tier in core, not
no_std yet). So:
- Engine-hosted plugins (sim-sensor, and any sentant-hosting) run on the **FULL
  hives** (laptop / wasm-hive), with DFR1195s as routing nodes.
- **D4b button (IO18) + D4c LCD = FIRMWARE-LEVEL test instrumentation** in hive's
  no_std firmware (inject event / show delivery), NOT engine plugins on the MCU.
  → **Peer-ask hive** to align placement before authoring them as composer plugins.
- D5 test ensemble + sentant-hosting plugins → full hives for now.
- Routing-only is sufficient to PROVE transient networking; on-device ensembles
  are a later vision step (re-tiering r2-def/ensemble/dispatch = a core+spec item).

## Dependencies / coordination
- **hive** — DFR1195 no_std firmware (Path B esp-hal/embassy), the OTA receiver
  (no_std embassy-net; composer's F5 push wire unchanged — coordinate the wire
  contract), and D4 plugin placement.
- **core** — D3a (std/alloc ASYNC r2-discovery udp_lan/tcp) Linux-verifiable
  NOW → gives the orchestrator/wasm-hive a REAL transport soon. D3b (no_std SYNC
  R2-TRANSPORT SX1262) → the D4d lora trait must be SYNC embedded-hal to match
  it (propose to supervisor → core when reaching D4d). **Keep it CHIP-AGNOSTIC:**
  Roy has an **LR2021** kit (Semtech 4th-gen, different chip; no_std `lr2021`
  crate exists — github TheClams/lr2021, async/embassy). The trait must serve
  SX1262 AND LR2021 (radio-HAL seam; async driver wraps under sync `poll_recv`) —
  don't bake SX1262 specifics in. Roy deciding LR2021 = hive's LoRa radio vs a
  standalone node. Owns `r2-core/platforms/unoq/` (Uno-Q+LoRa, later board class).
  r2-def web_template test fixed (68565d8) — re-sync vendored r2-def at a clean
  point to green the workspace-wide `cargo test`.
- **specs** — R2-WEB v0.3 (read in-place). r2-web plugin template DOESN'T EXIST
  yet (specs gap); required SHAPE per R2-PLUGIN §13: name `web-template`, one
  plugin `ui`, plugin_type `web`, bundle `ui/`, default mount, no channels, no
  CSP. Conjecture-catalogue schema published.

## Confirmations needed from Roy (physical DFR1195 boards)
1. SX1262 silk MI/MO → MOSI/MISO (recorded MO=MOSI=GPIO6, MI=MISO=GPIO5).
2. Exact 0.96″ TFT controller variant (assumed ST7735S, colour SPI 160×80).
3. USB enumeration (native S-JTAG assumed; confirm no CH340/CP2102 bridge).
