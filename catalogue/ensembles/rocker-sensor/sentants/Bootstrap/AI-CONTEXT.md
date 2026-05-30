# AI-CONTEXT.md — sentants/Bootstrap

Receives `#wifi_offer` over L2CAP, validates TG signature, persists creds to NVS, reboots. R2-BOOTSTRAP §4.

## Critical framing gotcha (Phase 1.4-source MUST preserve)

`ble-l2cap` delivers frames as `[FrameHeader byte | compact frame ...]`. The sentant decodes `data[0]` separately. Off-by-one here → event_hash wrong by exactly one byte (workshop saw `0x0d01f776` instead of `0x01f77656`).

## Platform-extension template tokens

`{{platform.verify_tg_signature(...)}}` and `{{platform.parse_wifi_offer(...)}}` are r2-compiler extensions to R2-DEF §5.1 — the compiler synthesises Rust at code-gen. Same pattern as Identity's `{{platform.random_seed_32}}`.

## Read in order

1. sentant.yaml · 2. SENTANT.md · 3. `crates/r2-plugin-comms-ble-l2cap/PLUGIN.md` (§9 gotcha) · 4. R2-BOOTSTRAP §4 (upstream)

## Authoring status

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
