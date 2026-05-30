# AI-CONTEXT.md — esp32-s3-xiao

If you are a Claude Code session resuming work on this board entry cold, read this file first.

## Purpose

Seeed Studio XIAO ESP32-S3 (Pre-Soldered) — coin-sized 21 × 17.5 mm ESP32-S3 board with on-board LiPo charging + USB-C. r2-workshop's compact alternative to the DevKitC carrier (ADR-001 made it default; ADR-002 reverted). Same chip family as `esp32-s3-devkitc` but very different envelope: USB-C only, only 11 GPIOs broken out, on-board buck + LiPo charger. FSM status LED is the on-board mono yellow LED on GPIO21 (LEDC PWM) — same mono-LED pattern as `esp32-c6-dfr1117`. No external WS2812 module is required; FSM does not surface colour on this carrier.

The constrained-GPIO Xtensa option. Use when size and on-board power management matter; choose the DevKitC if you need pin headroom or colour-coded status indication.

## Class + target

| | |
|---|---|
| Directory name | `esp32-s3-xiao` |
| Target triple | `xtensa-esp32s3-espidf` |
| ESP-IDF version | `v5.2.5` |
| R2-DEF §7.7 compile_target tag | `esp32-s3` (shared with esp32-s3-devkitc) |

## Where the canonical artefact lives

[`board.toml`](board.toml) per SPEC-CATALOGUE-LAYOUT §3.2.

[`BOARD.md`](BOARD.md) — narrative companion + sibling-carrier comparison table.

## Vendor refs

On-disk under [`datasheets/`](datasheets/):

- `HARDWARE-WIRING-XIAO.md` — the canonical 3-phase build guide (BoM differences vs DevKitC, BAT+/BAT− soldering, external WS2812 wiring, on-board-charger behaviour, hot-swap caveats).

Vendor URLs:

- Seeed wiki: https://wiki.seeedstudio.com/xiao_esp32s3_getting_started/
- Product page: https://www.seeedstudio.com/Seeed-Studio-XIAO-ESP32S3-Pre-Soldered-p-6334.html

## Hive-shared plugins on this carrier

None scaffolded yet under `plugins/`. BLE / WiFi singletons come from the vendored `r2-esp` crate at firmware-render time.

## Templates

[`templates/`](templates/) — synced from `r2-workshop/firmware/esp32-s3/xiao/`. Identical partition layout to DevKitC (both are 8 MB carriers); the Cargo.toml differs only in description + WS2812 comments. See `BOARD.md` for the table.

## Quick differences vs siblings

| Versus | Difference |
|---|---|
| **esp32-s3-devkitc** | Same chip + target + tag. XIAO has 11 GPIOs (vs 45), mono LED on GPIO21 (vs DevKitC's on-board WS2812 on GPIO38), on-board LiPo handling, USB-C only. Choose XIAO for size + USB-C + simpler hardware; DevKitC for GPIO + colour-coded status. |
| **esp32-c6-dfr1117** | Different ISA (Xtensa vs RISC-V), 8 MB vs 4 MB flash, ADXL355 SPI vs LIS2DH I²C (capability swap). **Same mono-LED pattern** — both use LEDC PWM on a single on-board pin (GPIO21 here, GPIO15 on the dfr1117). |

## Known gotchas (quick read — full list in `board.toml [notes].gotchas`)

- **Cell solders directly to BAT+/BAT− pads on the back** — no JST-PH connector. Hot-swap during a session means de-soldering.
- **No over-discharge protection** on the on-board charger. Use protected 18650 cells; disconnect when idle for >24 h.
- **FSM status LED is the on-board mono yellow LED on GPIO21** (LEDC PWM). No external WS2812 module is required; FSM uses blink rate / duty cycle (not colour) to distinguish states. Same mono-LED pattern as dfr1117 (GPIO15).
- **Template lag (2026-05-31):** `templates/Cargo.toml.tera` still declares `ws2812-esp32-rmt-driver` because r2-workshop's xiao firmware hasn't yet been updated to the mono-LED design. The compiler plugin must drop that dep and emit LEDC-PWM driver code. See `board.toml [notes].gotchas` last entry and `[[project-xiao-led-choice]]` in memory.
- **D2/GPIO3 is a strapping pin** (JTAG signal source select). Do not wire.
- **D6/GPIO43, D7/GPIO44** are UART0 TX/RX — reserved unless adding an external UART console.
- Same `esptool`-not-`espflash` rule per R2-BUILD §5.1.
- Bootloader mode: hold BOOT, press/release RESET if `espflash` can't detect.
- **XIAO ESP32-S3 Plus** is a different SKU (16 MB flash, GPIOs D11–D18) — treat as a separate board entry when added.

## Read these files in this order (cold-start resume)

1. [`board.toml`](board.toml) — the structured contract.
2. [`BOARD.md`](BOARD.md) — narrative + sibling-carrier comparison table.
3. [`templates/Cargo.toml.tera`](templates/Cargo.toml.tera) — dependency baseline (note external WS2812 comments differ from DevKitC).
4. [`templates/sdkconfig.defaults`](templates/sdkconfig.defaults) — ESP-IDF tuning.
5. [`templates/partitions.csv`](templates/partitions.csv) — OTA layout (identical to DevKitC).
6. [`templates/.cargo/config.toml`](templates/.cargo/config.toml) — target + linker.
7. [`datasheets/HARDWARE-WIRING-XIAO.md`](datasheets/HARDWARE-WIRING-XIAO.md) — full 3-phase wiring narrative.
8. The local [`conversation/`](conversation/) directory.
9. **Upstream contract:** `../../../specifications/SPEC-CATALOGUE-LAYOUT.md` §3.

## Authoring status

- ✅ `board.toml` (manually authored 2026-05-31)
- ✅ `BOARD.md`
- ✅ `AI-CONTEXT.md` (this file)
- ✅ `templates/` (synced from r2-workshop)
- ✅ `datasheets/HARDWARE-WIRING-XIAO.md`
- ⏳ `pinout.svg` — Phase 4
- ⏳ Vendor PDF datasheets — to fetch via the authoring-flow WebFetch when available

---

*Created 2026-05-31, Phase 1.3.*
