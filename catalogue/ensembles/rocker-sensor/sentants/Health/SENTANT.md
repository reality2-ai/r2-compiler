# Health sentant

**Class:** `ai.reality2.workshop.sensor.health` · **Storage:** `ephemeral`

Watchdog. Tracks last-arrived sample timestamp; if no acceleration arrives in 2 s while in real mode, flags `data_source = "sim"` so the dashboard sees the degradation. SPEC-R2-WORKSHOP-SENSOR-HEALTH §6.

## Events

| Direction | Event |
|---|---|
| inbound | `init`, `r2.sensor.acceleration` (every sample), `watchdog_tick` (self) |
| outbound | `set_data_source { source: "sim" }` |

## Reference

Distributed across `r2-workshop` firmware — Phase 1.4-source consolidates into a clean Health module.

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
