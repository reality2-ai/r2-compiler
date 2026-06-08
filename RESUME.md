# RESUME — r2-composer (composer-worker)

Owned by the composer worker (I keep this current). Master fleet save (read-only
reference): `claude-fleet/fleet-context/FLEET-CONTEXT-SAVE.md`.
Last updated: 2026-06-09.

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
1. **Part C (i): orchestrator r2-web host** ← NEXT. Read ensemble
   `registrations.r2-web` → mount `static_bundle@route_prefix` → wire
   `subscriptions` to `/r2`. Full-hive territory — unaffected by the MCU
   routing-only constraint below. Start by reading **R2-WEB v0.3 in-place** at
   `r2-specifications/specs/r2-core/R2-WEB.md` (NO vendoring) + workshop's
   `docs/own-hive-web-ui-recipe.md` (two hives / one /r2 channel), then post a
   design before building.
2. **Part C (ii): browser wasm-hive** — a FULL R2 hive via `crates/r2-wasm`
   (retire the toy `webapp/crate` wasm), **TCP-only transport** via a WS↔TCP
   bridge to `r2-transport/tcp.rs`; pluggable. AUTH = **Ed25519, not HMAC**
   (R2-WEB v0.3 §4.2; `device_id` = DEV_PK). Serverless WASM-hive = §8.4.
3. **D4 b/c/d are NOT composer's** (hive placement confirmed 2026-06-09):
   button(IO18)+LCD(ST7735) = hive's no_std firmware test instrumentation;
   LoRa = core's no_std SX1262 sync transport (D3b). composer's remaining
   Part-D = **(a)** the **D5 test ensemble + semantics** (what to inject / what
   "delivered" looks like) on the FULL hives, DFR1195s as routing endpoints
   (inject via button-frame, show via LCD); **(b)** feed a SYNC embedded-hal
   SX1262 trait proposal INTO core D3b (not a parallel composer trait); **(c)**
   ✅ OTA reply-status contract delivered — `specifications/OTA-REPLY-STATUS-CONTRACT.md`
   (status 0x00 OK / 0x01 ERR + CODE-in-msg vocabulary; DFR1195 = 4 MB → TOO_BIG
   bound; folds into SPEC-APIARY-FLASH §6 at merge). Also: peer-ask workshop for
   reusable no_std ST7735 code to point hive at.
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
  R2-TRANSPORT SX1262) → the D4d lora plugin trait must be SYNC embedded-hal to
  match it (propose the trait to supervisor → core when reaching D4d). Owns
  `r2-core/platforms/unoq/` (Uno-Q+LoRa, the later heterogeneous board class).
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
