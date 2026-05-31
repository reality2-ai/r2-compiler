# Espressif ESP32-S3-DevKitC-1

The current default Xtensa carrier in r2-composer's catalogue (per `r2-workshop`'s ADR-002). High-GPIO-count Espressif reference board for the ESP32-S3 family.

## At a glance

| | |
|---|---|
| **Vendor** | Espressif Systems |
| **SKU (reference)** | ESP32-S3-DevKitC-1-N8R8 (8 MB flash + 8 MB octal PSRAM). Also: N32R16V (32 MB flash + 16 MB octal PSRAM). |
| **SoC** | ESP32-S3-WROOM-1 module (ESP32-S3R8 die) |
| **Architecture** | Xtensa LX7 dual-core (240 MHz) |
| **Flash / PSRAM** | 8 MB / 8 MB octal (N8R8) |
| **USB** | Two ports: native USB-OTG (USB-Serial-JTAG) AND CP2102 UART |
| **On-board RGB LED** | WS2812 — **GPIO38 on v1.1 (current production)**, GPIO48 on v1.0 |
| **GPIO count** | 45 broken-out (J1 + J3 headers) — highest of r2-workshop's carriers |
| **Vendor docs** | https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32s3/esp32-s3-devkitc-1/ |
| **Board entry created** | 2026-05-31 |

## Role in r2-composer

The DevKitC-1 is **r2-workshop's current default carrier per ADR-002**. It was the original choice (Phase 0), briefly displaced by the XIAO under ADR-001 during a parts-availability window, and reverted to default once the buck-boost regulator and SD breakout for the XIAO build had arrived (ADR-002). It remains the high-GPIO-headroom option whenever pin count matters more than form factor.

For r2-composer's v0.1 success gate, this is the carrier most r2-workshop testing and OTA cycles have actually run on — the densest body of behavioural evidence. If the orchestrator's first end-to-end build doesn't reproduce this carrier exactly, that's the strongest signal of a regression vs the working baseline.

## Where to wire what (full table in [`board.toml`](board.toml) `[pinout]`)

| Function | GPIO | Header pin |
|---|---|---|
| Status LED (on-board WS2812) | 38 (v1.1) / 48 (v1.0) | n/a — on-board |
| Battery ADC (ADC1_CH3) | 4 | J1.4 |
| ADXL355 CS | 10 | J1.16 |
| ADXL355 MOSI / SPI shared | 11 | J1.17 |
| ADXL355 SCLK / SPI shared | 12 | J1.18 |
| ADXL355 MISO / SPI shared | 13 | J1.19 |
| ADXL355 DRDY | 14 | J1.20 |
| SD CS | 9 | J1.15 |
| SD CD (optional) | 15 | J1.8 |

Full narrative (BoM, three-phase build, voltage divider math, pre-power-up checklist) is at [`datasheets/HARDWARE-WIRING-DEVKITC.md`](datasheets/HARDWARE-WIRING-DEVKITC.md).

## Build & flash

```bash
cd out/esp32-s3-devkitc-<timestamp>/
cargo build --release           # cross-compiles to xtensa-esp32s3-espidf
esptool --chip esp32s3 --port /dev/ttyACMx write_flash \
  0x0     build/bootloader/bootloader.bin \
  0x8000  build/partition_table/partition-table.bin \
  0x10000 target/xtensa-esp32s3-espidf/release/r2-workshop-firmware.bin
```

After the first USB flash, subsequent updates can go over WiFi via OTA.

## Templates

`templates/` holds the per-build seed files copied by `tools/sync-catalogue.sh` from `r2-workshop/firmware/esp32-s3/devkitc/`:

| File | Purpose |
|---|---|
| `templates/Cargo.toml.tera` | crate manifest — esp-idf-svc + r2-esp + r2-core + r2-wire + ws2812-esp32-rmt-driver. Path deps need re-anchoring at render time. |
| `templates/.cargo/config.toml` | target = xtensa-esp32s3-espidf, ldproxy linker, espflash runner, MCU=esp32s3 |
| `templates/sdkconfig.defaults` | ESP-IDF tuning: 8 MB flash, octal PSRAM enabled, NimBLE host, FATFS LFN, USB-Serial-JTAG console |
| `templates/partitions.csv` | two-OTA-slot layout: ota_0 = ota_1 = 3 MB; storage = 1.875 MB FAT (room for cached metadata / debug logs) |
| `templates/build.rs` | walks up to find `esp-idf-sys-*/out/` and copies `partitions.csv` into the CMake build dir |
| `templates/rust-toolchain.toml` | pins the esp toolchain |
| `templates/wifi_config.toml.example` | dev fallback for WiFi credentials |

## Known gotchas

See [`board.toml`](board.toml) `[notes].gotchas`. The big ones:

- **Two USB ports** — flash via either. CP2102 side is the older convention; native USB-Serial-JTAG (USB-OTG port) is now preferred (one cable for flash + monitor).
- **WS2812 GPIO depends on board revision** — v1.1 (current production) uses GPIO38; v1.0 used GPIO48. Verify against the silkscreen near the LED.
- **PSRAM claims GPIO35/36/37** on R8/R16V variants — these pins are NOT available even though they appear on the J3 header.
- **Octal PSRAM at 80 MHz** is enabled in `sdkconfig.defaults` — if you ever use a no-PSRAM module, comment out the `CONFIG_SPIRAM*` lines.
- Use `esptool`, not `espflash` (R2-BUILD §5.1).

## Authoring history

The original DevKitC-1 firmware crate at `r2-workshop/firmware/esp32-s3/devkitc/` was r2-workshop's first carrier — months of session work captured in `r2-workshop/conversation/`.

This `catalogue/boards/esp32-s3-devkitc/` entry was scaffolded in r2-composer design session 01 (2026-05-31), board.toml + BOARD.md authored as part of Phase 1.3 (the same pattern exercised first on the dfr1117 entry).

## See also

- [`board.toml`](board.toml) — the structured contract
- [`AI-CONTEXT.md`](AI-CONTEXT.md) — fresh-CC brief
- [`datasheets/HARDWARE-WIRING-DEVKITC.md`](datasheets/HARDWARE-WIRING-DEVKITC.md) — full wiring narrative
- `../../../specifications/SPEC-CATALOGUE-LAYOUT.md` §3 — board entry schema
- `../esp32-s3-xiao/` — sibling Xtensa carrier (compact form factor, fewer GPIOs, mono LED instead of on-board WS2812)
- `../esp32-c6-dfr1117/` — sibling RISC-V carrier (different ISA, different sensor by capability swap)
