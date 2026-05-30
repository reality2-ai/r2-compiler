# AI-CONTEXT.md — comms/reset-tcp

TCP listener (port 21044) accepting a single 0x52 byte and calling `esp_restart`. Implements SPEC-R2-WORKSHOP-SENSOR-REMOTE-RESET. Used by [`Reset`](../../../sentants/Reset/).

## Modes

`aot` for esp32-s3 + esp32-c6.

## Read in order

1. plugin.toml · 2. PLUGIN.md · 3. SPEC-R2-WORKSHOP-SENSOR-REMOTE-RESET (in r2-workshop) · 4. R2-PLUGIN §12

## Authoring status

- ✅ plugin.toml · ✅ PLUGIN.md · ✅ AI-CONTEXT.md · ⏳ Cargo.toml + src/
