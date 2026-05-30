# Reset sentant

**Class:** `ai.reality2.workshop.sensor.reset` · **Storage:** `ephemeral`

Handles `r2.dash.reset` (R2-WIRE path) AND the `reset-tcp` plugin's CMD_RESET byte (TCP path). Both flow into a 500 ms-delayed `reboot_now` which calls `platform.esp_restart()`.

## Plugins

`reset-tcp` (opt-in — may be reclassified as core in the second pass, since "every device should be remotely resettable" is a strong argument).

## Events

| Direction | Event |
|---|---|
| inbound | `init`, `r2.dash.reset`, `comms/reset-tcp`, `reboot_now` (self) |
| outbound | `set_status_state { state: "Boot" }` (LED feedback) |

## Reference

Inline in r2-workshop firmware; see SPEC-R2-WORKSHOP-SENSOR-REMOTE-RESET in the workshop spec dir.

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
