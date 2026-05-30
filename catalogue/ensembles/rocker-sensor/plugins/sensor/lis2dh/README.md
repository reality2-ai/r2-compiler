# r2-plugin-sensor-lis2dh

ST LIS2DH 3-axis accelerometer plugin for the R2 platform. Provides the generic `ai.reality2.cap.accel.triaxial` capability — same capability as `r2-plugin-sensor-adxl355`, different chip + bus, swappable per R2-PLUGIN §10 without sentant changes.

| | |
|---|---|
| Crate name | `r2-plugin-sensor-lis2dh` |
| Provides | `ai.reality2.cap.accel.triaxial`, `ai.reality2.cap.accel.triaxial.10bit` |
| Bus | I²C, addresses `0x18` or `0x19` |
| Mode | `aot` only (MCU firmware) |
| Reference carrier | `esp32-c6-dfr1117` (uses DFRobot SEN0224 4-pin Gravity I²C module) |

See [`PLUGIN.md`](PLUGIN.md) for the full R2-PLUGIN §12.8 interface contract (10 sections including events, configuration, hardware requirements, limitations).

See [`AI-CONTEXT.md`](AI-CONTEXT.md) for the fresh-CC brief — how to pick up this plugin's work cold.

## Status (2026-05-31)

Metadata draft. `plugin.toml` + `PLUGIN.md` + `AI-CONTEXT.md` authored as a worked example. Cargo crate (Cargo.toml + src/) not yet present — source extraction from `r2-workshop/firmware/esp32-c6/dfr1117/src/lis2dh.rs` is Phase 1.4-source work.

## Quick links

- Datasheet: STMicroelectronics LIS2DH (see vendor page); local copy under [`datasheets/`](datasheets/) when fetched.
- Vendor SEN0224 product wiki: https://wiki.dfrobot.com/Gravity__I2C_Triple_Axis_Accelerometer_-_LIS2DH_SKU__SEN0224
- Reference implementation (current authoritative source): `r2-workshop/firmware/esp32-c6/dfr1117/src/lis2dh.rs`
- Capability-peer driver: [`../adxl355/`](../adxl355/) (not yet scaffolded — same `provides` capability, different bus + precision)
