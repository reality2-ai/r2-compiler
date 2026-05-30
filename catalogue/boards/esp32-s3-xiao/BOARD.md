# Seeed Studio XIAO ESP32-S3 (Pre-Soldered)

The compact Xtensa carrier in r2-compiler's catalogue. Coin-sized 21 × 17.5 mm board around the ESP32-S3R8 die, with on-board LiPo charging + USB-C. Fully-supported alternative to the DevKitC; ADR-001 briefly made it default, ADR-002 reverted.

## At a glance

| | |
|---|---|
| **Vendor** | Seeed Studio |
| **SKU** | XIAO ESP32-S3 (Pre-Soldered) |
| **SoC die** | ESP32-S3R8 |
| **Architecture** | Xtensa LX7 dual-core (240 MHz) |
| **Flash / PSRAM** | 8 MB / 8 MB octal |
| **USB** | Single USB-C, native USB-Serial-JTAG |
| **Power** | USB-C; on-board buck regulator handles LiPo cell directly across full 3.0–4.2 V curve; on-board CC/CV LiPo charger |
| **Cell connection** | BAT+ / BAT− solder pads on the BACK (no JST-PH) |
| **On-board LED** | mono yellow on GPIO21 (LEDC PWM) — the FSM status indicator. No on-board addressable RGB; FSM doesn't surface colour on this carrier. |
| **GPIO count** | 11 broken-out (D0–D10) |
| **Form factor** | 21 × 17.5 mm |
| **Vendor docs** | https://wiki.seeedstudio.com/xiao_esp32s3_getting_started/ |
| **Board entry created** | 2026-05-31 |

## Role in r2-compiler

The XIAO is r2-workshop's **compact alternative** to the DevKitC. ADR-001 promoted it to default during a parts-availability window (USB-C + on-board charging + small form factor were big draws); ADR-002 reverted to DevKitC once the external buck-boost and SD breakout for the lab's existing kit had arrived. The XIAO build stays fully supported — a future student or operator who values size, USB-C convenience, or on-board LiPo handling may legitimately pick it. Same R2-WIRE traffic, same sentants, same firmware codebase as the DevKitC build.

For r2-compiler's v0.1 success gate, this carrier exercises the **constrained-GPIO** path. The XIAO has only 11 broken-out pins to the DevKitC's 45 — every pin is allocated, no headroom. The status LED is the on-board mono yellow LED on GPIO21 (LEDC PWM) — same mono-LED pattern as the dfr1117 carrier (GPIO15). No external WS2812 module is required, and the FSM does not surface colour on this carrier; states are distinguished by blink rate. If r2-compiler can produce working builds for the DevKitC (on-board WS2812) AND the XIAO + dfr1117 (mono LED) from the same score, the "carrier-as-substrate" abstraction with per-carrier LED driver selection is real.

## Where to wire what

Wire by silkscreen label (`D0`–`D10`, `3V3`, `GND`), not by GPIO number. Full table in [`board.toml`](board.toml) `[pinout]`:

| Function | XIAO silk | GPIO |
|---|---|---|
| Status LED (on-board mono yellow) | (on-board) | 21 |
| ADXL355 CS | D0 | 1 |
| ADXL355 DRDY (optional) | D1 | 2 |
| Battery ADC | D3 | 4 |
| SD CS | D4 | 5 |
| SPI SCK (shared) | D8 | 7 |
| SPI MISO (shared) | D9 | 8 |
| SPI MOSI (shared) | D10 | 9 |

D5/GPIO6 is **spare** (the prior external-WS2812 pin assignment is no longer used).

Full narrative (BoM, three-phase build, on-board-charger rationale, hot-swap notes) at [`datasheets/HARDWARE-WIRING-XIAO.md`](datasheets/HARDWARE-WIRING-XIAO.md).

## Build & flash

```bash
cd out/esp32-s3-xiao-<timestamp>/
cargo build --release           # cross-compiles to xtensa-esp32s3-espidf
esptool --chip esp32s3 --port /dev/ttyACMx write_flash \
  0x0     build/bootloader/bootloader.bin \
  0x8000  build/partition_table/partition-table.bin \
  0x10000 target/xtensa-esp32s3-espidf/release/r2-workshop-firmware.bin
```

After the first USB flash, subsequent updates can go over WiFi via OTA.

## Templates

`templates/` holds the per-build seed files synced from `r2-workshop/firmware/esp32-s3/xiao/`:

