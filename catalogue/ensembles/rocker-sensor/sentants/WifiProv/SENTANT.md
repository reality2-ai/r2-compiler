# WifiProv sentant

**Class:** `ai.reality2.workshop.sensor.wifi-prov` · **Storage:** `durable`

One-shot at boot. Reads (ssid, psk) from NVS — set by [`Bootstrap`](../Bootstrap/) on a successful `#wifi_offer`. Calls `platform.wifi_associate(...)`; on failure or no-creds, yields to Bootstrap and sets the LED to Advertising.

## FSM

`start → reading_nvs → associating → done (associated)` OR `→ yielded_to_bootstrap`.

## Platform-extension

`{{platform.wifi_associate(ssid, psk, timeout_ms)}}` — the compiler plugin maps this to the carrier's `esp_wifi_*` call sequence. Returns boolean for the `test` action.

## Plugins used

- `nvs` (reads ssid + psk)
- `led` (sets state for visual feedback)

## Events

| Direction | Event |
|---|---|
| inbound | `init`, `nvs` (×2: ssid then psk), `read_psk` (self), `no_creds` (self), `*→no_creds` (global) |
| outbound | `associated` (public) — Beacon + Uplink subscribe |

## Reference

`r2-workshop/firmware/esp32-{s3,c6}/<carrier>/src/sender.rs` (WiFi STA init inline at boot).

## Authoring status

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
