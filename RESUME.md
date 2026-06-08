# RESUME — r2-composer (composer-worker)

Fleet checkpoint 2026-06-09. Master save: `r2-specifications/fleet-context/FLEET-CONTEXT-SAVE.md`.

**Role:** the **dynamic fleet tool** — creates/manages a fleet of devices with plugins + sentants (ensembles) +
OTA + the proof UX. It orchestrates hives; it is NOT the hive. (Hive = core's no_std crates + platform layers.)

**In flight (resume here):**

**Part D — DFR1195 hardware tier (lead).** Carrier-board model + OTA (F5/F5b) READY; 4 plugins host-testable now.
- D1 DFR1195 carrier board (board.toml + BOARD.md + AI-CONTEXT). PIN MAP: SX1262 SPI SCK7/MISO5/MOSI6,
  NSS10/RST41/BUSY40/DIO1-4; LCD SPI MOSI11/SCK12/CS17/RST15/DC14/BL16/PWR48; Key1=IO18, Key2/BOOT=0; LED21;
  I2C SDA8/SCL9; BatADC1. **LCD = 160×80 SPI TFT (ST7735-class), NOT SSD1306 OLED.**
- D4 plugins in order: a sim-sensor (no HW dep — test data source) → b button-IO18 → c lcd (SPI-TFT driver) →
  d lora-sx1262 (**sync / embedded-hal trait** to match core's D3b no_std binding — propose the trait, supervisor
  brokers to core). Real radio TX/RX gated on core D3b.
- D5 test process = new ensemble (inject-here/expect-there).
- OTA: your **push** side ready; device receiver is hive's **no_std** firmware (not the std ota_tcp.rs).

**Part C — proof-surface UX (composer UX plugin with its own hive).** Sequence: (i) orchestrator **r2-web host**
(read ensemble `registrations.r2-web` → mount static_bundle@route_prefix → wire subscriptions to `/r2`); (ii) browser
**wasm-hive** = a FULL R2 hive via `crates/r2-wasm` (retire the toy webapp wasm), TCP-only transport via WS↔TCP
bridge to `r2-transport/tcp.rs`, may host other plugins (web-server); (iii) `TestCoordinator` sentant + `test-ux`
ensemble → two views; (iv) coverage grid reads specs' conjecture-catalogue JSON (tier+status, never bare tick).
Templates: `notekeeper.ensemble.yaml`, R2-WEB / R2-PLUGIN §13.2; workshop `dashboard/` = own-hive precedent (peer-ask it).
Read R2-WEB in-place at `r2-specifications/specs/r2-core/R2-WEB.md` (no vendoring).

**Branch:** `phase-1.4-plugin-source` (NO upstream — push `-u`). Will branch fresh off main for Part C/D. WIP checkpointed.
