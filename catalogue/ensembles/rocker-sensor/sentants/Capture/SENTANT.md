# Capture sentant

**Class:** `ai.reality2.workshop.sensor.capture` · **Storage:** `durable-state`

The Idle / Calibrating / Recording state machine per SPEC-R2-WORKSHOP-CAPTURE §2. Owns named capture-file writes alongside the always-on ring buffer. Sidecar event-marks per §7.5.

## FSM

```
idle → recording → idle    (capture.start / capture.stop)
idle → calibrating → idle  (cal.sample.req / cal_done)
```

## Plugins

`sd-card`, `clock`.

## Platform-extensions

- `{{platform.capture_filename(name)}}` — builds `<ts16>-<name>.csv`
- `{{platform.csv_line(params)}}` — formats one acceleration row
- `{{platform.event_mark_csv(params)}}` — formats an event-mark row

## Events

| Direction | Event |
|---|---|
| inbound | `init`, `r2.dash.capture.start/stop/mark/event_mark`, `r2.dash.cal.sample.req`, `r2.sensor.acceleration`, `storage/sd-card` (results) |
| outbound | `r2.sensor.cal.sample.resp` (public), `set_status_state` |

## Reference

`r2-workshop/firmware/esp32-s3/<carrier>/src/capture.rs`.

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
