# data-tcp

**Version:** 0.1.0 · **Modes:** `aot` (esp32-s3 only) · **Conformance:** R2-PLUGIN §12

## 1. Purpose

TCP server on port **21047** exposing a small command protocol (LIST / GET / DEL / DEL_ALL) over the SD card's captures directory. The dashboard downloads named capture files (`<ts16>-<name>.csv`) through this server. SPEC-R2-WORKSHOP-CAPTURE §6.

## 2. Modes & Platforms

`aot` for esp32-s3 (depends on `storage/sd-card`, which is currently esp32-s3-only).

## 3. Events Handled

| Event | Parameters | Purpose |
|---|---|---|
| `r2.dash.data.list` | `{}` | (Dashboard can fall back to event-driven list when TCP unavailable.) |
| `r2.dash.data.get` | `{ name }` | |
| `r2.dash.data.delete` | `{ name }` | |

## 4. Events Emitted

| Status | Data |
|---|---|
| `"ok"` (init) | `{ listening_port: 21047 }` |
| `"ok"` (op_complete) | `{ op, name?, bytes? }` |
| `"error"` | `error: "enoent" / "eio" / "ebadf"` |

## 5. Configuration

```yaml
data:
  port: 21047
  captures_dir: "/sdcard/captures"
```

## 6. Example Sentants

[`Capture`](../../../sentants/Capture/) writes the files; this plugin serves them. No dedicated sentant — the dashboard talks to this plugin directly over TCP.

## 7. Hardware / Host Requirements

- TCP on the WiFi STA.
- A mounted SD card (depends on `../storage/sd-card`).

## 8. Credentials

None. LAN-trust model.

## 9. Known Limitations

- **Source not yet extracted**.
- **No directory tree** — flat `/sdcard/captures/` only.
- **No range support** in GET — full-file transfer per request. Large files (>10 MB) take noticeable time on a 25 MHz SD bus over a 2.4 GHz WiFi link.
- **dfr1117 not supported** — depends on the sd-card plugin, which itself is currently esp32-s3-only.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft. |
