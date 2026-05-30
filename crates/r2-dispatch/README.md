# r2-dispatch

Local event dispatch contract for R2 hives. Implements the normative behaviour
described in R2-RUNTIME §2.4 (see `r2-specifications`).

See `src/lib.rs` for crate-level documentation.

## Why this crate exists

R2-ROUTE's `ForwardAction::DeliverOnly` means "the routing layer has done its
job; now deliver the event to whatever sentant runtime this hive has loaded."
Before this crate, every hive wired that up ad-hoc. Now there is a normative
contract, and apps written against the contract work across all conformant R2
implementations (r2-hive Rust, r2-hive Elixir, hypothetical Go or Python hives).

## Relationship to other crates

- **Depends on:** nothing (standalone contract crate).
- **Depended on by:** `r2-hive` (attaches a `DispatchTarget`), and eventually
  `r2-ensemble` (loaders install the target that routes to sentants).
