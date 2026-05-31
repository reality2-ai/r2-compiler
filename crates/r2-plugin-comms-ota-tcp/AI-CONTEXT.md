# AI-CONTEXT.md — comms/ota-tcp

## Purpose

TCP OTA receiver (port 21043). COMPULSORY per SPEC-R2-COMPOSER §12.1 — without it the device is unmanageable after first install.

## Conformance

R2-PLUGIN §12 + R2-DEPLOY (R2-PLUGIN §9.1's "first plugin" pattern).

## Compulsory status

Every MCU board.toml declares `compulsory_plugins.capabilities = ["ai.reality2.deploy.ota"]` with `prefer = ["ota-tcp"]`. The compiler plugin refuses to build a board without this plugin in scope.

## Reference

`r2-workshop/firmware/esp32-s3/devkitc/src/sender.rs` (OTA acceptor inline in the sender RX loop). Phase 1.4-source should refactor into a clean ota.rs.

## Read in order

1. plugin.toml · 2. PLUGIN.md · 3. SPEC-R2-COMPOSER §12.1 · 4. R2-DEPLOY §4.7 (upstream) · 5. reference sender.rs

## Authoring status

- ✅ plugin.toml · ✅ PLUGIN.md · ✅ AI-CONTEXT.md · ⏳ Cargo.toml + src/
