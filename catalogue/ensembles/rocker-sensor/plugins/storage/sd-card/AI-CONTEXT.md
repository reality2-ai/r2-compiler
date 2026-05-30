# AI-CONTEXT.md — storage/sd-card

## Purpose

FATFS + 20-byte acceleration ring buffer for store-and-forward. Used by [`Recorder`](../../../sentants/Recorder/) (ring) and [`Capture`](../../../sentants/Capture/) (named files).

## Modes

`aot` for esp32-s3 (devkitc + xiao). dfr1117 support deferred — that carrier's SD wiring exists but the driver isn't ported.

## Reference

`r2-workshop/firmware/esp32-s3/devkitc/src/{ring,sd}.rs`. Two files because the ring lives at a separate abstraction (fixed-record store) from generic FATFS file ops.

## Coupling

- Shares SPI bus with `../../sensor/adxl355/` (different CS pins).
- The 20-byte record schema is contracted with [`Recorder`](../../../sentants/Recorder/) and the wire format.

## Read in order

1. plugin.toml · 2. PLUGIN.md · 3. reference `ring.rs` + `sd.rs` · 4. SPEC-R2-WORKSHOP-CAPTURE §6 (in r2-workshop)

## Authoring status

- ✅ plugin.toml · ✅ PLUGIN.md · ✅ AI-CONTEXT.md · ⏳ Cargo.toml + src/ (Phase 1.4-source)
