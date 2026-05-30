# ble-l2cap

**Version:** 0.1.0 · **Modes:** `aot` · **Conformance:** R2-PLUGIN §12 + R2-BLE + R2-BOOTSTRAP

## 1. Purpose

L2CAP Connection-Oriented Channel (CoC) server on **PSM 0x00D2**. The bootstrap-handshake endpoint: receives the controller's `#wifi_offer` (TG-signed WiFi creds + DeviceCertificate) when a sensor needs provisioning. Per R2-BLE + R2-BOOTSTRAP.

The frame arrives as: `[FrameHeader byte | R2-WIRE compact frame ...]`. The plugin strips the 2-byte L2CAP length prefix; the consuming sentant decodes the FrameHeader + compact frame per R2-WIRE.

## 2. Modes & Platforms

`aot` esp32-s3 + esp32-c6. Requires NimBLE + L2CAP CoC enabled in sdkconfig (`CONFIG_BT_NIMBLE_L2CAP_COC_MAX_NUM=3`).

## 3. Events Handled

| Event | Parameters |
|---|---|
| `r2.bootstrap.l2cap.listen` | `{}` |
| `r2.bootstrap.l2cap.stop` | `{}` |

## 4. Events Emitted

| Status | Data | Notes |
|---|---|---|
| `"ok"` (init/listen) | `{ psm: 0x00D2 }` | |
| `"ok"` (frame_received) | `{ raw: [u8] }` | The plugin delivers each received L2CAP frame here; consuming sentant decodes |
| `"ok"` (peer_connected/disconnected) | `{ peer_addr }` | |
| `"error"` | `error: "psm_busy" / "peer_disconnected_mid_frame"` | |

## 5. Configuration

```yaml
data:
  psm: 0x00D2
  max_concurrent_peers: 1   # one bootstrap session at a time
```

## 6. Example Sentants

[`Bootstrap`](../../../sentants/Bootstrap/) subscribes `comms/ble-l2cap` (frame events), decodes the FrameHeader + compact frame, validates the TG signature, and persists the WiFi creds + DeviceCertificate to NVS.

## 7. Hardware / Host Requirements

- ESP32 BLE radio + NimBLE host.
- `CONFIG_BT_NIMBLE_L2CAP_COC_MAX_NUM ≥ 1` in sdkconfig (all three carriers set this to 3).
- 247-byte MTU minimum to accommodate `#wifi_offer` payloads (`CONFIG_BT_NIMBLE_ATT_PREFERRED_MTU=251` in r2-workshop's sdkconfig — covers it).

## 8. Credentials

None at the plugin level. The `#wifi_offer` payload itself is TG-signed; signature verification happens in the Bootstrap sentant using the baked-in TG public key.

## 9. Known Limitations

- **Source not yet extracted** — upstream `r2-core/crates/r2-esp::l2cap`. Vendoring r2-esp pending.
- **Single concurrent peer** — multiple controllers attempting bootstrap simultaneously: first wins, others get `peer_disconnected_mid_frame`.
- **Frame framing quirk** — r2-workshop's AI-CONTEXT.md notes the r2-bootstrap controller prepends a single R2-WIRE FrameHeader byte BEFORE the compact frame, and r2-esp::l2cap strips the 2-byte length prefix but NOT the FrameHeader byte. The Bootstrap sentant must `r2_wire::FrameHeader::decode(data[0])` + decode `&data[1..]`. Phase 1.4-source must preserve this.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft. |
