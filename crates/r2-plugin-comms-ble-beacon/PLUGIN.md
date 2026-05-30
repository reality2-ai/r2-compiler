# ble-beacon

**Version:** 0.1.0 · **Modes:** `aot` (esp32-s3, esp32-c6) · **Conformance:** R2-PLUGIN §12 + R2-BEACON

## 1. Purpose

R2-BEACON legacy-mode BLE advertiser. Encodes the 28-byte advertising-data payload per R2-BEACON spec:

- Class hash (FNV-1a-32 of the ensemble class string — e.g. `nz.ac.auckland.rocker` → `0x624c47bc`)
- RBID (FNV-1a-32 of `device_pk`)
- Power state byte
- Provisioning flag

Every sensor advertises continuously while running; the controller scans for matching class hash and initiates `#wifi_offer` over L2CAP (via [`ble-l2cap`](../ble-l2cap/)).

## 2. Modes & Platforms

`aot` for esp32-s3 + esp32-c6. NimBLE host stack via `esp_idf` — requires `CONFIG_BT_NIMBLE_ENABLED=y` (set on all three carriers in `templates/sdkconfig.defaults`).

## 3. Events Handled

| Event | Parameters | Purpose |
|---|---|---|
| `r2.beacon.start` | `{}` | Begin advertising |
| `r2.beacon.stop` | `{}` | Halt |
| `r2.beacon.update` | `{ power_state?, provisioned? }` | Refresh AD payload — only fields that changed |

## 4. Events Emitted

| Status | Data |
|---|---|
| `"ok"` (init/start/stop/update) | `{}` |
| `"error"` | `error: "ble_init" / "advertising_busy"` |

## 5. Configuration

```yaml
data:
  class: "nz.ac.auckland.rocker"   # baked at compile time; runtime cannot rewrite
  interval_ms: 1000
  tx_power_dbm: 0
```

## 6. Example Sentants

[`Beacon`](../../../sentants/Beacon/) wraps this plugin — calls `r2.beacon.start` after `Identity` reaches ready and keeps the advertiser running.

## 7. Hardware / Host Requirements

- ESP32-family chip with BLE radio.
- NimBLE host stack enabled in sdkconfig.
- Coexistence with WiFi — both share the 2.4 GHz radio time-multiplexed; r2-workshop's BLE-coex settings are in sdkconfig.

## 8. Credentials

None. Class hash IS the identity — class membership gates which controllers respond.

## 9. Known Limitations

- **Source not yet extracted** — `r2-workshop` uses `crates/r2-esp::beacon` upstream from r2-core; vendoring strategy for r2-esp is open (r2-esp is xtensa-only and Roy may want a per-target r2-esp).
- **Class hash is compile-time only** — operators can't rotate the class without a re-flash. Per SPEC-R2-WORKSHOP-ENSEMBLE §2.3 "class rotation" procedure.
- **No extended-AD support** — 31-byte legacy AD only.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft. |
