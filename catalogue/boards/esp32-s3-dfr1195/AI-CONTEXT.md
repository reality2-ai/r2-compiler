# AI-CONTEXT — esp32-s3-dfr1195

Composer-facing notes for reasoning about this carrier. Authoritative data is
in [`board.toml`](board.toml); this captures the *judgement* around it.

## What this board is for

The Phase-3 transient-networking **hardware test node** (PHASE3 plan Part D).
A suite of 4× DFR1195 + the laptop exercises **BLE + WiFi + LoRa** for real.
It is the first carrier with three radios, so it is where below-TG anonymous
relay (L1–L4) and beyond-radio store-carry-forward (L2) become *physically*
testable (carry a unit out of LoRa range, power one down).

## Decisions / things not derivable from the toml

- **Tri-radio, two SPI buses.** WiFi+BLE are the ESP32-S3 internal radio; LoRa
  is an external SX1262 on its own SPI bus, distinct from the TFT's SPI bus.
  Plugins that bind `r2.hw.lora` and `r2.hw.display` must each get their own
  bus + CS — they do not share.
- **Display is ST7735 colour, not SSD1306 OLED.** Earlier Part-D planning
  assumed a 0.96″ SSD1306 (I²C mono); the DFR1195's 0.96″ is 160×80 **colour
  TFT (ST7735-family, SPI)**. The D4c LCD plugin must target ST7735. (Controller
  chip is not named on the wiki — assume ST7735S until a physical board says
  otherwise.)
- **MI/MO silk ambiguity is unresolved on paper.** board.toml records
  MO=MOSI=GPIO6, MI=MISO=GPIO5. Confirm on hardware before first SX1262
  bring-up — this is the single most likely first-bring-up failure.
- **GPIO48 ≠ devkitc's WS2812.** Here GPIO48 gates LCD VDD via a load switch.
  Do not carry over devkitc/xiao LED-on-48 assumptions.
- **4 MB flash is the binding constraint.** Two OTA slots + NVS + phy_init in
  4 MB leaves ≲1.5 MB per app image. The AOT-size discipline ([[feedback-aot-optimisation-constraint]])
  is tighter here than on the 8 MB devkitc.

## Plugin/firmware coordination (Path B, no_std)

- Firmware is **hive-owned, pure no_std (esp-hal/embassy)** — one hive codebase,
  thin platform layer. composer supplies the carrier profile + OTA push + the
  catalogue plugins; hive links them.
- The D4d **LoRa plugin** trait must be a **sync, embedded-hal-compatible** SPI
  surface matching core's R2-TRANSPORT D3b SX1262 binding (not the async host
  r2-discovery bindings). Propose the trait to the supervisor/core before
  authoring.
- The **OTA receiver** is a no_std embassy-net listener in firmware; composer's
  F5 push wire is unchanged — coordinate the wire contract with hive.

## Open confirmations (need a physical board / vendor doc)

1. SX1262 MOSI/MISO (MI/MO) silk → signal mapping.
2. TFT controller chip (ST7735S vs ST7735R vs GC9xxx) + colour order/offset.
3. Whether GPIO0/Key2 is debounced in hardware or needs firmware debounce.
4. Exact USB enumeration (native S-JTAG assumed; confirm no CH340/CP2102 bridge).
