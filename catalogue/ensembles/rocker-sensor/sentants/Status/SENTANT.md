# Status sentant

**Class:** `ai.reality2.workshop.sensor.status` · **Storage:** `ephemeral`

Emits `r2.sensor.status` every 2 s — FSM state + data_source + seq watermark + uptime. Drives the dashboard's virtual LED (mirrors the physical one) + diagnostic pane. Centralises LED control: other sentants emit `set_status_state` rather than calling `led` directly.

## Plugins

`led`, `clock`.

## Events

| Direction | Event |
|---|---|
| inbound | `init`, `tick` (self-tick), `clock` (result), `set_status_state` / `set_data_source` (from other sentants), `r2.sensor.acceleration` (to track high-water seq) |
| outbound | `r2.sensor.status` (public) |

## Reference

State indicator logic is split across `led.rs` + bits of `main.rs` in r2-workshop firmware. Phase 1.4-source consolidates into a clean Status FSM that calls the led plugin.

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
