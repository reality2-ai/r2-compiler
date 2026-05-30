//! `Builder` sentant — per-build state machine that drives a compile.
//!
//! **Phase 1.7a stub.** Receives `r2.compiler.build.start`, emits three
//! synthetic progress events + a `done`. Phase 1.7b replaces the stub
//! body with a dispatch to the real `claude-code` plugin (which spawns
//! `claude -p '<brief>' --output-format=stream-json` and parses the
//! stream-json output back into progress events per SPEC-R2-COMPILER §5).
//!
//! Per [[feedback-sentants-vs-plugins-terminology]] this sentant is a
//! thin FSM router — the actual subprocess + filesystem work happens
//! in the `claude-code` and `cargo-runner` plugins.

use r2_engine::action::PayloadBuf;
use r2_engine::{Action, ActionBuf, Event, Sentant, StateId, Target};

use crate::bridge::registry;

/// Idle → Working → Idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum State {
    Idle = 0,
    Working = 1,
}

/// Phase 1.7a stub `Builder` sentant.
pub struct BuilderSentant {
    state: State,
    /// FNV hash of `r2.compiler.build.start` — cached for the subscription list.
    start_hash: u32,
    /// Pre-hashed event names for the synthetic responses.
    progress_hash: u32,
    done_hash: u32,
}

impl BuilderSentant {
    /// Construct the sentant. Subscribes only to `r2.compiler.build.start`
    /// in v0.1; Phase 1.7+ adds `.cancel` + plugin-result events.
    pub fn new() -> Self {
        let reg = registry();
        Self {
            state: State::Idle,
            start_hash: reg.hash_of("r2.compiler.build.start").unwrap(),
            progress_hash: reg.hash_of("r2.compiler.build.progress").unwrap(),
            done_hash: reg.hash_of("r2.compiler.build.done").unwrap(),
        }
    }

    fn emit_progress(&self, actions: &mut ActionBuf, phase: &str, message: &str) {
        let payload = serde_json::json!({"phase": phase, "message": message});
        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
        actions.push(Action::Send {
            target: Target::Broadcast,
            event_hash: self.progress_hash,
            payload: PayloadBuf::from_slice(&bytes),
        });
    }

    fn emit_done(&self, actions: &mut ActionBuf, summary: &str) {
        let payload = serde_json::json!({
            "summary": summary,
            "note": "Phase 1.7a stub — real build path lands when the claude-code plugin is wired in.",
        });
        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
        actions.push(Action::Send {
            target: Target::Broadcast,
            event_hash: self.done_hash,
            payload: PayloadBuf::from_slice(&bytes),
        });
    }
}

impl Default for BuilderSentant {
    fn default() -> Self {
        Self::new()
    }
}

impl Sentant for BuilderSentant {
    fn handle_event(&mut self, event: &Event, actions: &mut ActionBuf) {
        if event.hash != self.start_hash {
            return;
        }
        // Decode the payload as JSON — extract score + target for the log.
        let parsed: serde_json::Value = serde_json::from_slice(event.payload).unwrap_or(serde_json::Value::Null);
        let target = parsed.get("target").and_then(|v| v.as_str()).unwrap_or("?");
        let score = parsed.get("score").and_then(|v| v.as_str()).unwrap_or("?");

        self.state = State::Working;

        // Synthetic 3-phase build progress so the WS round-trip is visible.
        self.emit_progress(
            actions,
            "preparing",
            &format!("would resolve score={score} for target={target}"),
        );
        self.emit_progress(
            actions,
            "generating",
            "would dispatch claude-code plugin (Phase 1.7b)",
        );
        self.emit_progress(
            actions,
            "compiling",
            "would run cargo build --release --target <triple> (Phase 1.7b)",
        );
        self.emit_done(actions, "Phase 1.7a stub completed — wire claude-code next.");

        self.state = State::Idle;
    }

    fn state(&self) -> StateId {
        self.state as StateId
    }

    fn class_hash(&self) -> u32 {
        // The Builder sentant's class string: ai.reality2.r2-compiler.builder
        r2_fnv::fnv1a_32(b"ai.reality2.r2-compiler.builder")
    }

    fn name(&self) -> &str {
        "Builder"
    }

    fn subscriptions(&self) -> &[u32] {
        // Returning a slice means the slice must outlive Self. We cache
        // the start_hash in a one-element array on the heap via a static
        // pattern: keep it in a Box leaked once per instance. For Phase
        // 1.7a there is at most one BuilderSentant; the leak is bounded.
        //
        // (R2-COMPILE §3.1 generated sentants will return a `&'static`
        // slice from a const table — that's the canonical path. For
        // hand-written sentants on Linux, a leaked Box is fine.)
        use std::sync::OnceLock;
        static SUBS: OnceLock<&'static [u32]> = OnceLock::new();
        SUBS.get_or_init(|| {
            let start_hash = registry()
                .hash_of("r2.compiler.build.start")
                .unwrap();
            Box::leak(Box::new([start_hash])) as &[u32]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(name: &str, payload: &[u8]) -> (u32, Vec<u8>) {
        (r2_fnv::fnv1a_32(name.as_bytes()), payload.to_vec())
    }

    #[test]
    fn ignores_unrelated_events() {
        let mut b = BuilderSentant::new();
        let mut actions = ActionBuf::new();
        let (h, p) = make_event("r2.compiler.unrelated", b"{}");
        let ev = Event {
            hash: h,
            payload: &p,
            source: r2_engine::EventSource::Local(0),
            msg_id: 0,
        };
        b.handle_event(&ev, &mut actions);
        assert!(actions.is_empty());
    }

    #[test]
    fn emits_three_progress_plus_done_on_build_start() {
        let mut b = BuilderSentant::new();
        let mut actions = ActionBuf::new();
        let payload = b"{\"score\":\"rocker-sensor.yaml\",\"target\":\"esp32-c6-dfr1117\"}";
        let h = r2_fnv::fnv1a_32(b"r2.compiler.build.start");
        let ev = Event {
            hash: h,
            payload,
            source: r2_engine::EventSource::Local(0),
            msg_id: 0,
        };
        b.handle_event(&ev, &mut actions);
        // 3 progress + 1 done = 4 actions
        let collected: Vec<_> = actions.drain().collect();
        assert_eq!(collected.len(), 4);

        let progress_hash = r2_fnv::fnv1a_32(b"r2.compiler.build.progress");
        let done_hash = r2_fnv::fnv1a_32(b"r2.compiler.build.done");

        let mut prog_count = 0;
        let mut done_count = 0;
        for a in collected {
            if let Action::Send { event_hash, .. } = a {
                if event_hash == progress_hash {
                    prog_count += 1;
                } else if event_hash == done_hash {
                    done_count += 1;
                }
            }
        }
        assert_eq!(prog_count, 3);
        assert_eq!(done_count, 1);
    }
}
