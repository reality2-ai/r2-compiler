# AI-CONTEXT.md — time/clock

Monotonic + offset clock; offset persisted to NVS by the Sync sentant.

## Category

`time/` is a new R2-PLUGIN §12.2 category claim — see [[feedback-plugin-category-claims]].

## Reference

Often inline in r2-workshop; conceptually `monotonic_ms()` wraps `esp_timer_get_time()` and `now_ms() = monotonic_ms() + offset`.

## Read in order

1. plugin.toml · 2. PLUGIN.md · 3. SPEC-R2-WORKSHOP-TIMESYNC §2 (in r2-workshop) · 4. ESP-IDF esp_timer API docs

## Authoring status

- ✅ plugin.toml · ✅ PLUGIN.md · ✅ AI-CONTEXT.md · ⏳ Cargo.toml + src/
