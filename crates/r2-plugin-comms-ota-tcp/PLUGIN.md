# ota-tcp

**Version:** 0.1.0 · **Modes:** `aot` (esp32-s3, esp32-c6) · **Conformance:** R2-PLUGIN §12 · **COMPULSORY** per SPEC-R2-COMPILER §12.1

## 1. Purpose

TCP listener on port **21043** receiving a new firmware image, staging it to the inactive OTA partition (`esp_ota_*` APIs), verifying SHA-256, marking the new slot bootable, and triggering `esp_restart()`. The bootloader's rollback gate handles failures — if the new image can't talk to the dashboard within the rollback window, the bootloader reverts to the previous slot.

Compulsory per SPEC-R2-COMPILER §12.1 — every MCU build links this plugin regardless of operator selection.

## 2. Modes & Platforms

| Mode | Targets |
|---|---|
| `aot` | `esp32-s3`, `esp32-c6` (uses `esp_idf_svc::ota` — `no_std = false` for this plugin alone) |

## 3. Events Handled

| Event | Parameters | Purpose |
|---|---|---|
| `r2.dash.fw.update` | `{}` | Listener started; subsequent TCP connection drives the OTA session |
| `r2.deploy.ota.abort` | `{}` | Tear down in-progress session, discard partial slot |

## 4. Events Emitted

| Status | Data | Notes |
|---|---|---|
| `"ok"` (init) | `{ listening_port: 21043 }` | |
| `"ok"` (progress) | `{ phase: "receiving"\|"verifying"\|"swapping"\|"reboot", bytes_written?, total? }` | Streamed during a session |
| `"error"` | `error: "sha256_mismatch" / "partition_full" / "bind_failed" / "remote_closed"` | |

## 5. Configuration

```yaml
data:
  port: 21043
  max_image_size_bytes: 1966080   # 1.875 MB — matches partitions.csv ota_0/ota_1 size on dfr1117; S3 carriers have 3 MB slots
```

## 6. Example Sentants

The [`Ota`](../../../sentants/Ota/) sentant subscribes `r2.dash.fw.update` and uses this plugin. On `"ok" reboot` the sentant logs + calls `esp_restart` (delegated through this plugin).

## 7. Hardware / Host Requirements

- ESP32 family chip with `ota_0` + `ota_1` partitions per `partitions.csv`.
- `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` in `sdkconfig.defaults` (set on all three carriers).
- WiFi association — OTA pushes over the dashboard's hotspot.

## 8. Credentials

None — image signature verification (Phase 9-secure in r2-workshop) is a separate concern; v0.1 verifies SHA-256 only (per R2-DEPLOY).

## 9. Known Limitations

- **Source not yet extracted** — reference at `r2-workshop/firmware/esp32-s3/{devkitc,xiao}/src/sender.rs` (OTA acceptor lives in the sender's RX loop) and the dedicated module in `esp32-c6/dfr1117/`.
- **No image signing in v0.1** — Phase 9-secure adds TG-signed image headers; until then, OTA push trust comes from network ACL.
- **Rollback only protects from boot failure, not protocol failure** — a new image that boots but produces wrong wire frames isn't auto-reverted; operator does it.
- **`no_std = false`** — `esp_idf_svc::ota` requires `std`. The carrier-target lookup in `[modes.aot]` allows `no_std = false` per R2-PLUGIN §12.3 schema.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft. |
