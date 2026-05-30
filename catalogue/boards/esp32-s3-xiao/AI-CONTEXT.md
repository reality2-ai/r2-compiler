# AI-CONTEXT.md — esp32-s3-xiao

> **Placeholder.** This entry has not been authored yet. Status: scaffolding only.

## Purpose

Seeed Studio XIAO ESP32-S3 (Pre-Soldered) carrier — compact ESP32-S3 module with on-board USB-C. Variants exist (Pre-Soldered, Plus, Sense). v0.1 targets the **Pre-Soldered** variant — 8 MB flash, 8 MB octal SPI PSRAM, native USB-Serial-JTAG.

Used by `r2-workshop/firmware/esp32-s3/xiao/`. The current default carrier for new r2-workshop sensor units (ADR-001).

## Class + target

- `<arch>-<chip>-<carrier>` = `esp32-s3-xiao`
- Target triple = `xtensa-esp32s3-espidf`
- R2-DEF §7.7 compile_target tag = `esp32-s3`

## Vendor refs (to populate)

- Seeed Studio XIAO ESP32-S3 wiki — https://wiki.seeedstudio.com/xiao_esp32s3_getting_started/
- ESP32-S3R8 (the in-module chip die) datasheet
- XIAO ESP32-S3 schematic PDF — fetch from Seeed wiki

## Authoring source

- `r2-workshop/firmware/esp32-s3/xiao/` — the working per-carrier crate.
- `r2-workshop/specifications/HARDWARE-WIRING-XIAO.md` — pin assignments per signal.

## XIAO-specific pin map (informative — verify when authoring `board.toml`)

| Function | XIAO silkscreen | GPIO |
|---|---|---|
| SPI CS | D0 | GPIO1 |
| Sensor DRDY | D1 | GPIO2 |
| Battery sense | D3 | GPIO4 |
| SD CS | D4 | GPIO5 |
| WS2812 DIN | D5 | GPIO6 |
| SPI SCK | D8 | GPIO7 |
| SPI MISO | D9 | GPIO8 |
| SPI MOSI | D10 | GPIO9 |

Source: r2-workshop's `firmware/esp32-s3/xiao/README.md`.

## Known gotchas

- Native USB-Serial-JTAG console — `cargo run` invokes `espflash flash --monitor`; no separate USB-to-serial chip in the way.
- Bootloader mode: if `espflash` cannot detect the chip, hold `BOOT` while pressing/releasing `RESET`. Most of the time this is not needed.
- Same `espflash` header-bug caveat as DevKitC (R2-BUILD §5.1) — prefer `esptool` for production flashes.
- **XIAO ESP32-S3 Plus** is a different SKU (16 MB flash, additional GPIOs D11–D18) — sdkconfig differs. Treat as a separate board entry when added.

## Read these files in this order (once authored)

Same as `esp32-s3-devkitc/AI-CONTEXT.md`.

---

*Created 2026-05-31 as a scaffold; needs full authoring before v0.1 success gate can be exercised.*
