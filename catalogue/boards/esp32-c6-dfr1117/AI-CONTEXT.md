# AI-CONTEXT.md — esp32-c6-dfr1117

If you are a Claude Code session resuming work on this board entry with no prior conversation context, read this file first, then the files it points at.

## Purpose

The DFRobot Beetle ESP32-C6 (DFR1117) carrier — r2-compiler's RISC-V reference. Single-core 160 MHz RISC-V (ESP32-C6), WiFi 6, BLE 5.0, 802.15.4. 4 MB flash, no PSRAM. Distinct from the two Xtensa ESP32-S3 carriers (`esp32-s3-devkitc`, `esp32-s3-xiao`) in that it's a different ISA — cross-architecture firmware-build exercise.

## Class + target

| | |
|---|---|
| Directory name | `esp32-c6-dfr1117` |
| Target triple | `riscv32imac-esp-espidf` |
| ESP-IDF version | `v5.2.5` |
| R2-DEF §7.7 compile_target tag | `esp32-c6` |

## Where the canonical artefact lives

[`board.toml`](board.toml) — the structured contract per SPEC-CATALOGUE-LAYOUT §3.2.

[`BOARD.md`](BOARD.md) — narrative companion (at-a-glance + wiring summary + template rationale).

## Vendor refs

All on-disk under [`datasheets/`](datasheets/):

- `HARDWARE-WIRING-DFR1117.md` — copied from `r2-workshop/specifications/`; the canonical wiring guide for this carrier.

Vendor URLs (cited but NOT relied on as live links — fetch + save before using):

- DFRobot product wiki: https://wiki.dfrobot.com/dfr1117/
- ESP32-C6 SoC vendor page: https://www.espressif.com/en/products/socs/esp32-c6
- Arduino variant (label↔GPIO ground-truth): https://github.com/espressif/arduino-esp32/tree/master/variants/dfrobot_beetle_esp32c6

## Hive-shared plugins on this carrier

None scaffolded yet under `plugins/`. The carrier's transport singletons (BLE radio, WiFi radio) are provided by the vendored `r2-esp` crate when the firmware crate is rendered — they don't need their own catalogue plugin entry until the C6 grows a carrier-specific singleton not satisfied by `r2-esp`.

## Templates

[`templates/`](templates/) holds the per-build seed files. See `BOARD.md` "Templates" section for the table.

The compiler plugin (Phase 1.5+) renders `Cargo.toml.tera` per-build, substituting the vendored-crate paths. The other files are copied verbatim.

## Quick differences vs siblings

- **vs `esp32-s3-devkitc`**: different ISA (RISC-V vs Xtensa), single-core vs dual-core, 4 MB vs 8 MB flash, no PSRAM vs 8 MB octal PSRAM, USB-C native USB-Serial-JTAG only (no CP2102), two-bus design (I²C accel + SPI SD) vs shared SPI for both. WiFi 6 (C6) vs WiFi 4 (S3).
- **vs `esp32-s3-xiao`**: same ISA difference. Both are coin-sized boards but DFR1117 is the RISC-V reference; XIAO is the alternative Xtensa coin board. DFR1117 has on-board LiPo charger via TP4057; XIAO has charger + buck regulator integrated.
- **Shared with both ESP32-S3 carriers**: native USB-Serial-JTAG, OTA over WiFi (TCP port 21043), `esptool` (not `espflash`) for the first USB flash, custom `partitions.csv` (two OTA slots, no factory).

## Known gotchas (quick read — full list in `board.toml [notes].gotchas`)

- Custom `partitions.csv` requires **two clean rebuilds** on a fresh checkout; `build.rs` finds the esp-idf-sys CMake build dir and copies the partition table there. First build often uses the ESP-IDF default 1-app layout; rebuild and the custom 2-OTA layout takes effect. See `r2-workshop/tools/setup-firmware.sh`.
- **First install MUST be a USB flash** (`esptool write_flash`) — this writes the partition table + both OTA slots. OTA-over-WiFi only works thereafter.
- **Two-bus design** on this carrier: I²C for accel (SDA=19, SCL=20), SPI for SD (SCK=23, MOSI=22, MISO=21, CS=7). The ESP32-S3 carriers share one SPI bus between ADXL355 + SD.
- ESP32-C6 console routes over **native USB-Serial-JTAG** (`CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG=y`) — one USB-C cable does flash + monitor.
- Use **`esptool`** (Python, ESP-IDF-bundled), NOT `espflash` — per R2-BUILD §5.1, `espflash v3.x` writes a header byte that breaks ESP-IDF v5.3+ bootloaders.

## Read these files in this order (cold-start resume)

1. [`board.toml`](board.toml) — the structured contract.
2. [`BOARD.md`](BOARD.md) — narrative.
3. [`templates/Cargo.toml.tera`](templates/Cargo.toml.tera) — dependency baseline.
4. [`templates/sdkconfig.defaults`](templates/sdkconfig.defaults) — ESP-IDF tuning.
5. [`templates/partitions.csv`](templates/partitions.csv) — OTA layout.
6. [`templates/.cargo/config.toml`](templates/.cargo/config.toml) — target + linker.
7. [`datasheets/HARDWARE-WIRING-DFR1117.md`](datasheets/HARDWARE-WIRING-DFR1117.md) — wiring narrative + accelerometer choice rationale.
8. The local [`conversation/`](conversation/) directory — most recent authoring session.
9. **Upstream contract:** `../../../specifications/SPEC-CATALOGUE-LAYOUT.md` §3 — the schema this entry must satisfy.

## Authoring status

- ✅ `board.toml` (manually authored 2026-05-31 from synced templates + workshop wiring guide)
- ✅ `BOARD.md`
- ✅ `AI-CONTEXT.md` (this file)
- ✅ `templates/` (synced from `r2-workshop/firmware/esp32-c6/dfr1117/`)
- ✅ `datasheets/HARDWARE-WIRING-DFR1117.md`
- ⏳ `pinout.svg` — Phase 4 (deferred per `plan/PLAN.md`)
- ⏳ Vendor PDF datasheets — to fetch via the authoring-flow WebFetch when it exists
- ✅ `conversation/2026-05-31-board-toml-authored-01.md` — this session's transcript (placeholder; will be filled by the design conversation when written)

---

*Created 2026-05-31. The first concrete board entry in r2-compiler's catalogue — used as the schema worked example referenced by SPEC-CATALOGUE-LAYOUT.*
