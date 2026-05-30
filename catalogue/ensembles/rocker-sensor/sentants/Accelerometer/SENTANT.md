# Accelerometer sentant

**Class:** `nz.ac.auckland.rocker.accelerometer` · **Storage:** `durable-state` · **Compilable:** ✅ R2-COMPILE §3.1

## Purpose

THE domain sentant of the rocker-sensor ensemble. Reads triaxial acceleration at the configured ODR from whichever sensing plugin provides `ai.reality2.cap.accel.triaxial`, applies a per-axis calibration offset, stamps with the clock's `now_ms`, and emits `r2.sensor.acceleration`.

The deployment-specific sentant — a sibling deployment (people-counter, pet-gait) replaces this with its own domain logic + capability binding (e.g. `ai.reality2.cap.passive-infrared`). The substrate sentants (Identity, Bootstrap, Beacon, Uplink, …) stay unchanged across deployments.

## FSM

States: `start → configuring → running` (plus `*` for the calibration-update transition).

## Plugins used

- **`@capability:ai.reality2.cap.accel.triaxial`** — bound by CAPABILITY not by chip. Resolves to [`adxl355`](../../plugins/sensor/adxl355/) on ESP32-S3 carriers, [`lis2dh`](../../plugins/sensor/lis2dh/) on dfr1117. Swap lever per R2-PLUGIN §10.
- **`clock`** — for `now_ms` timestamps. (Likely to be reclassified as core in the upcoming pass.)

## Events emitted / consumed

| Direction | Event | Public |
|---|---|---|
| inbound | `init`, `sample_tick` (self-send), `set_calibration` | no |
| inbound | `@capability:...accel.triaxial`, `clock` (plugin results) | no |
| outbound | `r2.sensor.acceleration { x, y, z, ts_ms }` | **yes** — primary deliverable |

## Health behaviour

On `init` if the sensing plugin returns an error, `data_source` flips to `"sim"` and the FSM continues with a built-in simulator. The Status sentant surfaces `data_source` to the dashboard via `r2.sensor.status` per SPEC-R2-WORKSHOP-SENSOR-HEALTH.

## Reference

`r2-workshop/firmware/esp32-c6/dfr1117/src/sender.rs` (the sender thread that drives the sample loop). Phase 1.4-source extracts this into a generated FSM matching the YAML above.

## AOT compilation notes

- `@capability:` binding resolved at compile time → direct calls to the resolved plugin's `execute(opcode, …)`.
- Self-tick (`send { event: sample_tick, delay: X }`) compiles to a one-shot timer per R2-DEF §3.3.1 delayed-send.
- `data_source` flip is a single `enum DataSource { Real, Sim }` field in the generated Rust struct.

## Authoring status

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
