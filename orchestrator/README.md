# orchestrator/

The workstation-side R2 hive — a Rust binary that:

- hosts an R2 hive (TG member, same shape as `r2-workshop/dashboard/`),
- serves the webapp WASM bundle on `localhost:21050/r2-compiler/` (or the operator-configured port),
- listens for `r2.compiler.*` events from the browser hive on the same port (peek-protocol-detect per R2-WIRE §13.5),
- spawns `claude -p '<brief>' --output-format=stream-json` per build / authoring session,
- runs `cargo build` (and optionally `esptool`) against the produced per-carrier crate,
- syncs `catalogue/` and `crates/` from sibling repos on demand.

## Status (2026-05-31)

**Not implemented.** Directory shell only. Phase 1.5 in `../plan/PLAN.md`.

## Planned structure

```
orchestrator/
├── Cargo.toml
├── src/
│   ├── main.rs                 # axum WSS + static serve + R2 hive setup
│   ├── hive.rs                 # TG membership, sentant registration
│   ├── claude_code.rs          # claude -p subprocess driver, stream-json parsing
│   ├── plugins/
│   │   ├── cargo_runner.rs     # cargo build wrapper
│   │   ├── git_runner.rs       # for catalogue sync
│   │   └── webfetch.rs         # datasheet fetcher
│   └── sentants/
│       ├── catalogue_server.rs # r2.compiler.catalogue.*
│       ├── compiler.rs         # r2.compiler.build.* — the central FSM
│       ├── author_pilot.rs     # r2.compiler.author.* — catalogue authoring
│       ├── flasher.rs          # r2.compiler.flash.*
│       └── sync.rs             # tools/sync-catalogue.sh wrapper
└── prompts/
    ├── compile.md              # Tera template — the build brief for claude -p
    ├── author-board.md
    ├── author-plugin.md
    └── author-sentant.md
```

## Spec

[`../specifications/SPEC-R2-COMPILER.md`](../specifications/SPEC-R2-COMPILER.md) — esp. §3 (composition), §4 (events), §5 (compile path), §7 (authoring).
