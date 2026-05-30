# nvs

**Version:** 0.1.0 (metadata draft) · **Modes:** `aot` (esp32-s3, esp32-c6) · **Conformance:** R2-PLUGIN §12

## 1. Purpose

Wraps ESP-IDF's NVS (Non-Volatile Storage) key/value API. The single most-reused storage primitive in the r2-workshop firmware — every sentant that needs persistence across reboot pokes this plugin:

| Consumer | Key |
|---|---|
| `Identity` | `device_keypair` (32 + 32 B) |
| `Identity` | `rbid` (u32) |
| `WifiProv` | `wifi_ssid`, `wifi_psk` |
| `Sync` | `clock_offset_ms` (i64) |
| `Recorder` | `last_acked_seq` (u32) |
| `Bootstrap` | `tg_cert` (DeviceCertificate bytes) |

## 2. Modes & Platforms

| Mode | Targets |
|---|---|
| `aot` | `esp32-s3`, `esp32-c6` (ESP-IDF NVS API is the same across ESP32 family) |

## 3. Events Handled

| Event | Parameters | Purpose |
|---|---|---|
| `r2.hw.nvs.init` | `{ namespace }` | Open the NVS namespace |
| `r2.hw.nvs.read` | `{ key }` | Get value bytes |
| `r2.hw.nvs.write` | `{ key, value }` | Set value bytes (does NOT auto-commit) |
| `r2.hw.nvs.erase` | `{ key }` | Remove key |
| `r2.hw.nvs.list` | `{ prefix? }` | Enumerate keys |
| `r2.hw.nvs.commit` | `{}` | Flush pending writes |

## 4. Events Emitted

| Status | Data |
|---|---|
| `"ok"` (read) | `{ key, value: [u8] }` (or `data: null` if key absent) |
| `"ok"` (write/erase/commit) | `{ key? }` |
| `"ok"` (list) | `{ keys: [str] }` |
| `"error"` | `error: "not_found" / "no_space" / "invalid_handle" / ...` |

## 5. Configuration

```yaml
data:
  namespace: "r2-workshop"   # ESP-IDF NVS namespace
```

## 6. Example Sentants

The [`Identity`](../../../sentants/Identity/) sentant's worked example (already authored at metadata + has source reference at `r2-workshop/firmware/esp32-c6/dfr1117/src/identity.rs`) drives this plugin during boot.

## 7. Hardware / Host Requirements

- ESP32 family chip with `nvs` partition entry in `partitions.csv` (24 KB at offset 0x9000 in all three carriers' partition tables).
- ESP-IDF NVS C library (built by `esp-idf-svc`).
- Per `partitions.csv` examination: `nvs, data, nvs, 0x9000, 0x6000` on all three carriers.

## 8. Credentials

None. NVS itself holds credentials (WiFi password, TG-issued certs); this plugin exposes the read/write surface — credential SCOPE policy lives in the consuming sentant.

## 9. Known Limitations

- **Source not yet extracted** — wraps `esp-idf-svc::nvs::EspDefaultNvs`. Reference at `r2-workshop/firmware/esp32-{s3,c6}/<carrier>/src/identity.rs` (where NVS use is densest).
- **Single namespace per plugin instance** — namespace fixed at init.
- **No transactions** — explicit `commit` ends the write batch; failure mid-batch leaves partial state.
- **24 KB limit** is shared across all keys; if more storage is needed it has to go to SD.
- **No encryption** — NVS_FLASH_ENC requires the ESP32-S3's encryption key burned; not configured in r2-workshop's `sdkconfig.defaults`. Secrets stored here are protected by physical security only.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft. |
