# Beacon sentant

**Class:** `ai.reality2.workshop.sensor.beacon` · **Storage:** `ephemeral` · **Compilable:** ✅

Substrate sentant — runs on every rocker-sensor device.

Subscribes the `identity` event from Identity. Once known, calls the **core** `ble-beacon` plugin (`crates/r2-plugin-comms-ble-beacon`) to start advertising the 28-byte R2-BEACON AD: class hash + RBID + state.

## FSM

`start → advertising → stopped` (rare; usually advertises for the device's lifetime).

## Plugins used

`ble-beacon` (core, lives in `crates/r2-plugin-comms-ble-beacon` per Roy's classification 2026-05-31).

## Events

| Direction | Event |
|---|---|
| inbound | `identity` (from Identity sentant — pulled via `get_identity`), `power_state_changed` (from Battery), `stop` |
| outbound | none (plugin calls only) |

## Reference

`r2-workshop/firmware/esp32-{s3,c6}/<carrier>/src/sender.rs` (beacon start integrated with the sender's init).

## Authoring status

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
