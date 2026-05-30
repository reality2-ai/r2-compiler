//! OTP-style supervision: restart strategies, restart-intensity windows,
//! per-child policies, and backoff. Modelled on Erlang/OTP supervisor
//! semantics (see <https://www.erlang.org/doc/system/sup_princ.html>),
//! adapted to Rust's panic-unwind model.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Ensemble-level supervisor strategy. On a child sentant crash, the
/// registry consults this enum to decide which siblings (if any) are
/// also restarted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RestartStrategy {
    /// Restart only the crashed sentant. Default.
    #[default]
    OneForOne,
    /// Restart every sentant in the ensemble.
    OneForAll,
    /// Restart the crashed sentant and every sentant defined after it
    /// in score order.
    RestForOne,
}

/// Per-sentant restart policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RestartPolicy {
    /// Always restart. Default — matches OTP `:permanent`.
    #[default]
    Permanent,
    /// Restart only on panic; never on a clean exit. Matches OTP
    /// `:transient`. (v0.1 has no clean-exit channel, so this behaves
    /// the same as `Permanent` for now — the distinction lights up
    /// when we add a `Sentant::stop` hook.)
    Transient,
    /// Never restart. Matches OTP `:temporary`.
    Temporary,
}

/// Backoff policy applied between successive restart attempts of the
/// same sentant within the intensity window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffPolicy {
    /// Constant delay between restarts.
    Constant {
        /// Delay in milliseconds.
        delay_ms: u32,
    },
    /// Exponential backoff: `delay_n = base_ms * 2^n`, capped at
    /// `max_ms`.
    Exponential {
        /// Initial delay.
        base_ms: u32,
        /// Cap. Once `base_ms * 2^n` exceeds this, every subsequent
        /// delay is `max_ms`.
        max_ms: u32,
    },
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        BackoffPolicy::Exponential {
            base_ms: 100,
            max_ms: 5_000,
        }
    }
}

impl BackoffPolicy {
    /// Compute the delay before the `n`-th restart attempt within the
    /// current intensity window. `n == 0` is the first restart.
    pub fn delay_for(&self, n: u32) -> Duration {
        match self {
            BackoffPolicy::Constant { delay_ms } => Duration::from_millis(*delay_ms as u64),
            BackoffPolicy::Exponential { base_ms, max_ms } => {
                let shift = n.min(31);
                let shifted = (*base_ms as u64).checked_shl(shift).unwrap_or(u64::MAX);
                Duration::from_millis(shifted.min(*max_ms as u64))
            }
        }
    }
}

/// Configuration for one ensemble's supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupervisionConfig {
    /// Restart strategy for the ensemble.
    pub strategy: RestartStrategy,
    /// Maximum restarts within `period` before escalating to `Failed`.
    pub max_restarts: u32,
    /// Sliding window length.
    pub period: Duration,
    /// Backoff between restart attempts.
    pub backoff: BackoffPolicy,
}

impl Default for SupervisionConfig {
    fn default() -> Self {
        Self {
            strategy: RestartStrategy::OneForOne,
            max_restarts: 3,
            period: Duration::from_secs(60),
            backoff: BackoffPolicy::default(),
        }
    }
}

/// Sliding-window restart counter.
///
/// Tracks recent restart timestamps; on each `record`, evicts any
/// timestamp older than `period`. The window is bounded by
/// `max_restarts + 1` so a single restart over the cap triggers
/// escalation.
#[derive(Debug)]
pub struct RestartLedger {
    timestamps: VecDeque<Instant>,
    period: Duration,
    cap: u32,
}

impl RestartLedger {
    /// Create a fresh ledger.
    pub fn new(cfg: &SupervisionConfig) -> Self {
        Self {
            timestamps: VecDeque::with_capacity(cfg.max_restarts as usize + 1),
            period: cfg.period,
            cap: cfg.max_restarts,
        }
    }

    /// Evict expired entries (older than `period`).
    fn evict(&mut self, now: Instant) {
        while let Some(&t) = self.timestamps.front() {
            if now.duration_since(t) > self.period {
                self.timestamps.pop_front();
            } else {
                break;
            }
        }
    }

    /// Record a restart at `now`. Returns the count of live restarts in
    /// the window after recording. If the count exceeds `cap`, the
    /// caller MUST escalate.
    pub fn record(&mut self, now: Instant) -> u32 {
        self.evict(now);
        self.timestamps.push_back(now);
        self.timestamps.len() as u32
    }

    /// `true` iff recording another restart now would exceed `cap`.
    pub fn would_exceed(&mut self, now: Instant) -> bool {
        self.evict(now);
        self.timestamps.len() as u32 >= self.cap
    }

    /// Restart count for backoff calculation: live restarts in the
    /// current window (excluding the one being recorded).
    pub fn live_count(&mut self, now: Instant) -> u32 {
        self.evict(now);
        self.timestamps.len() as u32
    }

    /// Reset the ledger (used after a successful manual reload, or
    /// when an operator clears a `Failed` ensemble).
    pub fn clear(&mut self) {
        self.timestamps.clear();
    }
}

/// Lifecycle status of a loaded ensemble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnsembleStatus {
    /// All sentants healthy and dispatchable.
    Healthy,
    /// At least one sentant is being restarted; dispatch to gated
    /// instances returns `Backpressure`.
    Degraded,
    /// Restart-intensity exceeded; dispatch to all sentants in the
    /// ensemble returns `NoHandler`. Operator action required to
    /// reload.
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_backoff_grows_then_caps() {
        let p = BackoffPolicy::Exponential { base_ms: 100, max_ms: 1_000 };
        assert_eq!(p.delay_for(0), Duration::from_millis(100));
        assert_eq!(p.delay_for(1), Duration::from_millis(200));
        assert_eq!(p.delay_for(2), Duration::from_millis(400));
        assert_eq!(p.delay_for(3), Duration::from_millis(800));
        assert_eq!(p.delay_for(4), Duration::from_millis(1_000));
        assert_eq!(p.delay_for(20), Duration::from_millis(1_000));
    }

    #[test]
    fn restart_ledger_evicts_old_timestamps() {
        let cfg = SupervisionConfig {
            max_restarts: 3,
            period: Duration::from_millis(100),
            ..Default::default()
        };
        let mut ledger = RestartLedger::new(&cfg);
        let t0 = Instant::now();
        ledger.record(t0);
        ledger.record(t0 + Duration::from_millis(10));
        // 200ms later, both should be evicted.
        let after = ledger.live_count(t0 + Duration::from_millis(200));
        assert_eq!(after, 0);
    }

    #[test]
    fn restart_ledger_would_exceed() {
        let cfg = SupervisionConfig {
            max_restarts: 2,
            period: Duration::from_secs(60),
            ..Default::default()
        };
        let mut ledger = RestartLedger::new(&cfg);
        let t0 = Instant::now();
        ledger.record(t0);
        ledger.record(t0);
        // Cap is 2; two records means another would exceed.
        assert!(ledger.would_exceed(t0));
    }
}
