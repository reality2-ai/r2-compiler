# tools/

Scripts that support r2-compiler's catalogue + build lifecycle.

## Planned scripts (v0.1)

| Script | Purpose |
|---|---|
| `sync-catalogue.sh` | Pull `crates/`, `catalogue/plugins/`, `catalogue/sentants/`, and `catalogue/boards/<each>/templates/` from `../r2-core/` and `../r2-workshop/`. Single source of truth for catalogue freshness. |
| `new-entry.sh <kind> <name>` | CLI fallback for the authoring flow when the webapp is not running. Scaffolds the empty directory shell per SPEC-CATALOGUE-LAYOUT and emits a brief the operator can paste into `claude` manually. |

## Status (2026-05-31)

**Empty.** Scripts will land alongside Phase 1 in `../plan/PLAN.md`.
