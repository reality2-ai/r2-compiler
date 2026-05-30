# AI-CONTEXT.md — sentants/Accelerometer

THE domain sentant of rocker-sensor. Binds `ai.reality2.cap.accel.triaxial` by CAPABILITY (not chip), emits `r2.sensor.acceleration` at the configured ODR. Falls back to simulator on sensing-plugin failure (SPEC-R2-WORKSHOP-SENSOR-HEALTH).

## Capability binding

The yaml uses `@capability:ai.reality2.cap.accel.triaxial` instead of a concrete `name:`. The compiler plugin resolves: ESP32-S3 carrier → `adxl355`; dfr1117 → `lis2dh`. Sentant unchanged either way.

## Read in order

1. sentant.yaml · 2. SENTANT.md · 3. r2-workshop/firmware/esp32-c6/dfr1117/src/sender.rs (reference) · 4. R2-PLUGIN §10 (capability swap lever)

## Authoring status

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
