# Bootstrap sentant

**Class:** `ai.reality2.workshop.sensor.bootstrap` · **Storage:** `ephemeral`

The BLE L2CAP `#wifi_offer` receiver. Runs while the sensor is unprovisioned (no WiFi creds in NVS) — listens on PSM 0x00D2, validates the TG-signed offer, persists creds + DeviceCertificate, reboots.

## FSM

`start → listening → done` (no recovery — failed validation just stays in `listening` for the next attempt).

## The framing gotcha

`r2-bootstrap` (controller side) prepends ONE R2-WIRE FrameHeader byte before the compact frame. `ble-l2cap` plugin strips the L2CAP length prefix but NOT the FrameHeader byte. This sentant's `decode_wifi_offer` transition handles that — `data[0]` is FrameHeader, `data[1..]` is the compact frame. **Phase 1.4-source must preserve this** — see `[[project-tg-management-workflow]]` and the ble-l2cap PLUGIN.md §9.

## Platform-extension tokens

The yaml uses `{{platform.verify_tg_signature(...)}}` and `{{platform.parse_wifi_offer(...)}}` — extensions to R2-DEF §5.1 templates. The compiler plugin recognises these tokens and synthesises calls to the appropriate Rust functions (TG-public-key-baked-in signature check; CBOR decode of the wifi_offer payload). Same extension pattern as Identity's `{{platform.random_seed_32}}`.

## Plugins used

- `ble-l2cap` (core, `crates/r2-plugin-comms-ble-l2cap`)
- `nvs` (catalogue — pending core/opt-in classification pass)

## Events

| Direction | Event |
|---|---|
| inbound | `init`, `ble-l2cap` (plugin result — `frame_received` sub-event), `decode_wifi_offer` (self-send), `bootstrap_complete` (self-send) |
| outbound | `r2.dash.reset` (triggers Reset sentant for the clean reboot) |

## Reference

`r2-workshop/firmware/esp32-{s3,c6}/<carrier>/src/...` (bootstrap inline with sender today).

## Authoring status

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
