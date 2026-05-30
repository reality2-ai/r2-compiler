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
│   ├── plugins/                # plugins do the actual work (subprocess, I/O, network)
│   │   ├── claude_code.rs      # `claude -p` subprocess driver, stream-json parsing
│   │   ├── compiler.rs         # materialises per-carrier crate, drives claude-code + cargo, returns artefact
│   │   ├── cargo_runner.rs     # `cargo build` wrapper
│   │   ├── flasher.rs          # `esptool write_flash`
│   │   ├── ota_push.rs         # TCP push to device port 21043
│   │   ├── webfetch.rs         # datasheet fetcher
│   │   ├── git_runner.rs       # git wrapper (for sync)
│   │   ├── sync.rs             # wraps tools/sync-catalogue.sh
│   │   ├── catalogue.rs        # watches catalogue/ tree on disk
│   │   └── keyholder.rs        # TG private-key + cert issuance
│   └── sentants/               # sentants are thin FSMs routing events to plugins
│       ├── catalogue.rs        # routes r2.compiler.catalogue.*
│       ├── builder.rs          # per-build FSM; routes r2.compiler.build.*
│       ├── author.rs           # authoring-session FSM; routes r2.compiler.author.*
│       ├── deploy.rs           # per-device deploy FSM; routes r2.compiler.deploy.*
│       ├── sync.rs             # routes r2.compiler.sync.*
│       └── tg.rs               # TG management; routes r2.compiler.tg.*
└── prompts/
    ├── compile.md              # Tera template — the build brief for claude -p
    ├── author-board.md
    ├── author-plugin.md
    └── author-sentant.md
```

## Spec

[`../specifications/SPEC-R2-COMPILER.md`](../specifications/SPEC-R2-COMPILER.md) — esp. §3 (composition), §4 (events), §5 (compile path), §7 (authoring).
