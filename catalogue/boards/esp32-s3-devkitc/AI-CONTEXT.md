# AI-CONTEXT.md — esp32-s3-devkitc

If you are a Claude Code session resuming work on this board entry cold, read this file first.

## Purpose

Espressif ESP32-S3-DevKitC-1 — Espressif's reference dev board for the ESP32-S3 family. r2-workshop's **current default carrier** per ADR-002. Xtensa LX7 dual-core, 8 MB flash + 8 MB octal PSRAM (N8R8 reference variant), two USB ports, on-board WS2812 RGB LED, 45 broken-out GPIOs.

The high-pin-count Xtensa option. Use when GPIO headroom matters more than form factor.

## Class + target

| | |
|---|---|
| Directory name | `esp32-s3-devkitc` |
| Target triple | `xtensa-esp32s3-espidf` |
| ESP-IDF version | `v5.2.5` |
| R2-DEF §7.7 compile_target tag | `esp32-s3` (shared with esp32-s3-xiao) |

## Where the canonical artefact lives

[`board.toml`](board.toml) per SPEC-CATALOGUE-LAYOUT §3.2.

[`BOARD.md`](BOARD.md) — narrative companion.

## Vendor refs

On-disk under [`datasheets/`](datasheets/):

- `HARDWARE-WIRING-DEVKITC.md` — the canonical 3-phase build guide (BoM, wiring tables, voltage-divider analysis, pre-power-up checklist).

Vendor URLs (cited; fetch + save before using as reference):

- Vendor page: https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32s3/esp32-s3-devkitc-1/
- ESP32-S3 SoC: https://www.espressif.com/en/products/socs/esp32-s3

## Hive-shared plugins on this carrier

None scaffolded yet under `plugins/`. The carrier's BLE and WiFi singletons are provided by the vendored `r2-esp` crate when the firmware crate is rendered.

## Templates

[`templates/`](templates/) holds the per-build seed files synced from `r2-workshop/firmware/esp32-s3/devkitc/`. See `BOARD.md` "Templates" section for the table.

## Quick differences vs siblings

| Versus | Difference |
|---|---|
| **esp32-s3-xiao** | Same chip, same target, same tag. DevKitC has 45 GPIOs vs XIAO's 11; on-board WS2812 vs external; CP2102 + USB-OTG vs USB-C-only; no on-board LiPo charging vs XIAO's on-board buck+charger. Choose DevKitC for GPIO headroom + diagnosable power chain; XIAO for size + USB-C convenience. |
| **esp32-c6-dfr1117** | Different ISA (Xtensa vs RISC-V), different target triple. DevKitC has PSRAM + 8 MB flash; DFR1117 has neither PSRAM + only 4 MB. DevKitC uses ADXL355 over SPI; DFR1117 uses LIS2DH over I²C — capability-vs-chip swap lever (R2-PLUGIN §10). |

## Known gotchas (quick read — full list in `board.toml [notes].gotchas`)

- WS2812 GPIO: **38 on v1.1 (current production)**, 48 on v1.0. Check silkscreen.
- Custom `partitions.csv` needs **two clean rebuilds** on a fresh checkout (esp-idf-sys CMake build dir doesn't exist on first build).
- First install MUST be a USB flash; OTA only works after the table + both slots are written.
- PSRAM claims GPIO35/36/37 — don't wire them.
- ADC2 (GPIO11-20) is unusable while WiFi is active. Battery sense uses ADC1 (GPIO4) deliberately.
- Use **`esptool`**, not `espflash` (R2-BUILD §5.1).

## Read these files in this order (cold-start resume)

1. [`board.toml`](board.toml) — the structured contract.
2. [`BOARD.md`](BOARD.md) — narrative + template table.
3. [`templates/Cargo.toml.tera`](templates/Cargo.toml.tera) — dependency baseline.
4. [`templates/sdkconfig.defaults`](templates/sdkconfig.defaults) — ESP-IDF tuning.
5. [`templates/partitions.csv`](templates/partitions.csv) — OTA layout.
6. [`templates/.cargo/config.toml`](templates/.cargo/config.toml) — target + linker.
7. [`datasheets/HARDWARE-WIRING-DEVKITC.md`](datasheets/HARDWARE-WIRING-DEVKITC.md) — full 3-phase wiring narrative.
8. The local [`conversation/`](conversation/) directory — most recent authoring session.
9. **Upstream contract:** `../../../specifications/SPEC-CATALOGUE-LAYOUT.md` §3.

## Authoring status

- ✅ `board.toml` (manually authored 2026-05-31)
- ✅ `BOARD.md`
- ✅ `AI-CONTEXT.md` (this file)
- ✅ `templates/` (synced from r2-workshop)
- ✅ `datasheets/HARDWARE-WIRING-DEVKITC.md`
- ⏳ `pinout.svg` — Phase 4
- ⏳ Vendor PDF datasheets — to fetch via AuthorPilot WebFetch when available

---

*Created 2026-05-31, Phase 1.3.*
