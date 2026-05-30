# led

**Version:** 0.1.0 · **Modes:** `aot` · **Category:** `indicator/` (new — see [[feedback-plugin-category-claims]]) · **Conformance:** R2-PLUGIN §12

## 1. Purpose

Status LED driver. Abstracts the carrier's available LED — addressable RGB on the DevKitC (WS2812 on GPIO38 v1.1), mono PWM on the XIAO (GPIO21) and DFR1117 (GPIO15) — behind one event interface. The Status sentant calls `set_state(<state>)`; the plugin renders appropriately for the carrier.

## 2. Modes & Platforms

`aot` esp32-s3 + esp32-c6. The compiler plugin links a different backend per carrier per `board.toml [pinout]` `role_hint`:
- `role_hint = "status-led-ws2812"` → RMT-driven WS2812 (DevKitC)
- `role_hint = "status-led"` → LEDC PWM (XIAO, DFR1117)

## 3. Events Handled

| Event | Parameters | Purpose |
|---|---|---|
| `r2.led.set_state` | `{ state: "Boot"\|"Advertising"\|"ConnectingWifi"\|"Calibrating"\|"Streaming"\|"CatchingUp"\|"LowBattery"\|"Error"\|"Ota" }` | High-level state mapping |
| `r2.led.set_color` | `{ r: u8, g: u8, b: u8 }` | Raw colour (ignored on mono carriers — clamped to brightness) |
| `r2.led.off` | `{}` | All LEDs off |

## 4. Events Emitted

| Status | Data |
|---|---|
| `"ok"` | `{}` |
| `"error"` | `error: "ws2812_rmt_fail" / "ledc_init_fail"` |

## 5. Configuration

```yaml
data:
  brightness_cap: 0.20   # 20% maximum (r2-workshop's calm-tech ceiling)
  state_table:           # informational — per-state colour/blink-rate mapping
    Boot:           { rgb: [255,255,255], blink_hz: 0 }      # solid white briefly
    Advertising:    { rgb: [0,0,255],     blink_hz: 1 }      # blue slow pulse
    ConnectingWifi: { rgb: [0,255,255],   blink_hz: 4 }      # cyan fast pulse
    Calibrating:    { rgb: [128,0,255],   blink_hz: 0 }      # purple solid
    Streaming:      { rgb: [0,255,0],     blink_hz: "60bpm" }# green heartbeat
    CatchingUp:     { rgb: [255,255,0],   blink_hz: "60bpm" }# yellow heartbeat
    LowBattery:     { rgb: [255,128,0],   blink_hz: 1 }      # orange slow pulse
    Error:          { rgb: [255,0,0],     blink_hz: 4 }      # red fast pulse
    Ota:            { rgb: [255,255,255], blink_hz: 8 }      # white fast strobe
```

On mono carriers the RGB tuple is collapsed to a brightness scalar (sum of channels / 3), and only blink rate distinguishes states.

## 6. Example Sentants

[`Status`](../../../sentants/Status/) sets the LED state on every FSM transition.

## 7. Hardware / Host Requirements

- GPIO output pin per the carrier's `board.toml [pinout]` (`role_hint = "status-led*"`).
- RMT peripheral (for WS2812 carriers).
- LEDC PWM (for mono carriers).
- Brightness capped at 20% — uncapped is retinopathic indoors (r2-workshop convention).

## 8. Credentials

None.

## 9. Known Limitations

- **Source not yet extracted** — per-carrier `led.rs` modules in r2-workshop (different WS2812 vs LEDC variants).
- **State mapping fixed in firmware** — operators can't reconfigure colours without rebuild.
- **No animation queueing** — `set_state` is fire-and-forget; the previous state is preempted instantly.
- **xiao currently ships with external WS2812** per r2-workshop — but the design intent (per [[project-xiao-led-choice]]) is mono GPIO21 LEDC PWM. The Compiler-plugin must use the GPIO21 path per `board.toml` regardless of the synced Cargo.toml.tera's lagging deps.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft. Claims new `indicator/` category — flag for upstream R2-PLUGIN §12.2 amendment. |
