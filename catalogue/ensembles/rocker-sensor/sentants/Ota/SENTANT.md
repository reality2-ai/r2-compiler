# Ota sentant

**Class:** `ai.reality2.workshop.sensor.ota` · **Storage:** `ephemeral`

Subscribes `r2.dash.fw.update`, delegates to the **core** `ota-tcp` plugin (now at `crates/r2-plugin-comms-ota-tcp/`). On `phase: "reboot"` from the plugin, schedules a clean reset.

Note: `mark_app_valid` is Uplink's responsibility (after first frame round-trips post-reboot per SENSOR §12.2), NOT this sentant's. Don't confuse the two.

## Plugins

`ota-tcp` (core, in crates/).

## Events

| Direction | Event |
|---|---|
| inbound | `init`, `r2.dash.fw.update`, `comms/ota-tcp` (plugin result/progress) |
| outbound | `set_status_state { state: "Ota" }` (to Status), `r2.dash.reset` (delayed), `r2.dash.fw.progress` (public progress) |

## Reference

`r2-workshop/firmware/esp32-s3/<carrier>/src/sender.rs` (OTA branch in the RX loop).

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
