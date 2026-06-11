//! `TestCoordinator` sentant (Phase 3 D5) — drives hardware-tier
//! transient-networking test runs and adjudicates them.
//!
//! ## Why this speaks the sim vocabulary
//!
//! Per the supervisor's alignment directive: the hardware rig and the in-memory
//! sim (r2-core `r2-harness`) MUST share **one semantic frame** so the campaign
//! coverage grid maps 1:1 and we never re-author tests. So a hardware test run
//! is the conjecture catalogue's `experiment` block verbatim —
//! `{ topology, timeline, expect }` (R2-TRANSIENT-NETWORKING.md §6) — and the
//! `expect` clauses are the **A6 assert set** keyed by [`MsgKey`]:
//! `exactly_once`, `no_duplicate`, `no_drop`, `copy_count`, `delivered_by`,
//! `reconcile_correct_after_heal`.
//!
//! This module is the **adjudication core**: a [`DeliveryLedger`] of what each
//! node reported receiving (keyed by `(origin, msg_id)`), and the A6 asserts
//! over it returning structured [`AssertFail`]s (collected, never panicked —
//! mirroring `r2-harness` `assert.rs`). The Sentant FSM that drives the timeline
//! (inject frames) and ingests per-node delivery reports off the `/r2/wire`
//! channel wraps this core — added next; the adjudication logic is proven here
//! standalone (Linux, no hardware).

use std::collections::BTreeMap;

/// A tracked frame, keyed by its originator + message id (the A6 `MsgKey`;
/// matches the conjecture catalogue's `(origin, msg_id)` addressing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MsgKey {
    /// Originating node id (NOT the immediate hop).
    pub origin: u32,
    /// Per-origin message id.
    pub msg_id: u32,
}

impl MsgKey {
    /// Construct a key.
    pub fn new(origin: u32, msg_id: u32) -> Self {
        Self { origin, msg_id }
    }
}

/// A node in the test topology, addressed by a scenario-local label (`"A"`,
/// `"B1"`, …) — independent of runtime ids, matching the catalogue's
/// `[tg, local_index]` addressing.
pub type NodeId = String;

/// What every node reported receiving: `(msg, node) -> copy count`. The rig
/// reports each `(origin, msg_id)` a node observed; duplicates increment the
/// count (so dedup / spray-and-wait copy bounds are checkable).
#[derive(Debug, Default, Clone)]
pub struct DeliveryLedger {
    counts: BTreeMap<(MsgKey, NodeId), u32>,
}

impl DeliveryLedger {
    /// New empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `node` received `msg` once (increments its copy count).
    pub fn record(&mut self, msg: MsgKey, node: &str) {
        *self.counts.entry((msg, node.to_string())).or_insert(0) += 1;
    }

    /// How many copies of `msg` reached `node`.
    pub fn copy_count(&self, msg: MsgKey, node: &str) -> u32 {
        self.counts.get(&(msg, node.to_string())).copied().unwrap_or(0)
    }

    /// Did `node` receive `msg` at least once?
    pub fn delivered_by(&self, msg: MsgKey, node: &str) -> bool {
        self.copy_count(msg, node) >= 1
    }

