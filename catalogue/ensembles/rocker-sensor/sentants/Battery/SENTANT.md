# Battery sentant

**Class:** `ai.reality2.workshop.sensor.battery` · **Storage:** `ephemeral`

Polls `battery-adc` every 30 s, emits `r2.sensor.battery` (public), and trips the LED + `power_state_changed` when cell voltage < 3.3 V.

## Plugins

`battery-adc` (opt-in), `led` (opt-in — likely reclassified in the second pass).

## Events

| Direction | Event |
|---|---|
| inbound | `init`, `poll` (self-tick), `sensor/battery-adc` (plugin result) |
| outbound | `r2.sensor.battery` (public), `power_state_changed` (consumed by Beacon) |

## Reference

`r2-workshop/firmware/esp32-{s3,c6}/<carrier>/src/battery.rs`.

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
