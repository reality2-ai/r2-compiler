# Sync sentant

**Class:** `ai.reality2.workshop.sensor.sync` · **Storage:** `durable-state`

Cristian's algorithm. Dashboard sends `r2.dash.sync_pulse{pulse_id}`; sensor stamps with `clock.monotonic_ms` and replies `r2.sensor.sync_pong{pulse_id, ts_ms_local}`. Dashboard computes RTT + offset and sends `r2.dash.set_clock_offset{offset_ms}` back; sensor writes it via `clock.set_offset`.

Targets ~5 ms accuracy.

## Plugins

`clock`.

## Events

| Direction | Event |
|---|---|
| inbound | `r2.dash.sync_pulse`, `r2.dash.set_clock_offset` |
| outbound | `r2.sensor.sync_pong` (public) |

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
