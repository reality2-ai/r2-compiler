# log-tcp

**Version:** 0.1.0 · **Modes:** `aot` · **Conformance:** R2-PLUGIN §12

## 1. Purpose

TCP fan-out of the firmware's log lines on port **21046**. Multiple dashboard clients (or `nc 192.168.x.x 21046`) can tail the live log. Implements SPEC-R2-WORKSHOP-SENSOR-LIVE-LOGS.

## 2. Modes & Platforms

`aot` (esp32-s3, esp32-c6).

## 3. Events Handled

| Event | Parameters |
|---|---|
| `r2.log.line` | `{ level, msg }` — internal; the platform's `log::*!` macros are wired through this plugin |

## 4. Events Emitted

| Status | Data |
|---|---|
| `"ok"` (init) | `{ listening_port: 21046, max_clients: 4 }` |
| `"ok"` (client_connected/client_disconnected) | `{ remote }` |

## 5. Configuration

```yaml
data:
  port: 21046
  max_clients: 4
  buffer_lines: 200       # ring buffer; new clients get the last N lines on connect
```

## 6. Example Sentants

The dashboard's per-device "View logs" pane connects here. No sentant on the device needs to subscribe — the platform log macros are wired in at firmware init by the [`Status`](../../../sentants/Status/) sentant's setup.

## 7. Hardware / Host Requirements

- TCP on the device's WiFi STA.

## 8. Credentials

None. LAN-trust model identical to `reset-tcp`.

## 9. Known Limitations

- **Source not yet extracted** — reference at `r2-workshop/firmware/esp32-s3/{devkitc,xiao}/src/...` (logging fan-out is integrated with the sender today).
- **No persistence** — past log lines lost beyond the in-memory buffer.
- **No level filter** — client gets all levels; dashboard does the filtering.
- **Bound to one log target** — adding multiple log destinations (UART + TCP + file) would require a multiplexer, not addressed here.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft. |
