# Uplink sentant

**Class:** `ai.reality2.workshop.sensor.uplink` · **Storage:** `ephemeral`

The data plane. Maintains a single TCP session to the controller (`192.168.4.1:21042`), sends the announce frame on connect (signed with device_sk), then forwards every `r2.sensor.acceleration` over R2-WIRE.

## FSM

`start → connecting → streaming → connecting (link_lost)`.

## Platform-extensions

- `{{platform.tcp_connect(ip, port, timeout_ms)}}` → returns bool
- `{{platform.tcp_send_compact_frame(params)}}` → emits a compact frame; returns bool

The compiler plugin synthesises these from the carrier's networking stack.

## Plugins used

`led` only (for state indication). TCP I/O is platform-synthesised.

## Events

| Direction | Event |
|---|---|
| inbound | `associated` (from WifiProv), `r2.sensor.acceleration` (from Accelerometer), `link_lost` (self), `reconnect_after` (self with delay) |
| outbound | `r2.sensor.announce` (public, sent on the TCP socket at connect) |

## Reference

`r2-workshop/firmware/esp32-{s3,c6}/<carrier>/src/sender.rs` — the TCP send loop. The most complex hand-coded file in r2-workshop's firmware.

## Authoring status

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
