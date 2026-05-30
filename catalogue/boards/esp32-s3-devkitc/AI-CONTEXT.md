# AI-CONTEXT.md — esp32-s3-devkitc

> **Placeholder.** This entry has not been authored yet. Status: scaffolding only.
>
> When this entry is brought live (via `tools/sync-catalogue.sh` or the **+ New Board** authoring flow), this file must be rewritten to match SPEC-CATALOGUE-LAYOUT §3.3.

## Purpose

Espressif ESP32-S3-DevKitC-1 carrier — the reference development board for the ESP32-S3 chip family. Xtensa LX7 dual-core, 8 MB flash, optional PSRAM, USB-Serial-JTAG.

Currently used by `r2-workshop/firmware/esp32-s3/devkitc/` as one of three r2-workshop carriers. r2-compiler v0.1 must round-trip this carrier behaviourally (see [`../../../specifications/SPEC-R2-COMPILER.md`](../../../specifications/SPEC-R2-COMPILER.md) §6).

## Class + target (provisional — verify against board.toml when authored)

- `<arch>-<chip>-<carrier>` = `esp32-s3-devkitc`
- Target triple = `xtensa-esp32s3-espidf`
- R2-DEF §7.7 compile_target tag = `esp32-s3`

## Where the canonical artefact will live

`board.toml` (not yet written). See SPEC-CATALOGUE-LAYOUT §3.2 for the schema.

## Vendor refs (to populate)

- ESP32-S3-DevKitC-1 datasheet — pending download from https://docs.espressif.com/...
- ESP32-S3 chip datasheet — pending download
- ESP-IDF v5.x partition table reference — informative

## Authoring source

When this board is authored, the agent should consult:

- `r2-workshop/firmware/esp32-s3/devkitc/` for the working per-carrier crate (pin assignments, sdkconfig, partitions).
- `r2-workshop/specifications/HARDWARE-WIRING-DEVKITC.md` for the rocker-rig wiring (informative — most of it is rig-specific, but the chip pin functions are reusable).
- `r2-core/platforms/esp32-s3/` for the Rust platform crate.
- `r2-specifications/specs/r2-core/R2-COMPILE.md` §4 for the target's place in the compile model.

## Known gotchas (from r2-workshop)

- The first build under `esp-idf-sys` rebuilds the entire ESP-IDF SDK and takes 15–30 minutes. Subsequent incremental builds are fast.
- `esp-idf-sys + custom partition table`: ESP-IDF resolves `CONFIG_PARTITION_TABLE_CUSTOM_FILENAME` relative to `esp-idf-sys`'s auto-generated build directory, not the crate root. `build.rs` walks up to find `esp-idf-sys-*/out/` and copies `partitions.csv` there. First-build chicken-and-egg solved by `tools/setup-firmware.sh`. May need a SECOND clean rebuild if the first produces the default 1-app layout.
- `espflash v3.x` writes a header byte that breaks ESP-IDF v5.3+ bootloaders (R2-BUILD §5.1). **Use `esptool`** (Python, bundled with ESP-IDF) for flashing, not `espflash`.

## Read these files in this order (once authored)

1. `board.toml` — the contract.
2. `BOARD.md` — narrative.
3. `templates/Cargo.toml.tera` — what gets rendered into the per-build crate.
4. `templates/sdkconfig.defaults` — ESP-IDF tuning.
5. `templates/partitions.csv` — OTA layout.
6. `templates/.cargo/config.toml` — target + linker.

---

*Created 2026-05-31 as a scaffold; needs full authoring before v0.1 success gate can be exercised.*
