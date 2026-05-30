# catalogue/ensembles/

R2 ensembles — one directory per ensemble. The canvas's second opt-in part type (the first being carrier boards). The operator picks one or more ensembles per build.

An ensemble is the **user-meaningful unit of functionality** (R2-ENSEMBLE §1.1) — sentants + ensemble-owned plugins + UI registrations, with one class string and one purpose. Sentants and plugins live **inside** their ensemble, not as separate top-level catalogue trees.

## Layout

See [`../../specifications/SPEC-CATALOGUE-LAYOUT.md`](../../specifications/SPEC-CATALOGUE-LAYOUT.md) §4 for the normative layout. Summary:

```
catalogue/ensembles/<name>/
  ensemble.yaml         # R2-DEF §7 score — canonical artefact
  ENSEMBLE.md           # narrative + composition diagram
  AI-CONTEXT.md
  plugins/              # ensemble-owned (R2-ENSEMBLE §2.1.2)
    <category>/<name>/  # full per-plugin shell (plugin.toml, PLUGIN.md, src/, datasheets/, …)
  sentants/
    <Name>/             # sentant.yaml, SENTANT.md, AI-CONTEXT.md, …
  datasheets/
  conversation/
```

`<name>` is kebab-case and equals the `ensemble.name` field.

## Conformance gate

- `ensemble.yaml` MUST pass R2-DEF §7.10 load-time validation.
- Every nested plugin under `plugins/` MUST pass R2-PLUGIN §12.3 / §12.5 / §12.8 checks.
- Every nested sentant under `sentants/` MUST pass R2-DEF §2 + R2-COMPILE §3.1 (for AOT) checks.
- `compile_target` in `ensemble.yaml` MUST overlap with at least one board in [`../boards/`](../boards/).

## Adding an ensemble

Through the visual UI's **+ New Ensemble** button or `tools/new-entry.sh ensemble <name>`. The agent will ask whether to fork an existing ensemble or scaffold from scratch; either way, the result is a self-contained directory with everything a future CC session needs.

## Adding a plugin / sentant *inside* an existing ensemble

The operator opens an ensemble on the canvas and clicks **+ New Plugin** / **+ New Sentant**. The authoring agent is scoped to that ensemble's directory and writes the new entry under `plugins/<cat>/<name>/` or `sentants/<Name>/`.

Plugins / sentants cannot be created at the top level — they're always scoped to an ensemble (or a board, for hive-shared singletons; see [`../boards/_README.md`](../boards/_README.md)).

## v0.1 target ensembles

| Ensemble | Source | Notes |
|---|---|---|
| `rocker-sensor` | `r2-workshop/ensemble/sensor.yaml` | The first round-trip target. 15 sentants + multiple plugins. |

Future deployments (notekeeper-style, photo-share, etc.) become separate ensemble entries.