| File | Purpose |
|---|---|
| `templates/Cargo.toml.tera` | crate manifest synced from r2-workshop. ⚠ Currently declares `ws2812-esp32-rmt-driver` because r2-workshop's xiao firmware still uses the external WS2812 — the Compiler sentant MUST drop that dep and emit LEDC-PWM driver code per the GPIO21 pinout. See [`board.toml`](board.toml) `[notes].gotchas` last entry. |
| `templates/.cargo/config.toml` | target = xtensa-esp32s3-espidf, MCU=esp32s3 |
| `templates/sdkconfig.defaults` | 8 MB flash, octal PSRAM, NimBLE, FATFS LFN, USB-Serial-JTAG console |
| `templates/partitions.csv` | identical to the DevKitC layout (both carriers have 8 MB flash) — two OTA slots × 3 MB + 1.875 MB FAT storage |
| `templates/build.rs` | esp-idf-sys partitions-copy workaround |
| `templates/rust-toolchain.toml` | pins the esp toolchain |
| `templates/wifi_config.toml.example` | dev fallback for WiFi creds |

## Differences vs DevKitC (peer Xtensa carrier)

| Aspect | DevKitC-1 | XIAO ESP32-S3 |
|---|---|---|
| GPIO count | 45 | 11 |
| Status LED | on-board WS2812 (RGB, GPIO38 on v1.1) | on-board mono yellow (GPIO21, LEDC PWM) |
| Power | external buck-boost + JST-PH | on-board buck + BAT pads + on-board charger |
| USB | dual (USB-OTG + CP2102) | single (USB-C native) |
| Cell hot-swap | yes (JST-PH disconnect) | no (solder pads) |
| Form factor | ~50 × 70 mm | 21 × 17.5 mm |
| Best for | GPIO headroom, diagnosable power, colour-coded status | size, USB-C convenience, simpler hardware |

Both carriers compile against the same Rust target (`xtensa-esp32s3-espidf`), same dependencies, same ESP-IDF version, and produce functionally-equivalent firmware. Only pin literals + sdkconfig comments + carrier-specific gotchas differ.

## Known gotchas

See [`board.toml`](board.toml) `[notes].gotchas`. The XIAO-specific ones:

- **Cell solders directly to BAT+/BAT−** — there's no connector. Hot-swap during a session means de-soldering, so the workflow is "charge over USB-C while bench-debugging, run from cell in the field".
- **No over-discharge protection** on-board. Use a protected 18650 cell, or disconnect when idle.
- **FSM status LED is the on-board mono yellow on GPIO21** (LEDC PWM). The FSM does NOT surface colour on this carrier — states distinguished by blink rate, matching the dfr1117 pattern. No external WS2812 module required.
- **Template lag (2026-05-31):** synced `templates/Cargo.toml.tera` still declares the old `ws2812-esp32-rmt-driver` dep because r2-workshop's xiao firmware hasn't yet caught up to this design. The Compiler sentant must reconcile.
- **XIAO Plus is a different SKU** (16 MB flash, more GPIOs) — would be a separate board entry, sdkconfig differs.

## Authoring history

XIAO firmware crate originally landed in r2-workshop under ADR-001 — `r2-workshop/decisions/ADR-001-xiao-esp32-s3-carrier.md` documents the carrier swap rationale. ADR-002 reverted default to DevKitC; XIAO build stayed fully supported.

This `catalogue/boards/esp32-s3-xiao/` entry: scaffolded 2026-05-31 design session 01; board.toml + BOARD.md authored as part of Phase 1.3.

## See also

- [`board.toml`](board.toml) — the structured contract
- [`AI-CONTEXT.md`](AI-CONTEXT.md) — fresh-CC brief
- [`datasheets/HARDWARE-WIRING-XIAO.md`](datasheets/HARDWARE-WIRING-XIAO.md) — full wiring narrative
- `../../../specifications/SPEC-CATALOGUE-LAYOUT.md` §3 — board entry schema
- `../esp32-s3-devkitc/` — sibling Xtensa carrier (larger, more GPIOs, dual USB)
- `../esp32-c6-dfr1117/` — sibling RISC-V carrier
- r2-workshop's `decisions/ADR-001-xiao-esp32-s3-carrier.md` and `ADR-002-revert-active-default-to-devkitc.md` for the original carrier-swap rationale
