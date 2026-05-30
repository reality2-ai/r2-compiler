# AI-CONTEXT.md — comms/log-tcp

TCP fan-out of `log::*!` lines on port 21046. Read-only; multiple dashboard clients tail concurrently. SPEC-R2-WORKSHOP-SENSOR-LIVE-LOGS in r2-workshop is the contract.

## Modes

`aot` esp32-s3 + esp32-c6.

## Read in order

1. plugin.toml · 2. PLUGIN.md · 3. SPEC-R2-WORKSHOP-SENSOR-LIVE-LOGS (r2-workshop)

## Authoring status

- ✅ plugin.toml · ✅ PLUGIN.md · ✅ AI-CONTEXT.md · ⏳ Cargo.toml + src/
