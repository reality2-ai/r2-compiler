# DFRobot Beetle ESP32-C6 (DFR1117)

The RISC-V member of r2-compiler's carrier-board catalogue. Coin-sized DFRobot board around the **ESP32-C6-FH4** — single-core 160 MHz RISC-V, WiFi 6, BLE 5.0, 802.15.4. 4 MB flash, no PSRAM, native USB-Serial-JTAG.

## At a glance

| | |
|---|---|
| **Vendor** | DFRobot |
| **SKU** | DFR1117 ("FireBeetle 2 ESP32-C6 IoT Microcontroller with Onboard WiFi 6, Bluetooth, Sensor") |
| **SoC** | Espressif ESP32-C6-FH4 |
| **Architecture** | RISC-V (single-core, 160 MHz) |
| **Flash / PSRAM** | 4 MB / none |
| **USB** | USB-C, native USB-Serial-JTAG (flash + monitor on one cable) |
| **Power** | USB-C 5 V; on-board LiPo charge (TP4057) + 3.3 V LDO |
| **Vendor wiki** | https://wiki.dfrobot.com/dfr1117/ |
| **Board entry created** | 2026-05-31 |

## Role in r2-compiler

The DFR1117 is r2-workshop's **non-Xtensa reference carrier** — it's the carrier that proves the firmware substrate is genuinely portable across instruction sets (RISC-V here, Xtensa on the DevKitC + XIAO). It's also r2-workshop's worked example of the **R2-PLUGIN §10 capability-vs-chip swap lever**: where the Xtensa carriers run the ADXL355 (precision SPI), this carrier ships the SEN0224 (LIS2DH, plug-and-play I²C). Same sentant binds both — `ai.reality2.cap.accel.triaxial` — different plugin under the hood.

For r2-compiler's v0.1 success gate, this is the carrier that exercises the RISC-V build path. If r2-compiler can round-trip the rocker-sensor ensemble for both Xtensa (devkitc/xiao) and RISC-V (dfr1117) carriers from one score, the cross-architecture compile path is real.

## Where to wire what

The board's edge pads are silk-labelled with **function names** (`SDA`, `SCL`, `SCK`, `MO`, `MI`, `RX`, `TX`, `LP_*`), not raw `IOnn` numbers. **Wire by the printed label**; the firmware pin assignments under `board.toml [pinout]` already match.

Summary (full table in [`board.toml`](board.toml) `[pinout]`):

| Function | GPIO | Silk |
|---|---|---|
| Status LED (mono) | 15 | (on-board, no header) |
| Battery ADC | 4 | `LP_RX` |
| SPI SCK | 23 | `SCK` |
| SPI MOSI | 22 | `MO` |
| SPI MISO | 21 | `MI` |
| SD CS | 7 | `LP_SCL` |
| I²C SDA | 19 | `SDA` |
| I²C SCL | 20 | `SCL` |

Full wiring narrative — including the SEN0224 accelerometer setup, the SPI-vs-I²C bus split rationale, and the LiPo / VIN / 5 V routing — is at [`datasheets/HARDWARE-WIRING-DFR1117.md`](datasheets/HARDWARE-WIRING-DFR1117.md).

## Build & flash

The per-build crate is rendered by r2-compiler's orchestrator from [`templates/`](templates/). Manual flow (matches r2-workshop) once a crate has been produced:

```bash
cd out/esp32-c6-dfr1117-<timestamp>/
cargo build --release           # cross-compiles to riscv32imac-esp-espidf
esptool --chip esp32c6 --port /dev/ttyACMx write_flash \
  0x0     build/bootloader/bootloader.bin \
  0x8000  build/partition_table/partition-table.bin \
  0x20000 target/riscv32imac-esp-espidf/release/r2-workshop-firmware.bin
```

After the first USB flash, subsequent updates can go over WiFi via OTA (R2-DEPLOY plugin).

## Templates

`templates/` holds the per-build seed files copied by `tools/sync-catalogue.sh` from `r2-workshop/firmware/esp32-c6/dfr1117/`. The Compiler sentant renders `Cargo.toml.tera` per-build (substituting the vendored-crate paths into the deps section), and copies the others verbatim:

| File | Purpose |
|---|---|
| `templates/Cargo.toml.tera` | crate manifest — dependencies, profile, bin target. **Path deps need re-anchoring** at render time: r2-workshop has `path = "../../../crates/r2-foo"`; r2-compiler builds emit into `out/<slug>-<ts>/` so the equivalent dep path differs. |
| `templates/.cargo/config.toml` | sets target triple, `ldproxy` linker, `espflash` runner, MCU env vars |
| `templates/sdkconfig.defaults` | ESP-IDF tuning: chip target, flash size, NimBLE host, FATFS LFN, USB-Serial-JTAG console |
| `templates/partitions.csv` | two-OTA-slot layout sized for 4 MB flash (1.875 MB per slot, no factory, no internal FAT — capture data lives on external microSD) |
| `templates/build.rs` | walks up to find `esp-idf-sys-*/out/` and copies `partitions.csv` into the CMake build dir so the custom layout takes effect |
| `templates/rust-toolchain.toml` | pins the esp toolchain |
| `templates/wifi_config.toml.example` | dev fallback for WiFi credentials |

## Known gotchas

Spelled out in [`board.toml`](board.toml) `[notes].gotchas`. Re-read those before changing anything in `templates/`.

## Authoring history

The original DFR1117 firmware crate (`r2-workshop/firmware/esp32-c6/dfr1117/`) was authored over many r2-workshop sessions through May 2026 — see `r2-workshop/conversation/` for the trail.

This `catalogue/boards/esp32-c6-dfr1117/` entry was scaffolded in r2-compiler design session 01 (2026-05-31) — see [`../../../conversation/2026-05-31-r2-compiler-design-01.md`](../../../conversation/2026-05-31-r2-compiler-design-01.md). The `board.toml` + this BOARD.md were manually authored (pre-AuthorPilot) by reading the synced template files and the wiring guide.

## See also

- [`board.toml`](board.toml) — the structured contract
- [`AI-CONTEXT.md`](AI-CONTEXT.md) — fresh-CC brief
- [`datasheets/HARDWARE-WIRING-DFR1117.md`](datasheets/HARDWARE-WIRING-DFR1117.md) — full wiring narrative
- `../../../specifications/SPEC-CATALOGUE-LAYOUT.md` §3 — the board entry schema
- `../../../../r2-workshop/firmware/esp32-c6/dfr1117/` — the working firmware this entry mirrors
