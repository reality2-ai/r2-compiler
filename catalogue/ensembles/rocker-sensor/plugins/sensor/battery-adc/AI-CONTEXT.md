# AI-CONTEXT.md — sensor/battery-adc

## Purpose

ADC1 reader for the LiPo cell voltage through a 100 kΩ/100 kΩ divider. Emits millivolts; the [`Battery`](../../../sentants/Battery/) sentant polls every 30 s and forwards `r2.sensor.battery` to the dashboard.

## Conformance

R2-PLUGIN §12. Categorised under `sensor/` (the plugin senses battery voltage); could justify a future `power/` category — flagged in `[[feedback-plugin-category-claims]]` but `sensor/` is the v0.1 placement.

## Hardware

- ADC1 channel only (ADC2 unusable while WiFi active).
- 100 kΩ / 100 kΩ divider; 100 nF cap from ADC node to GND on ESP32-S3.
- GPIO4 on all three current carriers.

## Reference

`r2-workshop/firmware/esp32-{s3,c6}/<carrier>/src/battery.rs`.

## Read in order

1. plugin.toml · 2. PLUGIN.md · 3. reference battery.rs · 4. R2-PLUGIN §12

## Authoring status

- ✅ plugin.toml · ✅ PLUGIN.md · ✅ AI-CONTEXT.md · ⏳ Cargo.toml + src/ (Phase 1.4-source)
