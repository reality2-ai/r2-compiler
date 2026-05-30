# 2026-05-31 — lis2dh metadata authored

First worked-example plugin entry in r2-compiler's catalogue. Authored as part of Phase 1.4-metadata to validate SPEC-CATALOGUE-LAYOUT §4.3 conformance + the R2-PLUGIN §12.3/§12.8 disciplines.

## Context

Roy directed (2026-05-31): "1.4. Something to consider also, how might we do a similar process of plugins for the webapp." Phase 1.4 is the per-plugin and per-sentant extraction from r2-workshop's rocker-sensor ensemble into catalogue/ensembles/rocker-sensor/. To keep scope manageable, Phase 1.4 is split into two stages:

- **Phase 1.4-metadata** (this session) — author `plugin.toml`, `PLUGIN.md`, `README.md`, `AI-CONTEXT.md` for one worked-example plugin + one worked-example sentant. Validates the pattern before scaling.
- **Phase 1.4-source** (later) — extract the actual Rust source from r2-workshop's inline modules into standalone Cargo crates. The heavy lift.

`lis2dh` chosen as the first plugin because it's:
- The dfr1117 carrier's sensing element — proves the cross-architecture (RISC-V) plugin authoring path.
- Capability-paired with `adxl355` — exemplar of R2-PLUGIN §10's swap lever (same capability provided, different chip + bus).
- Already implemented in r2-workshop (`firmware/esp32-c6/dfr1117/src/lis2dh.rs`) — real reference, not greenfield.

## Decisions

| # | Decision |
|---|---|
| D-1 | Metadata-first authoring. `plugin.toml` + `PLUGIN.md` + `AI-CONTEXT.md` define the interface contract; source extraction is a separate later phase. |
| D-2 | This plugin declares ONLY `aot` mode in v0.1. NIF + web modes are explicitly `false`. Per R2-PLUGIN §12.1: silent incompatibility is a spec violation; declare what's not supported. |
| D-3 | The plugin provides both `ai.reality2.cap.accel.triaxial` (the generic swap-lever capability) AND `ai.reality2.cap.accel.triaxial.10bit` (a precision-specifier hint). Sentants binding by the generic capability get either lis2dh or adxl355 transparently; downstream consumers needing the precision hint can prefer one. |
| D-4 | Commands enumerated explicitly with opcodes per R2-PLUGIN §12.4.1 — even though source isn't extracted yet, the contract is firm. Future source MUST implement these exact opcodes. |
| D-5 | Configuration defaults baked in `data:` rather than `plugin.toml [config]` — matches r2-workshop's existing pattern (sentant `data:` block in the YAML score parameterises the plugin invocation). |

## Open items

- Source extraction (Phase 1.4-source). The reference `r2-workshop/firmware/esp32-c6/dfr1117/src/lis2dh.rs` already works on real hardware; the extraction is a refactor not a rewrite.
- Datasheet PDFs — to fetch via the authoring-flow WebFetch when that exists.
- Capability-peer plugin (`adxl355`) — same metadata pattern, deferred for now since one worked example is enough to validate the layout.

## Next session

Either:
- Author `adxl355` plugin metadata using lis2dh as the template (proves the pattern repeats).
- OR move to `Identity` sentant metadata authoring (covered in this same session — see `../../../../sentants/Identity/conversation/`).
- OR start Phase 1.4-source for lis2dh (write the actual Cargo crate by refactoring r2-workshop's source).
