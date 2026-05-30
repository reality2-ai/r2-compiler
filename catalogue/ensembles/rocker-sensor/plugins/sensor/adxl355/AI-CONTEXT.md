# AI-CONTEXT.md — sensor/adxl355

## Purpose

R2 plugin wrapping the **Analog Devices ADXL355** triaxial accelerometer over SPI. 20-bit native resolution, SHM-grade — the precision peer of `lis2dh` (10-bit, I²C). r2-workshop's primary sensing element on ESP32-S3 carriers.

## Conformance

R2-PLUGIN §12.3 / §12.4 / §12.8 + SPEC-CATALOGUE-LAYOUT §4.3.

## Modes & targets

| Mode | Targets |
|---|---|
| `aot` | `esp32-s3`, `esp32-c6`, `linux-embedded` |

## Hardware

- ADXL355 chip on EVAL-ADXL355-PMDZ (Analog Devices Pmod, 12-pin)
- SPI, 10 MHz max, CPOL=0/CPHA=0
- 3.3 V supply; <200 µA draw
- DEVID = 0xAD, PARTID = 0xED (verified at init)
- Pin assignments per carrier `board.toml [pinout]` (devkitc: CS=10, MOSI=11, SCK=12, MISO=13, DRDY=14)

## Capability-peer

`../lis2dh/` provides the SAME `ai.reality2.cap.accel.triaxial` capability via I²C at 10-bit. The Accelerometer sentant binds by capability not by chip name; the compiler plugin resolves which one to link based on the chosen carrier's `board.toml`. ADXL355 → ESP32-S3 carriers; LIS2DH → ESP32-C6 dfr1117 carrier.

## Reference implementation

`r2-workshop/firmware/esp32-s3/devkitc/src/adxl355.rs` (and the parallel xiao file). The driver scales 20-bit native counts → 256_000-LSB-per-g convention so the wire format and dashboard remain chip-agnostic.

## Read these files in this order

1. [`plugin.toml`](plugin.toml) — the contract
2. [`PLUGIN.md`](PLUGIN.md) — 10-section interface spec
3. **Reference:** `../../../../../../r2-workshop/firmware/esp32-s3/devkitc/src/adxl355.rs`
4. **Capability peer:** `../lis2dh/` (already authored at metadata + source levels)
5. **Upstream specs:** R2-PLUGIN §12, R2-COMPILE §3/§6

## Authoring status

- ✅ plugin.toml · ✅ PLUGIN.md · ✅ AI-CONTEXT.md · ⏳ Cargo.toml + src/ (Phase 1.4-source) · ⏳ datasheets/ (WebFetch)
