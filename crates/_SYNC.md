# crates/ — R2 protocol crates (vendored from r2-core)

This directory holds vendored copies of the R2 protocol crates from `r2-core/crates/`. Vendoring matches `r2-workshop`'s pattern (per `r2-workshop/AI-CONTEXT.md`: "Project is self-contained — no path deps on `../r2-core`. R2 protocol crates will be vendored into `crates/` when they're needed").

## Why vendor

1. **Self-contained build.** r2-composer must compile against a known-good snapshot of r2-core. A path dep would break whenever upstream's `Cargo.toml` reshuffles.
2. **Audit gate.** Vendoring forces an explicit sync step — we see exactly which upstream changes land in r2-composer.
3. **Hermetic CI.** CI does not need access to r2-core's git history; the vendored crates are part of this repo.

## Which crates to vendor (v0.1)

The orchestrator and webapp hives both need:

| Crate | From r2-core | Why |
|---|---|---|
| `r2-engine` | `r2-core/crates/r2-engine` | `Plugin` trait, `Sentant` trait, `EventBus`, hive lifecycle |
| `r2-fnv` | `r2-core/crates/r2-fnv` | Event-name hashing |
| `r2-cbor` | `r2-core/crates/r2-cbor` | Wire format payloads |
| `r2-wire` | `r2-core/crates/r2-wire` | Frame format |
| `r2-trust` | `r2-core/crates/r2-trust` | TG membership, certs, HKDF |
| `r2-route` | `r2-core/crates/r2-route` | Event routing |
| `r2-def` | `r2-core/crates/r2-def` | R2-DEF score parser + validation |
| `r2-ensemble` | `r2-core/crates/r2-ensemble` | Ensemble score model |
| `r2-wasm` | `r2-core/crates/r2-wasm` | Browser-hive bindings (webapp uses this) |

The orchestrator uses Linux/host async transports (axum + tokio); it does NOT need `r2-esp` (Xtensa/RISC-V only).

## Sync mechanism

`tools/sync-catalogue.sh` (not yet written) MUST be the only path that touches `crates/`. The script:

1. Reads the pinned upstream commit from `crates/_VERSIONS.toml` (also not yet written).
2. For each crate in the table, copies its contents from `../r2-core/crates/<name>/` overwriting `crates/<name>/`.
3. Strips upstream `[dev-dependencies]` that reference crates not vendored.
4. Patches any internal `path = "../<other>"` deps to point at this repo's vendored copy.
5. Commits the sync as a single commit titled `sync: r2-core@<sha>`.

## Conformance

- This directory MUST NOT be edited by hand. Local patches go upstream first, then sync.
- The `_VERSIONS.toml` file MUST be present and reflect the actual commit hashes of the vendored crates.
- Drift (vendored copy differs from `_VERSIONS.toml` claim) is a bug and CI MUST detect it.

## v0.1 status

**Not yet populated.** The directory shell exists; vendoring will happen as part of the orchestrator scaffolding phase.

## Re-sync log

- **2026-06-12 — `r2-def` only → r2-core `d6f2a6a` (≥ a257ac9).** Targeted re-sync
  (rsync of `crates/r2-def` alone, not a full re-vendor) to pick up the canonical
  web-UI registration model (`EnsembleScore::web_registration() -> WebPluginManifest`,
  `registrations.r2-web { mount, bundle }`) + the resilient `web_template` test
  (which had been failing-to-compile and breaking workspace-wide `cargo test`).
  The orchestrator does **not** depend on `r2-def` (only r2-engine/fnv/wire/trust),
  so blast radius = r2-def's own build/tests; verified `cargo test` fully green
  (orchestrator suite 140/140, r2-def tests pass). The **other** vendored crates
  remain at the original `54d2e1c` pin — a full re-vendor (r2-core has moved far
  since) is a separate, coordinated operation, not done here.
