# webapp/

The browser-side R2 hive + UI for r2-compiler. Same architecture as `r2-workshop/webapp/`: a WASM bundle that hosts a real R2 hive in the browser tab.

## Status (2026-05-31)

**Not implemented.** Directory shell only. Phase 2 in `../plan/PLAN.md`.

## Planned structure

```
webapp/
├── crate/                      # Rust crate compiled to wasm32-unknown-unknown
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # bindgen entrypoints
│       ├── hive.rs             # R2 hive setup using r2-wasm bindings
│       └── sentants/
│           ├── catalogue.rs    # Catalogue browser sentant
│           ├── composition.rs  # Canvas state sentant
│           ├── source_viewer.rs
│           ├── builder.rs
│           └── author.rs
├── ui/                         # plain JS — DOM, canvas, source viewer
│   ├── index.js                # bootstraps the hive + binds DOM
│   ├── canvas.js               # drag-and-drop layout
│   ├── source.js               # CodeMirror or shiki for Rust syntax
│   ├── catalogue.js            # the around-the-edges panels
│   └── console.js              # build-progress + agent-dialog pane
├── styles/
│   └── compiler.css
├── index.html
└── dist/                       # built bundle (in .gitignore)
```

## Layering

| Layer | Purpose |
|---|---|
| WASM | Protocol + crypto: frame decode/encode, HMAC verify, TG key derivation, R2-WIRE state, per-event dispatch. Catalogue + composition + source-viewer + builder sentants. |
| Plain JS | UX: DOM, drag-and-drop canvas, source viewer, layout, event handlers. |

Mirrors r2-workshop's `webapp/` split (which is the working reference).

## Spec

[`../specifications/SPEC-R2-COMPILER.md`](../specifications/SPEC-R2-COMPILER.md) — esp. §3.2 (browser-hive sentants), §3.3 (webapp plugins), §4 (event vocabulary).
