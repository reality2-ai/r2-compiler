# r2-def

Reference parser for R2-DEF ensemble, sentant, and swarm definition
files. Implements R2-DEF v0.1 (`r2-specifications/specs/r2-core/R2-DEF.md`)
across YAML, JSON, and TOML dialects, with light structural validation
and round-trip serde support.

This crate is intentionally *parser only* — no runtime semantics, no
plugin instantiation, no signature verification. Heavier rules live in
the runtime ([`r2-ensemble`](../r2-ensemble/)) and the trust layer
([`r2-trust`](../r2-trust/)).

---

## Public API

```rust
use r2_def::{
    parse_ensemble_yaml, parse_ensemble_json, parse_ensemble_toml,
    parse_sentant_yaml,  parse_sentant_json,  parse_sentant_toml,
    parse_swarm_yaml,    parse_swarm_json,    parse_swarm_toml,
};

let score: EnsembleScore = parse_ensemble_yaml(yaml_str)?;
score.validate()?;        // light structural check
```

### Public types

| Type | Purpose |
|---|---|
| `SentantDef` | One sentant — name, class, description, storage policy, vars, plugin refs, automations |
| `Automation` | Named FSM, vector of `Transition`s |
| `Transition` | `event` (REQUIRED), optional `from`/`to`, `parameters`, `actions` (opaque) |
| `StoragePolicy` | `Volatile` / `Durable` / `DurableState` (R2-DEF §2.3) |
| `PluginDef` | Ensemble-owned plugin (kind, image, config) |
| `PluginRef` | Sentant-side reference to a plugin (R2-DEF §4) |
| `EnsembleScore` | Top-level score (name, version, sentants, plugins, registrations, capabilities, trust_group, signatures) |
| `SentantEntry` | `Inline(SentantDef)` or `External { include: path }` |
| `CapabilityAggregate` | What an ensemble emits/consumes |
| `TrustGroupConstraints` | min crypto level, allowed roles, entanglement scope |
| `Signature` | Ed25519 signature record (signer / algorithm / signature / signed_at / scope) |

`EnsembleFile`, `SentantFile`, `SwarmFile` wrap each top-level form
(`ensemble: …`, `sentant: …`, `swarm: …`).

### Validation

Structural rules per R2-DEF §7.10 and §8.1:

- name / description / version must be non-empty
- ensemble_version must be `"0.1"`
- at least one sentant
- automation names unique per sentant
- plugin names unique per ensemble and per sentant
- inline sentants are recursively validated

Heavier rules — action-command validity, plugin-ref resolution, signature
chain verification — are runtime concerns and live in `r2-ensemble` /
`r2-trust`.

---

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `yaml` | on | enables `parse_*_yaml` via `serde_yaml` |
| `json` | on | enables `parse_*_json` via `serde_json` |
| `toml` | on | enables `parse_*_toml` via `toml` |

Building with only one dialect is supported and reduces compile time.

---

## R2 crates this crate uses

None — `r2-def` is purely a serde-driven parser. It exists below the
runtime layer so other crates (`r2-ensemble`, `r2-forge`, `r2-build`,
score linters) can consume parsed scores without pulling in a sentant
runtime.

External dependencies: `serde`, `serde_yaml`, `serde_json`, `toml`,
`thiserror`.

---

## Fixtures

Round-trip fixtures live in [`vectors/`](vectors/). Each fixture is
parsed, validated, and serialised back through serde to confirm no
information is lost.

---

## License

Reality2 follows an **open-core** model
(`r2-specifications/specs/thurisaz/TH-ESG.md §8`):

- The R2 protocol suite — including this crate — is open source.
- The Mariko marketplace and vertical-market services (TH-MARKET) are
  licensed commercially and live elsewhere.

This crate is dual-licensed under either of:

- **Apache License, Version 2.0** ([`LICENSE-APACHE`](../../LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- **MIT License** ([`LICENSE-MIT`](../../LICENSE-MIT) or
  <https://opensource.org/licenses/MIT>)

at your option — the standard permissive Rust ecosystem dual license.
No copyleft obligation.

Contributions are accepted under the same dual license unless you say
otherwise, per the Apache-2.0 contribution clause.
