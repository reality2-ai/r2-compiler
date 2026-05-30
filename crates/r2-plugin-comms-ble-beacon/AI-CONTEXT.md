# AI-CONTEXT.md — comms/ble-beacon

R2-BEACON advertiser (legacy mode, 28-byte AD: class hash + RBID + state). NimBLE on ESP32-S3 + ESP32-C6. Paired with `../ble-l2cap/` (the bootstrap PSM endpoint).

## Modes

`aot` esp32-s3 + esp32-c6.

## Reference

Upstream `r2-core/crates/r2-esp/src/beacon.rs`. Vendoring r2-esp into r2-compiler's `crates/` is pending — currently the workshop firmware path-deps it from r2-core directly.

## Read in order

1. plugin.toml · 2. PLUGIN.md · 3. R2-BEACON spec (upstream `r2-specifications/specs/r2-core/R2-BEACON.md`) · 4. r2-core's `crates/r2-esp/src/beacon.rs`

## Authoring status

- ✅ plugin.toml · ✅ PLUGIN.md · ✅ AI-CONTEXT.md · ⏳ Cargo.toml + src/ (likely wraps r2-esp::beacon)
