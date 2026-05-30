# AI-CONTEXT.md — sentants/Uplink

The data plane. Forwards `r2.sensor.acceleration` over TCP R2-WIRE to the controller. Single session; exponential reconnect on loss.

## Platform-extensions

- `{{platform.tcp_connect(...)}}` — calls esp_tcp / std::net::TcpStream::connect
- `{{platform.tcp_send_compact_frame(params)}}` — serialises params to R2-WIRE compact frame using `r2-cbor` + `r2-wire`, writes to the TCP stream

## Read in order

1. sentant.yaml · 2. SENTANT.md · 3. r2-workshop sender.rs (the canonical reference) · 4. R2-WIRE spec (upstream)

## Authoring status

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
