# Recorder sentant

**Class:** `ai.reality2.workshop.sensor.recorder` · **Storage:** `durable-state`

Writes every `r2.sensor.acceleration` to the SD card's 20-byte fixed-record ring buffer. On cumulative-ack from the dashboard (`r2.dash.ack { through_seq }`), pops up to that seq + persists the new `last_acked_seq` to NVS so a cold boot resumes from the right place.

## Plugins

`sd-card`, `nvs`.

## Events

| Direction | Event |
|---|---|
| inbound | `init`, `nvs`, `r2.sensor.acceleration` (every sample), `r2.dash.ack` |
| outbound | none directly (plugin calls only) |

## AOT note

The acceleration event arrives at ~100 Hz × 20 bytes = 2 KB/s — small for SD write throughput. The sd-card plugin batches internally (per its driver), so the hot path is `ring.push` per sample with the underlying FATFS write coalesced.

## Reference

`r2-workshop/firmware/esp32-s3/<carrier>/src/ring.rs` + `sender.rs`'s consumer-of-ring path.

✅ sentant.yaml · ✅ SENTANT.md · ✅ AI-CONTEXT.md · ✅ conversation
