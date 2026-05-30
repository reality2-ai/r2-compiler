# AI-CONTEXT.md — storage/nvs

## Purpose

ESP-IDF NVS key/value wrapper. The workhorse persistence layer — every sentant that needs cross-reboot state writes here. Identity, WifiProv, Sync, Recorder, Bootstrap all consume.

## Modes

`aot` for esp32-s3 + esp32-c6 (NVS API is identical across the ESP32 family).

## Why important

`Identity` is the FIRST sentant to use this — without NVS the device can't persist its Ed25519 keypair, so cold boots re-generate identities and the dashboard can't recognise a known device. Authoring NVS metadata is the path to making the Identity sentant truly usable.

## Reference

`r2-workshop/firmware/esp32-c6/dfr1117/src/identity.rs` (densest NVS usage) — also a thin wrapper exists in `r2-workshop/firmware/esp32-s3/devkitc/src/identity.rs`.

## Read in order

1. plugin.toml · 2. PLUGIN.md · 3. reference identity.rs · 4. ESP-IDF NVS API docs

## Authoring status

- ✅ plugin.toml · ✅ PLUGIN.md · ✅ AI-CONTEXT.md · ⏳ Cargo.toml + src/ (Phase 1.4-source)
