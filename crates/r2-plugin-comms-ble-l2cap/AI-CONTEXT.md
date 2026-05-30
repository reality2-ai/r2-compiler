# AI-CONTEXT.md — comms/ble-l2cap

L2CAP CoC server on PSM 0x00D2 — the bootstrap handshake endpoint. NimBLE on ESP32-S3/C6. Consumed by `Bootstrap`.

## Frame-framing gotcha (must preserve in Phase 1.4-source)

r2-bootstrap (controller side) prepends ONE R2-WIRE FrameHeader byte before the compact frame. r2-esp::l2cap strips the L2CAP 2-byte length prefix but NOT the FrameHeader byte. Bootstrap sentant decodes `data[0]` as FrameHeader, then decodes `&data[1..]` as compact-frame. Symptom of getting this wrong: event_hash off by one byte (e.g. `0x0d01f776` instead of `0x01f77656`).

See r2-workshop AI-CONTEXT.md "L2CAP `#wifi_offer` framing".

## Modes

`aot` esp32-s3 + esp32-c6.

## Read in order

1. plugin.toml · 2. PLUGIN.md · 3. R2-BLE + R2-BOOTSTRAP specs (upstream) · 4. r2-core's `crates/r2-esp/src/l2cap.rs`

## Authoring status

- ✅ plugin.toml · ✅ PLUGIN.md · ✅ AI-CONTEXT.md · ⏳ Cargo.toml + src/ (likely wraps r2-esp::l2cap)
