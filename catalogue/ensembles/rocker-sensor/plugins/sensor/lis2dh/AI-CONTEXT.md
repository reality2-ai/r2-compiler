# AI-CONTEXT.md — sensor/lis2dh

Fresh-CC brief for picking up this plugin entry cold.

## Purpose

R2 plugin wrapping the **STMicroelectronics LIS2DH** triaxial accelerometer over I²C. Provides the generic `ai.reality2.cap.accel.triaxial` capability so the rocker-sensor ensemble's `Accelerometer` sentant can read acceleration without knowing which chip is wired underneath.

Used by r2-workshop's **`esp32-c6-dfr1117` carrier** via the DFRobot SEN0224 Gravity I²C breakout. Lower-precision peer to `adxl355` (SPI, 20-bit) — same capability provided, different bus + chip, exemplar of R2-PLUGIN §10's swap lever.

## Conformance

This entry MUST conform to:

- **R2-PLUGIN §12.3** — `plugin.toml` manifest fields (all REQUIRED fields present, modes table, commands table, capabilities, events, credentials)
- **R2-PLUGIN §12.4** — implements `r2_engine::plugin::Plugin` trait (deferred — see §10 below)
- **R2-PLUGIN §12.5** — Cargo `[features]` declare `aot` / `nif` mutually-exclusive (deferred until Cargo.toml is written)
- **R2-PLUGIN §12.8** — `PLUGIN.md` has all 10 mandatory sections ✅
- **SPEC-CATALOGUE-LAYOUT §4.3** — directory layout under `catalogue/ensembles/<ensemble>/plugins/sensor/lis2dh/`

## Modes & targets

| Mode | Targets | Status |
|---|---|---|
| `aot` | `esp32-c6`, `esp32-s3` | Primary |
| `nif` | — | Not supported in v0.1 |
| `web` | — | Not applicable |

Source: `plugin.toml [modes]`.

## Hardware

- LIS2DH on the DFRobot **SEN0224** 4-pin Gravity I²C breakout
- I²C address: `0x18` (default — SA0 strapped GND on the board) or `0x19` (SA0=VDD)
- ±2/4/8/16 g programmable range; 10-bit data
- ODR 1 Hz to 5.3 kHz
- Pin assignments are CARRIER-SPECIFIC — for `esp32-c6-dfr1117`: SDA=GPIO19, SCL=GPIO20 (silk-labelled `SDA` / `SCL` on the board). Check `catalogue/boards/<carrier>/board.toml [pinout]` for other carriers when they grow LIS2DH support.

## Commands

Mapping from string → opcode per R2-PLUGIN §12.4.1:

| Command | Opcode | Input | Output (on `status: "ok"`) |
|---|---|---|---|
| `init` | 0x01 | `{ odr_hz, range_g, offset? }` | `{ who_am_i: 0x33 }` |
| `read` | 0x02 | `{}` | `{ x, y, z, ts_ms }` |
| `read_burst` | 0x03 | `{ max_samples }` | `{ samples: [...], count }` |
| `set_odr` | 0x04 | `{ odr_hz }` | `{}` |
| `set_range` | 0x05 | `{ range_g }` | `{}` |
| `set_offset` | 0x06 | `{ x, y, z }` | `{}` |
| `sleep` | 0x07 | `{}` | `{}` |

Full event payload schemas in [`PLUGIN.md`](PLUGIN.md) §3–4.

## Datasheet refs (to fetch + save under `datasheets/`)

When the authoring flow (Phase 2+) runs against this entry it should fetch:

- ST LIS2DH datasheet (PDF): from ST's product page or via direct PDF URL
- DFRobot SEN0224 wiki: https://wiki.dfrobot.com/Gravity__I2C_Triple_Axis_Accelerometer_-_LIS2DH_SKU__SEN0224

Currently `datasheets/` is empty (`E_BOARD_DS`-equivalent risk; v0.1 metadata-only).

## Existing reference implementation

The working LIS2DH driver lives inline at:

```
r2-workshop/firmware/esp32-c6/dfr1117/src/lis2dh.rs
```

That file is the authoritative source until extraction into this Cargo crate. When extracting:

1. Refactor as a standalone `no_std` Rust crate per R2-PLUGIN §12.2 layout.
2. Implement `r2_engine::plugin::Plugin` trait per §12.4 — `execute(command: u8, data: &[u8]) -> PluginResult` dispatches the opcodes in the table above.
3. Use `esp-idf-hal`'s I²C driver (the only async-capable I²C in the `esp-idf-svc` stack).
4. Keep `no_std` for the AOT path — no `String`, no `Vec` on hot paths; fixed-size buffers per R2-COMPILE §6.
5. Mirror the existing `r2-workshop/firmware/esp32-c6/dfr1117/src/lis2dh.rs` byte-for-byte where possible — that file already passes integration testing on real hardware. Don't rewrite it ground-up; refactor the modules into the proper crate shape.

## Known limitations

See [`PLUGIN.md`](PLUGIN.md) §9. The big ones:

- Source not yet extracted (this entry is metadata-only as of 2026-05-31).
- No FIFO mode in v0.1.
- No interrupt-driven sampling.
- NIF mode (Linux SBC bench testing) not supported.

## Read these files in this order (cold-start resume)

1. [`plugin.toml`](plugin.toml) — the contract.
2. [`PLUGIN.md`](PLUGIN.md) — full R2-PLUGIN §12.8 interface spec.
3. [`README.md`](README.md) — at-a-glance crate doc.
4. **Reference implementation:** `../../../../../../r2-workshop/firmware/esp32-c6/dfr1117/src/lis2dh.rs` — the working code.
5. **Upstream specs:**
   - `../../../../../specifications/SPEC-CATALOGUE-LAYOUT.md` §4.3 (directory layout + conformance)
   - `../../../../../../r2-specifications/specs/r2-core/R2-PLUGIN.md` §12 (canonical plugin authoring contract)
   - `../../../../../../r2-specifications/specs/r2-core/R2-COMPILE.md` §3, §6 (AOT compilable subset + memory model)
6. **Capability peer:** `../adxl355/` (when scaffolded) — same `provides` capability, different bus + precision; useful for cross-checking the capability interface.

## Authoring status

- ✅ `plugin.toml` (metadata-first; 2026-05-31)
- ✅ `PLUGIN.md` (all 10 mandatory sections)
- ✅ `README.md`
- ✅ `AI-CONTEXT.md` (this file)
- ⏳ `Cargo.toml` — Phase 1.4-source
- ⏳ `src/lib.rs` + `src/plugin.rs` + `src/driver.rs` — Phase 1.4-source (refactor from r2-workshop)
- ⏳ `datasheets/lis2dh-datasheet.pdf` — fetch via the authoring-flow WebFetch
- ⏳ `datasheets/sen0224-wiki.html` (or snapshot PDF) — fetch
- ⏳ `tests/` — native integration tests
- ✅ `conversation/2026-05-31-metadata-authored-01.md` — placeholder for this session's transcript

---

*Created 2026-05-31 as the first worked-example plugin entry. Phase 1.4-metadata complete; Phase 1.4-source pending.*
