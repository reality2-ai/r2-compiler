# AI-CONTEXT.md — indicator/led

Status LED abstraction. Two backends:
- **WS2812 (RMT)** — `role_hint = "status-led-ws2812"`, devkitc only.
- **Mono LEDC PWM** — `role_hint = "status-led"`, xiao + dfr1117.

The compiler plugin picks the backend per board.toml pinout at AOT time.

## Category

`indicator/` is a new R2-PLUGIN §12.2 category claim — see [[feedback-plugin-category-claims]] in memory. Don't conflate with `display/` (screens).

## Reference

`r2-workshop/firmware/esp32-s3/devkitc/src/led.rs` (WS2812 + smart-leds) and the parallel mono variant on dfr1117. xiao currently uses external WS2812 but moving to GPIO21 LEDC (per [[project-xiao-led-choice]]) — Phase 1.4-source author must follow board.toml, not the synced template.

## Read in order

1. plugin.toml · 2. PLUGIN.md · 3. reference led.rs files · 4. `[[project-xiao-led-choice]]` in memory

## Authoring status

- ✅ plugin.toml · ✅ PLUGIN.md · ✅ AI-CONTEXT.md · ⏳ Cargo.toml + src/
