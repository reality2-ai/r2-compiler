# DFRobot LoRaWAN ESP32-S3 (DFR1195)

The first **tri-radio** carrier in the catalogue — WiFi + BLE 5 (ESP32-S3)
**plus** a Semtech **SX1262 LoRa** radio — and the Phase-3 transient-networking
hardware test node. See [`board.toml`](board.toml) for the machine-readable
source of truth; this is the human reference.

## At a glance

| | |
|---|---|
| MCU module | ESP32-S3-WROOM-1-**N4** (Xtensa LX7 dual-core @ 240 MHz) |
| Flash / PSRAM | **4 MB** / none |
| Radios | WiFi 4 (2.4 GHz) · BLE 5 · **LoRa SX1262** (SPI) |
| Display | 0.96″ **160×80 colour TFT** (ST7735-family, SPI) |
| Inputs | Key1 (GPIO18) · Key2 (GPIO0 boot strap) |
| Other | LED (GPIO21) · battery ADC (GPIO1) |
| USB | USB-C, native USB-Serial-JTAG (`0x303a:0x1001`) |

## Pin map

**SX1262 LoRa (SPI bus A):** SCK `GPIO7`, MOSI `GPIO6` (silk *MO*), MISO `GPIO5`
(silk *MI*), NSS/CS `GPIO10`, RST `GPIO41`, BUSY `GPIO40`, DIO1 `GPIO4`,
RXEN `GPIO42`.
⚠️ The MI/MO silk → MOSI/MISO mapping is recorded as MO=MOSI=6, MI=MISO=5 but
**must be confirmed on a physical board** — the wiki prose was inconsistent, and
a swapped pair gives no SX1262 response with no error.

**0.96″ TFT (SPI bus B):** MOSI `GPIO11`, SCK `GPIO12`, CS `GPIO17`, RST `GPIO15`,
DC `GPIO14`, backlight `GPIO16`, **power-enable `GPIO48`** (load switch — raise
before driving the panel). It is a **colour ST7735-family TFT, not an SSD1306
OLED** — the on-device LCD plugin (Phase-3 D4c) targets ST7735.

**Buttons / LED / battery:** Key1 `GPIO18` (the Phase-3 test trigger), Key2 `GPIO0`
(also the boot strap — held low at reset = download mode), LED `GPIO21`,
battery sense `GPIO1` (ADC1_CH0).

**Reserved / don't-wire:** `GPIO0` (strap + Key2), `GPIO3/45/46` (straps),
`GPIO19/20` (USB), `GPIO48` (LCD VDD switch).

## R2 notes

- **Two separate SPI buses** — SX1262 (5/6/7) and TFT (11/12) are independent;
  don't collapse them.
- **4 MB flash** (half the devkitc) — the OTA partition table must fit two app
  slots + NVS + phy_init in 4 MB; AOT image-size discipline matters here.
- **OTA:** compulsory `ota-tcp` over WiFi STA; first-install by USB (esptool),
  then OTA. Path-B firmware uses a no_std embassy-net receiver; composer's push
  wire (F5) is unchanged.
- **Compile target** `esp32-s3`, shared with esp32-s3-devkitc / -xiao.

## References

- Wiki: <https://wiki.dfrobot.com/dfr1195/>
- Product: <https://www.dfrobot.com/product-2933.html>
- SX1262: <https://www.semtech.com/products/wireless-rf/lora-connect/sx1262>
