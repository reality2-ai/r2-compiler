# lis2dh

**Version:** 0.1.0 (metadata draft)
**Modes:** `aot` only (MCU firmware — esp32-c6, esp32-s3)
**Conformance:** R2-PLUGIN §12 — all 10 sections required per §12.8

## 1. Purpose

STMicroelectronics LIS2DH 3-axis MEMS accelerometer over I²C. Provides the generic `ai.reality2.cap.accel.triaxial` capability — the swap-lever in r2-workshop's sensor pipeline (R2-PLUGIN §10) — so the Accelerometer sentant can read calibrated `(x, y, z)` acceleration without knowing which chip is wired underneath.

10-bit resolution, ±2/4/8/16 g programmable range, ODR up to 5.3 kHz. Lower-precision peer to `adxl355` (SPI, 20-bit, used by the ESP32-S3 carriers); the LIS2DH ships on a 4-pin Gravity I²C connector (DFRobot SEN0224) which removes the per-device soldering step and is the canonical sensing element for the `dfr1117` carrier.

## 2. Modes & Platforms

| Mode | Targets | Status |
|---|---|---|
| `aot` | `esp32-c6`, `esp32-s3` | Primary (only mode in v0.1) |
| `nif` | — | Not supported in v0.1; could be added for Linux-SBC bench testing |
| `web` | — | Not applicable — hardware driver |

Must match `plugin.toml` `[modes]`. Currently does.

## 3. Events Handled (Inbound)

| Event class | Parameters | Purpose |
|---|---|---|
| `ai.reality2.cap.accel.init` | `{ odr_hz: number, range_g: number, offset?: {x, y, z} }` | Configure ODR + range; write calibration offsets; check WHO_AM_I (0x33) |
| `ai.reality2.cap.accel.read` | `{}` | Read one `(x, y, z)` sample (blocking on I²C transaction) |
| `ai.reality2.cap.accel.read-burst` | `{ max_samples: u8 }` | Drain FIFO up to `max_samples` (≤32) |
| `ai.reality2.cap.accel.set-odr` | `{ odr_hz: number }` | Change output data rate (1, 10, 25, 50, 100, 200, 400, 1344, 5376 Hz) |
| `ai.reality2.cap.accel.set-range` | `{ range_g: number }` | Change ±g range (2, 4, 8, 16) |
| `ai.reality2.cap.accel.set-offset` | `{ x: i16, y: i16, z: i16 }` | Per-axis calibration offset |
| `ai.reality2.cap.accel.sleep` | `{}` | Enter low-power mode (no sampling) |

## 4. Events Emitted (Outbound)

Per R2-PLUGIN §2.4 result envelope. Default event name = plugin name (`sensor/lis2dh`).

| Event class | Status | Data (when status="ok") | Description |
|---|---|---|---|
| `sensor/lis2dh` | `"ok"` | `{ command: "init", who_am_i: 0x33 }` | Init succeeded |
| `sensor/lis2dh` | `"ok"` | `{ command: "read", x: i32, y: i32, z: i32, ts_ms: u32 }` | Single sample, raw 10-bit left-justified into i32, timestamp from hive clock |
| `sensor/lis2dh` | `"ok"` | `{ command: "read-burst", samples: [{x, y, z, seq}], count: u8 }` | FIFO drain result |
| `sensor/lis2dh` | `"ok"` | `{ command: "set-odr"\|"set-range"\|"set-offset"\|"sleep" }` | Configuration command succeeded |
| `sensor/lis2dh` | `"error"` | `error: "i2c_no_ack"` | I²C device did not ACK its address (sensor unplugged / address wrong) |
| `sensor/lis2dh` | `"error"` | `error: "who_am_i_mismatch"` | WHO_AM_I register returned a value ≠ 0x33 (chip not LIS2DH) |
| `sensor/lis2dh` | `"error"` | `error: "bad_range"` | `set-range` called with a non-{2,4,8,16} value |
| `sensor/lis2dh` | `"error"` | `error: "bad_odr"` | `set-odr` called with an unsupported value |

## 5. Configuration

Default config baked into firmware at compile time; overridable from the consuming sentant's `data:` block per R2-DEF §2.3:

