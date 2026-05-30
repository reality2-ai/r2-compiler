# 2026-05-31 — sd-card metadata authored

Phase 1.4-metadata-rest. Two-function plugin (acceleration ring + FATFS files) merged into one entry per r2-workshop's r2-workshop/firmware/esp32-s3/devkitc/src/{ring,sd}.rs layout — they share the SPI bus + the FATFS mount, so coupling is high enough to justify one plugin crate.
