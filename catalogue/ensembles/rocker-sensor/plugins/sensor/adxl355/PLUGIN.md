# adxl355

**Version:** 0.1.0 (metadata draft) · **Modes:** `aot` only · **Conformance:** R2-PLUGIN §12 (§12.8 10 sections)

## 1. Purpose

Analog Devices ADXL355 precision triaxial accelerometer over SPI. 20-bit native data, low-noise (25 µg/√Hz), SHM-grade. The primary sensing element on r2-workshop's ESP32-S3 carriers (DevKitC + XIAO). Provides the generic `ai.reality2.cap.accel.triaxial` capability — capability-paired with [`lis2dh`](../lis2dh/) (I²C, 10-bit). Sentants binding the capability get either chip transparently per R2-PLUGIN §10's swap lever.

## 2. Modes & Platforms

| Mode | Targets | Status |
|---|---|---|
| `aot` | `esp32-s3`, `esp32-c6`, `linux-embedded` | Primary (only mode in v0.1) |
| `nif` | — | Not supported in v0.1 |
| `web` | — | Not applicable (hardware driver) |

## 3. Events Handled (Inbound)

| Event class | Parameters | Purpose |
|---|---|---|
| `ai.reality2.cap.accel.init` | `{ odr_hz, range_g, offset? }` | Configure ODR + range + offsets; verify DEVID + PARTID |
| `ai.reality2.cap.accel.read` | `{}` | One (x,y,z) sample, blocking |
| `ai.reality2.cap.accel.read-burst` | `{ max_samples: u8 }` | Drain FIFO (up to 96 samples × 9 B) |
| `ai.reality2.cap.accel.set-odr` | `{ odr_hz }` | Change ODR (4000 / 2000 / 1000 / 500 / 250 / 125 / 62.5 / 31.25 / 15.625 / 7.813 / 3.906 Hz) |
| `ai.reality2.cap.accel.set-range` | `{ range_g }` | Change ±g range (2 / 4 / 8 — note: 16 NOT supported on ADXL355) |
| `ai.reality2.cap.accel.set-offset` | `{ x: i16, y: i16, z: i16 }` | Per-axis offset calibration |
| `ai.reality2.cap.accel.sleep` | `{}` | Standby mode |

## 4. Events Emitted (Outbound)

Per R2-PLUGIN §2.4 envelope. Default event name: `sensor/adxl355`.

| Status | Data | Notes |
|---|---|---|
| `"ok"` (read) | `{ command: "read", x: i32, y: i32, z: i32, ts_ms: u32 }` | 20-bit sample sign-extended to i32, then scaled to the **256_000-LSB-per-g** convention (1 g at ±2 g range = 256_000 LSB) |
| `"ok"` (read-burst) | `{ samples: [...], count }` | |
| `"ok"` (config) | `{ command: "init"\|"set-odr"\|... }` | |
| `"error"` | `error: "spi_bus"` | SPI transaction failed |
| `"error"` | `error: "devid_mismatch"` | DEVID/PARTID != expected (chip not ADXL355) |
| `"error"` | `error: "bad_range"` | Range request outside {2, 4, 8} |
| `"error"` | `error: "bad_odr"` | ODR not in supported set |

## 5. Configuration

```yaml
data:
  odr_hz: 1000             # default — r2-workshop spec'd 1 kHz with 100:1 decimation
  range_g: 2               # ±2 g
  cal_offset: { x: 0, y: 0, z: 0 }
```

## 6. Example Sentants

The rocker-sensor ensemble's [`Accelerometer`](../../../sentants/Accelerometer/) sentant binds this plugin by capability (chip-agnostic). Same yaml as the lis2dh example, resolved differently per carrier — sentant unchanged.

## 7. Hardware / Host Requirements

| Requirement | Detail |
|---|---|
| SPI bus | One SPI bus, 10 MHz max (CPOL=0, CPHA=0) |
| Supply | 2.25–3.6 V; EVAL-ADXL355-PMDZ accepts 3.3 V |
| Pin assignments | Per carrier — for `esp32-s3-devkitc`: CS=GPIO10, MOSI=11, SCK=12, MISO=13, DRDY=14 |
| Datasheet | `datasheets/adxl355-datasheet.pdf` (not yet fetched) |
| Vendor breakout | EVAL-ADXL355-PMDZ Pmod (Analog Devices) |

## 8. Credentials

None.

## 9. Known Limitations

- **Source not yet extracted** — reference at `r2-workshop/firmware/esp32-s3/{devkitc,xiao}/src/adxl355.rs`.
- **Datasheet PDF not yet fetched** — pending the authoring-flow WebFetch.
- **FIFO mode unsupported in v0.1** (per-sample polling).
- **20-bit data scaling** — driver rescales to the 256_000-LSB-per-g convention so downstream code is chip-agnostic; raw 20-bit access not exposed.
- **±16 g range** not supported by chip (the LIS2DH supports it; the ADXL355 doesn't — exposes only ±2/4/8 g).
- **DRDY interrupt** wired-but-unused; firmware polls.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft authored in r2-compiler session 02 alongside the rest of Phase 1.4-metadata-rest. |