```yaml
data:
  odr_hz: 100              # default — matches r2-workshop's default sample rate
  range_g: 2               # ±2 g default (good for SHM-class signal range)
  i2c_address: 0x18        # SA0 strapped low on the SEN0224 board
  fifo_enabled: false      # v0.1 polls per-sample; FIFO support is a follow-up
  cal_offset:
    x: 0
    y: 0
    z: 0
```

## 6. Example Sentants

The rocker-sensor ensemble's `Accelerometer` sentant binds this plugin via capability (NOT chip name) per R2-PLUGIN §10:

```yaml
sentant:
  name: Accelerometer
  class: nz.ac.auckland.rocker.accelerometer
  plugins:
    - capability: ai.reality2.cap.accel.triaxial
  automations:
    - name: main
      transitions:
        - from: start
          event: init
          to: running
          actions:
            - plugin: lis2dh                    # resolved by the orchestrator from the capability binding
              command: init
              parameters:
                odr_hz: 100
                range_g: 2
        - from: running
          event: sample_tick
          actions:
            - plugin: lis2dh
              command: read
        - from: running
          event: sensor/lis2dh                  # default result-event name per R2-PLUGIN §2.4
          actions:
            - command: send
              parameters:
                event: r2.sensor.acceleration
                public: true
                x: "{{params.data.x}}"
                y: "{{params.data.y}}"
                z: "{{params.data.z}}"
                ts_ms: "{{params.data.ts_ms}}"
```

## 7. Hardware / Host Requirements

| Requirement | Detail |
|---|---|
| I²C bus | One bus, default 100 kHz (LIS2DH supports up to 400 kHz fast-mode) |
| I²C address | `0x18` (SA0 = GND on the SEN0224 board) or `0x19` (SA0 = VDD) |
| Supply voltage | 3.3 V (SEN0224 board has level shifters tolerant of 3.0–5 V) |
| Pin assignments | Per the chosen carrier's `board.toml [pinout]` — for `esp32-c6-dfr1117`: SDA=GPIO19, SCL=GPIO20 |
| Datasheet | `datasheets/lis2dh-datasheet.pdf` (not yet fetched — see §10 known limitations) |
| Vendor info | DFRobot SEN0224 product wiki: https://wiki.dfrobot.com/Gravity__I2C_Triple_Axis_Accelerometer_-_LIS2DH_SKU__SEN0224 |

## 8. Credentials

None. Local hardware driver, no remote endpoints, no API keys.

## 9. Known Limitations

- **Source not yet extracted.** This entry is metadata-only as of 2026-05-31. The working implementation lives inline at `../../../../../../r2-workshop/firmware/esp32-c6/dfr1117/src/lis2dh.rs`. Extraction into this standalone Cargo crate is pending — Phase 1.4-source in `plan/PLAN.md`.
- **Datasheet PDF not yet fetched.** The authoring flow (Phase 2+) will fetch ST's LIS2DH datasheet via WebFetch and save it under `datasheets/`.
- **FIFO mode unsupported in v0.1.** The driver polls per-sample. FIFO would reduce I²C bus contention at higher ODRs; deferred until needed.
- **Interrupt-driven sampling unsupported.** INT1 / INT2 are wired-but-unused; v0.1 polls. The DFRobot SEN0224's 4-pin Gravity connector doesn't expose the interrupts anyway — they're on a separate header.
- **Calibration is per-axis offset only.** No matrix calibration (the rocker dashboard does the matrix calibration server-side per `SPEC-R2-WORKSHOP-SENSOR` §9). Sufficient for the rocker rig's measurement model.
- **NIF mode (Linux SBC bench testing) not supported.** Could be added by feature-gating the `esp-idf-hal` I²C dep with a `linux-embedded-hal` alternative; out of scope for v0.1.
- **No graceful fall-back to simulator.** Per `SPEC-R2-WORKSHOP-SENSOR-HEALTH`, the consuming Accelerometer sentant is expected to fall back to a built-in simulator when this plugin fails to enumerate — but that's the sentant's responsibility, not this plugin's. This plugin reports the error envelope; the sentant decides.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft. `plugin.toml`, this `PLUGIN.md`, `AI-CONTEXT.md` authored in r2-compiler session 02 as the first worked-example plugin entry. No source / Cargo.toml / datasheets yet. |
