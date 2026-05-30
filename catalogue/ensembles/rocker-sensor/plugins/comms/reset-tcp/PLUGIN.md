# reset-tcp

**Version:** 0.1.0 · **Modes:** `aot` · **Conformance:** R2-PLUGIN §12

## 1. Purpose

TCP listener on port **21044** accepting a single byte 0x52 ("R" for Reset). On receipt → `esp_restart()`. Implements SPEC-R2-WORKSHOP-SENSOR-REMOTE-RESET. Used by the dashboard when a sensor wedges and the operator needs to power-cycle remotely without USB access.

## 2. Modes & Platforms

| Mode | Targets |
|---|---|
| `aot` | `esp32-s3`, `esp32-c6` |

## 3. Events Handled

| Event | Parameters | Purpose |
|---|---|---|
| `r2.dash.reset` | `{}` | Reset request from the dashboard (delivered over R2-WIRE; the plugin's TCP listener is an alternative path the dashboard uses when R2-WIRE is broken) |

## 4. Events Emitted

| Status | Data |
|---|---|
| `"ok"` (init) | `{ listening_port: 21044 }` |
| `"ok"` (triggered) | `{ source: "tcp"\|"event" }` — emitted right before `esp_restart` so the log captures it |
| `"error"` | `error: "bind_failed"` |

## 5. Configuration

```yaml
data:
  port: 21044
```

## 6. Example Sentants

[`Reset`](../../../sentants/Reset/) subscribes both `r2.dash.reset` (R2-WIRE path) and falls through to this plugin's TCP listener if R2-WIRE silence detected.

## 7. Hardware / Host Requirements

- TCP on the device's primary network interface (WiFi STA).

## 8. Credentials

None. **Trust comes from network ACL** — only TG members can reach the device's port 21044 (the relay forwarder gates the connection at the dashboard side). Operators on the same WiFi LAN have raw access; this is acceptable in the r2-workshop trust model.

## 9. Known Limitations

- **Source not yet extracted** — small module in r2-workshop firmware.
- **No auth byte beyond CMD_RESET=0x52** — fine in the LAN trust model, weak if exposed to the open internet.
- **No graceful-shutdown step** — `esp_restart` is hard. Capture-file sync state on the SD card is preserved by FATFS journaling; in-memory state is lost.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft. |
