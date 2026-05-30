# clock

**Version:** 0.1.0 · **Modes:** `aot` · **Category:** `time/` (new — see [[feedback-plugin-category-claims]])

## 1. Purpose

Time service for the rocker-sensor firmware. Two clocks:

- **`monotonic_ms()`** — ESP-IDF tick counter since boot. Strictly increasing, no jumps. Used for relative timestamps within a measurement session.
- **`now_ms()` = `monotonic_ms() + offset`** — wall-clock-relative timestamp. `offset` is set by the [`Sync`](../../../sentants/Sync/) sentant via Cristian's algorithm and persisted to NVS (key `clock_offset_ms`).

The plugin is small but every event-emitting sentant uses it to stamp records. Splitting it out from a generic `system` plugin keeps the contract minimal.

## 2. Modes & Platforms

`aot` esp32-s3 + esp32-c6. `no_std = true` — the implementation calls ESP-IDF's `esp_timer_get_time()` directly.

## 3. Events Handled

| Event | Parameters |
|---|---|
| `r2.clock.now` | `{}` |
| `r2.clock.monotonic` | `{}` |
| `r2.clock.set_offset` | `{ offset_ms: i64 }` (Sync sentant calls this) |
| `r2.clock.get_offset` | `{}` |

## 4. Events Emitted

| Status | Data |
|---|---|
| `"ok"` (now/monotonic) | `{ ms: u64 }` |
| `"ok"` (set_offset) | `{}` (also writes `clock_offset_ms` to NVS) |
| `"ok"` (get_offset) | `{ offset_ms: i64 }` |

## 5. Configuration

```yaml
data:
  nvs_key: "clock_offset_ms"
```

## 6. Example Sentants

[`Sync`](../../../sentants/Sync/) is the writer. Every other event-emitting sentant — Recorder, Capture, Uplink — is a reader.

## 7. Hardware / Host Requirements

- ESP-IDF `esp_timer_get_time()` (microsecond resolution; truncated to ms).
- NVS plugin in scope (for offset persistence).

## 8. Credentials

None.

## 9. Known Limitations

- **Source not yet extracted** — small module in r2-workshop, often inline.
- **Millisecond resolution** — sufficient for SHM-class signals; sub-millisecond timing not exposed.
- **No NTP / external time source** — sync comes from the dashboard's clock (which is the operator's laptop), via the Sync sentant. Wall-clock accuracy bounded by network round-trip jitter (~5 ms typical).
- **Wraparound** — `u64` ms ≈ 584 million years. Fine.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft. Claims new `time/` category — flag for upstream R2-PLUGIN §12.2 amendment. |