    /// The set of nodes that received `msg` (≥1 copy), sorted.
    pub fn nodes_with(&self, msg: MsgKey) -> Vec<NodeId> {
        let mut v: Vec<NodeId> = self
            .counts
            .iter()
            .filter(|((m, _), c)| *m == msg && **c >= 1)
            .map(|((_, n), _)| n.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

/// A failed expectation — structured + collected by the runner, never panicked
/// (mirrors `r2-harness` `assert.rs` `AssertFail`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertFail {
    /// Which assert helper failed (`exactly_once`, `no_drop`, …).
    pub which: &'static str,
    /// Human-readable detail (what was expected vs observed).
    pub detail: String,
}

/// `Ok(())` if every assert passed; the structured failure otherwise.
pub type AssertResult = Result<(), AssertFail>;

fn fail(which: &'static str, detail: String) -> AssertResult {
    Err(AssertFail { which, detail })
}

// ── A6 assert set (R2-TRANSIENT-NETWORKING.md §6) ────────────────────────

/// `msg` reached `dest` exactly once.
pub fn exactly_once(led: &DeliveryLedger, msg: MsgKey, dest: &str) -> AssertResult {
    match led.copy_count(msg, dest) {
        1 => Ok(()),
        n => fail("exactly_once", format!("{msg:?} reached {dest} {n}× (want 1)")),
    }
}

/// `msg` reached `node` at most `max` times (spray-and-wait / dedup bound).
pub fn copy_count(led: &DeliveryLedger, msg: MsgKey, node: &str, max: u32) -> AssertResult {
    let n = led.copy_count(msg, node);
    if n <= max {
        Ok(())
    } else {
        fail("copy_count", format!("{msg:?} reached {node} {n}× (max {max})"))
    }
}

/// No node received `msg` more than once (global dedup).
pub fn no_duplicate(led: &DeliveryLedger, msg: MsgKey) -> AssertResult {
    for ((m, node), c) in &led.counts {
        if *m == msg && *c > 1 {
            return fail("no_duplicate", format!("{msg:?} reached {node} {c}× (want ≤1)"));
        }
    }
    Ok(())
}

/// Every node in `dests` received `msg` at least once (nothing dropped).
pub fn no_drop(led: &DeliveryLedger, msg: MsgKey, dests: &[&str]) -> AssertResult {
    for d in dests {
        if !led.delivered_by(msg, d) {
            return fail("no_drop", format!("{msg:?} never reached {d}"));
        }
    }
    Ok(())
}

/// `node` received `msg` (≥1 copy).
pub fn delivered_by(led: &DeliveryLedger, msg: MsgKey, node: &str) -> AssertResult {
    if led.delivered_by(msg, node) {
        Ok(())
    } else {
        fail("delivered_by", format!("{msg:?} never reached {node}"))
    }
}

/// After a partition+heal run, the delivery set for `msg` over `dests` matches a
/// no-partition baseline run AND no node saw a duplicate across the seam (the
/// L3 invariant — no double-delivery, no drop at heal).
pub fn reconcile_correct_after_heal(
    actual: &DeliveryLedger,
    baseline: &DeliveryLedger,
    msg: MsgKey,
    dests: &[&str],
) -> AssertResult {
    no_duplicate(actual, msg)?;
    for d in dests {
        let a = actual.delivered_by(msg, d);
        let b = baseline.delivered_by(msg, d);
        if a != b {
            return fail(
                "reconcile_correct_after_heal",
                format!("{msg:?} at {d}: post-heal delivered={a}, baseline={b}"),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: MsgKey = MsgKey { origin: 1, msg_id: 7 };

    fn ledger(records: &[(&str, u32)]) -> DeliveryLedger {
        let mut l = DeliveryLedger::new();
        for (node, n) in records {
            for _ in 0..*n {
                l.record(M, node);
            }
        }
        l
    }

    #[test]
    fn exactly_once_pass_and_fail() {
        let l = ledger(&[("B", 1), ("C", 1)]);
        assert!(exactly_once(&l, M, "B").is_ok());
        // zero copies fails
        assert_eq!(exactly_once(&l, M, "Z").unwrap_err().which, "exactly_once");
        // two copies fails
        let l2 = ledger(&[("B", 2)]);
        assert_eq!(exactly_once(&l2, M, "B").unwrap_err().which, "exactly_once");
    }

    #[test]
    fn copy_count_bound() {
        let l = ledger(&[("B", 1)]);
        assert!(copy_count(&l, M, "B", 1).is_ok());
        let l2 = ledger(&[("B", 3)]);
        assert!(copy_count(&l2, M, "B", 1).is_err());
        assert!(copy_count(&l2, M, "B", 3).is_ok());
    }

    #[test]
    fn no_duplicate_detects_relay_copy() {
        assert!(no_duplicate(&ledger(&[("B", 1), ("C", 1)]), M).is_ok());
        assert_eq!(no_duplicate(&ledger(&[("C", 2)]), M).unwrap_err().which, "no_duplicate");
    }

    #[test]
    fn no_drop_over_destination_set() {
        let l = ledger(&[("B", 1), ("C", 1)]);
        assert!(no_drop(&l, M, &["B", "C"]).is_ok());
        assert_eq!(no_drop(&l, M, &["B", "C", "D"]).unwrap_err().which, "no_drop");
    }

    #[test]
    fn delivered_by_and_nodes_with() {
        let l = ledger(&[("B", 1), ("C", 2)]);
        assert!(delivered_by(&l, M, "C").is_ok());
        assert!(delivered_by(&l, M, "Z").is_err());
        assert_eq!(l.nodes_with(M), vec!["B".to_string(), "C".to_string()]);
    }

    #[test]
    fn reconcile_matches_baseline() {
        // baseline (no partition): B + C both delivered, once each.
        let baseline = ledger(&[("B", 1), ("C", 1)]);
        // healed run: same delivery set, no dup → reconciles.
        let healed_ok = ledger(&[("B", 1), ("C", 1)]);
        assert!(reconcile_correct_after_heal(&healed_ok, &baseline, M, &["B", "C"]).is_ok());
        // a drop at the seam (C missing) → fails.
        let dropped = ledger(&[("B", 1)]);
        assert_eq!(
            reconcile_correct_after_heal(&dropped, &baseline, M, &["B", "C"]).unwrap_err().which,
            "reconcile_correct_after_heal"
        );
        // a duplicate across the seam → fails (no_duplicate first).
        let duped = ledger(&[("B", 1), ("C", 2)]);
        assert_eq!(
            reconcile_correct_after_heal(&duped, &baseline, M, &["B", "C"]).unwrap_err().which,
            "no_duplicate"
        );
    }
}
