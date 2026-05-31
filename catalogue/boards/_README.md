# catalogue/boards/

One directory per carrier board. The canvas's first opt-in part type (the second being ensembles). Each entry is a self-contained shell holding `board.toml` + templates for the per-carrier firmware crate + datasheets + a narrative + the authoring conversation transcripts.

A carrier board IS a plugin per R2-PLUGIN §1 (it provides hardware capabilities to the hive), but it occupies its own category in the UI because it's the substrate everything else runs on. Hive-shared singleton plugins specific to the carrier (BLE radio, WiFi radio, R2-WEB) live under this entry's `plugins/` subdirectory; ensemble-owned plugins live inside the relevant ensemble.

## Layout (one example)

```
catalogue/boards/esp32-c6-dfr1117/
  board.toml                  # target triple, chip, GPIO map, sdkconfig profile
  BOARD.md                    # narrative
  AI-CONTEXT.md               # fresh-CC brief for this carrier
  pinout.svg                  # (Phase 4)
  plugins/                    # OPTIONAL — hive-shared singletons specific to this carrier
    <category>/<name>/        # e.g. comms/ble-radio, comms/wifi-radio
  templates/                  # Cargo.toml.tera, sdkconfig.defaults, partitions.csv, .cargo/config.toml
  datasheets/                 # vendor PDFs
  conversation/               # YYYY-MM-DD-<topic>-NN.md
```

Directory name follows the pattern `<arch>-<chip>-<carrier>`, kebab-case, where:

| Segment | Value |
|---|---|
| `<arch>` | R2-COMPILE §4 platform tag (`esp32`, `nrf`, `rp2`, `avr`, `linux-embedded`) |
| `<chip>` | chip family slug (`s3`, `c6`, …) — matches the espup / Cargo `--target` chip family |
| `<carrier>` | human-readable board model (`devkitc`, `xiao`, `dfr1117`, …) |

## Normative spec

See [`../../specifications/SPEC-CATALOGUE-LAYOUT.md`](../../specifications/SPEC-CATALOGUE-LAYOUT.md) §3 for the directory layout, `board.toml` schema, validation rules, and the `AI-CONTEXT.md` template.

## Adding a board

Through the visual UI: open r2-composer, hit **+ New Board** in the catalogue browser, describe the board to the agent. The agent will ask clarifying questions, fetch the vendor schematic + chip datasheet, write `board.toml`, populate `templates/`, and leave the entry in a state where the next CC session can pick it up.

Direct authoring (CLI fallback) — `tools/new-entry.sh board <slug>` will scaffold the empty directory; you fill it in by hand against SPEC-CATALOGUE-LAYOUT §3.

## v0.1 target boards

| Slug | Source | Status |
|---|---|---|
| `esp32-s3-devkitc` | `r2-workshop/firmware/esp32-s3/devkitc/` | placeholder dir; needs sync |
| `esp32-s3-xiao` | `r2-workshop/firmware/esp32-s3/xiao/` | placeholder dir; needs sync |
| `esp32-c6-dfr1117` | `r2-workshop/firmware/esp32-c6/dfr1117/` | placeholder dir; needs sync |
