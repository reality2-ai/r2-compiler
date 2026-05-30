# battery-adc

**Version:** 0.1.0 (metadata draft) · **Modes:** `aot` only · **Conformance:** R2-PLUGIN §12

## 1. Purpose

Single-channel ADC reader for the LiPo battery cell. Drives an ESP32 ADC1 channel through a 100 kΩ / 100 kΩ voltage divider (cell voltage 3.0–4.2 V → ADC input 1.5–2.1 V → reading 1870–2620 at 12-bit / 12 dB attenuation). Emits the cell voltage in millivolts; downstream sentants compute charge state.

## 2. Modes & Platforms

| Mode | Targets |
|---|---|
| `aot` | `esp32-s3`, `esp32-c6` |

ADC2 (GPIO11-20 on ESP32-S3) is unusable while WiFi is active — this plugin uses **ADC1 only**.

## 3. Events Handled

| Event | Parameters | Purpose |
|---|---|---|
| `r2.hw.adc.battery.init` | `{ gpio: u8, attenuation: str }` | Configure the ADC channel |
| `r2.hw.adc.battery.read` | `{}` | Sample voltage |
| `r2.hw.adc.battery.calibrate` | `{ known_mv: u32 }` | Run point calibration (operator measures with multimeter) |

## 4. Events Emitted

| Status | Data | Notes |
|---|---|---|
| `"ok"` (read) | `{ raw_u12: u16, millivolts: u32, ts_ms: u32 }` | Raw + scaled |
| `"error"` | `error: "adc_init"` | Init failed (wrong attenuation, bad pin, ADC2 attempted with WiFi up) |

## 5. Configuration

```yaml
data:
  gpio: 4                # GPIO4 on all three v0.1 carriers (ADC1_CH3)
  attenuation: "12dB"    # 0-3.3 V range
  divider_ratio: 0.5     # R2 / (R1 + R2) — informs voltage-from-raw scaling
```

## 6. Example Sentants

The [`Battery`](../../../sentants/Battery/) sentant polls this plugin every 30 s and emits `r2.sensor.battery` events.

## 7. Hardware / Host Requirements

- ADC1 channel on an ESP32-family target.
- 100 kΩ / 100 kΩ divider between cell positive and GND. 100 nF cap from ADC node to GND **required** on ESP32-S3 (high Thévenin impedance + ADC S/H — see `r2-workshop/specifications/HARDWARE-WIRING-DEVKITC.md` §4.2).
- Per-carrier pin: GPIO4 on devkitc / xiao / dfr1117 (all by coincidence).

## 8. Credentials

None.

## 9. Known Limitations

- **Source not yet extracted** — reference at `r2-workshop/firmware/esp32-{s3,c6}/<carrier>/src/battery.rs`.
- **Single point of calibration only** — no curve fit; assumes the divider is exactly 0.5.
- **No averaging** — single-shot read; consumer sentant smooths if needed.
- **ADC2 not supported** — would require an alternate path when WiFi is off; out of scope.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft. |
