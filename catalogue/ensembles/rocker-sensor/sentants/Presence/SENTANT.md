# Presence sentant

**Class:** `ai.reality2.workshop.sensor.presence` · **Storage:** `ephemeral`

One-shot UDP broadcast 5×200 ms = 1 s after WiFi association. Announces `(rbid, ip)` to the controller for fast bootstrap reconciliation post-reboot.

## Events

| Direction | Event |
|---|---|
| inbound | `associated`, `tick`, `done` |

## Platform-extension

`{{platform.udp_broadcast(addr, rbid, ip)}}` — sends one UDP packet.

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
