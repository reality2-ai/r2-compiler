# scores/

Complete R2-DEF §7 ensemble scores — the visual canvas's serialised output, and the input to the compile path.

## Source-of-truth flow

```
Catalogue (boards/plugins/sentants — building blocks)
    │
    │ operator composes on visual canvas
    ▼
Composition (in-memory, in webapp hive)
    │
    │ serialise on `r2.compiler.composition.preview` or build trigger
    ▼
scores/<name>.yaml  ←  this directory
    │
    │ r2.compiler.build.start
    ▼
out/<carrier>-<timestamp>/  (per-carrier firmware crate)
```

Every score in this directory MUST be a valid R2-DEF §7 ensemble — name, description, version, ensemble_version, class, sentants, plugins, registrations, capabilities, compile_target, signatures (optional in dev). See R2-DEF §7.10 for the validation table.

## v0.1 seed scores

| File | Source | Purpose |
|---|---|---|
| `rocker-sensor.yaml` | synced from `r2-workshop/ensemble/sensor.yaml` | The first success-gate score. r2-compiler must round-trip the three r2-workshop carriers given this. |

## Adding a score

Two paths:

- **Through the visual UI** — compose on the canvas, hit Compile, the orchestrator writes the score under `scores/<auto-name>-<timestamp>.yaml` and uses it for the build.
- **By hand** — author a score in YAML that validates against R2-DEF §7, drop it here. The orchestrator's catalogue plugin picks it up immediately. Useful for round-trip vectors and reproducibility.

## Conformance

Scores in this directory MUST pass R2-DEF §7.10 validation. The orchestrator's compiler plugin refuses to build a non-conforming score and emits `r2.compiler.build.error{phase: "preparing"}` instead.
