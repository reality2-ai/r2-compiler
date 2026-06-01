//! `Builder` sentant — per-build state machine that drives a compile.
//!
//! **Phase 1.7b.** On `r2.composer.build.start`, dispatches to the
//! `claude-code` plugin (subprocess driver). The plugin's `poll()`
//! emits progress / done / error events as the subprocess runs; the
//! Builder sentant subscribes to those (only when sourced from a
//! Plugin) and re-broadcasts them so the WS layer's outbound queue
//! receives them.
//!
//! Per [[feedback-sentants-vs-plugins-terminology]] this sentant is a
//! thin FSM router — the actual subprocess + filesystem work happens
//! in the `claude-code` and (future) `cargo-runner` plugins.

use r2_engine::action::PayloadBuf;
use r2_engine::plugin::PluginId;
use r2_engine::{Action, ActionBuf, Event, EventSource, Sentant, StateId, Target};

use crate::bridge::registry;
use crate::composer::claude_code;

/// Idle → Working → Idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum State {
    Idle = 0,
    Working = 1,
}

/// Builder sentant — Phase 1.7b version.
pub struct BuilderSentant {
    state: State,
    claude_code_plugin_id: PluginId,
    start_hash: u32,
    progress_hash: u32,
    done_hash: u32,
    error_hash: u32,
}

impl BuilderSentant {
    /// Construct the sentant. `claude_code_plugin_id` is the ID assigned
    /// to the registered `claude-code` plugin at bus-registration time.
    pub fn new(claude_code_plugin_id: PluginId) -> Self {
        let reg = registry();
        Self {
            state: State::Idle,
            claude_code_plugin_id,
            start_hash:    reg.hash_of("r2.composer.build.start").unwrap(),
            progress_hash: reg.hash_of("r2.composer.build.progress").unwrap(),
            done_hash:     reg.hash_of("r2.composer.build.done").unwrap(),
            error_hash:    reg.hash_of("r2.composer.build.error").unwrap(),
        }
    }
}

impl Sentant for BuilderSentant {
    fn handle_event(&mut self, event: &Event, actions: &mut ActionBuf) {
        // r2.composer.build.start  →  dispatch to claude-code plugin
        if event.hash == self.start_hash {
            self.state = State::Working;
            // The brief that goes to claude's stdin is the inbound JSON
            // payload — for v0.1 we forward it as-is. Phase 1.8+ wraps
            // it in a Tera-rendered prompt template per
            // SPEC-R2-COMPOSER §5.
            actions.push(Action::PluginCall {
                plugin_id: self.claude_code_plugin_id,
                command: claude_code::CMD_START,
                data: PayloadBuf::from_slice(event.payload),
            });
            return;
        }

        // r2.composer.build.{progress,done,error}  →  re-broadcast
        // ONLY when sourced from a plugin (the claude-code plugin's
        // poll() output). This avoids the loop where our own broadcast
        // would re-trigger us.
        let is_plugin_source = matches!(event.source, EventSource::Plugin(_));
        if !is_plugin_source {
            return;
        }

        if event.hash == self.progress_hash {
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.progress_hash,
                payload: PayloadBuf::from_slice(event.payload),
            });
        } else if event.hash == self.done_hash {
            self.state = State::Idle;
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.done_hash,
                payload: PayloadBuf::from_slice(event.payload),
            });
        } else if event.hash == self.error_hash {
            self.state = State::Idle;
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.error_hash,
                payload: PayloadBuf::from_slice(event.payload),
            });
        }
    }

    fn state(&self) -> StateId {
        self.state as StateId
    }

    fn class_hash(&self) -> u32 {
        r2_fnv::fnv1a_32(b"ai.reality2.r2-composer.builder")
    }

    fn name(&self) -> &str {
        "Builder"
    }

    fn subscriptions(&self) -> &[u32] {
        use std::sync::OnceLock;
        static SUBS: OnceLock<&'static [u32]> = OnceLock::new();
        SUBS.get_or_init(|| {
            let reg = registry();
            let subs = vec![
                reg.hash_of("r2.composer.build.start").unwrap(),
                reg.hash_of("r2.composer.build.progress").unwrap(),
                reg.hash_of("r2.composer.build.done").unwrap(),
                reg.hash_of("r2.composer.build.error").unwrap(),
            ];
            Box::leak(subs.into_boxed_slice())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(hash: u32, payload: &[u8], source: EventSource) -> Event<'_> {
        Event { hash, payload, source, msg_id: 0 }
    }

    #[test]
    fn build_start_dispatches_plugin_call() {
        let mut b = BuilderSentant::new(7);
        let mut actions = ActionBuf::new();
        let start_hash = r2_fnv::fnv1a_32(b"r2.composer.build.start");
        let payload = br#"{"score":"x","target":"esp32-c6-dfr1117"}"#;
        b.handle_event(&ev(start_hash, payload, EventSource::Local(0)), &mut actions);

        let collected: Vec<_> = actions.drain().collect();
        assert_eq!(collected.len(), 1);
        match &collected[0] {
            Action::PluginCall { plugin_id, command, .. } => {
                assert_eq!(*plugin_id, 7);
                assert_eq!(*command, claude_code::CMD_START);
            }
            other => panic!("expected PluginCall, got {other:?}"),
        }
        assert_eq!(b.state, State::Working);
    }

    #[test]
    fn plugin_sourced_progress_rebroadcasts() {
        let mut b = BuilderSentant::new(7);
        let mut actions = ActionBuf::new();
        let h = r2_fnv::fnv1a_32(b"r2.composer.build.progress");
        b.handle_event(&ev(h, b"{}", EventSource::Plugin(0)), &mut actions);
        let collected: Vec<_> = actions.drain().collect();
        assert_eq!(collected.len(), 1);
        match &collected[0] {
            Action::Send { event_hash, target, .. } => {
                assert_eq!(*event_hash, h);
                assert!(matches!(target, Target::Broadcast));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn locally_sourced_progress_does_not_rebroadcast() {
        // Otherwise our own re-broadcast would re-trigger us.
        let mut b = BuilderSentant::new(7);
        let mut actions = ActionBuf::new();
        let h = r2_fnv::fnv1a_32(b"r2.composer.build.progress");
        b.handle_event(&ev(h, b"{}", EventSource::Local(0)), &mut actions);
        assert!(actions.is_empty(), "must not re-broadcast our own emissions");
    }

    #[test]
    fn plugin_sourced_done_transitions_to_idle() {
        let mut b = BuilderSentant::new(7);
        b.state = State::Working;
        let mut actions = ActionBuf::new();
        let h = r2_fnv::fnv1a_32(b"r2.composer.build.done");
        b.handle_event(&ev(h, b"{}", EventSource::Plugin(0)), &mut actions);
        assert_eq!(b.state, State::Idle);
    }

    #[test]
    fn plugin_sourced_error_transitions_to_idle() {
        let mut b = BuilderSentant::new(7);
        b.state = State::Working;
        let mut actions = ActionBuf::new();
        let h = r2_fnv::fnv1a_32(b"r2.composer.build.error");
        b.handle_event(&ev(h, b"{}", EventSource::Plugin(0)), &mut actions);
        assert_eq!(b.state, State::Idle);
    }

    #[test]
    fn ignores_unrelated_events() {
        let mut b = BuilderSentant::new(7);
        let mut actions = ActionBuf::new();
        let h = r2_fnv::fnv1a_32(b"r2.composer.unrelated");
        b.handle_event(&ev(h, b"{}", EventSource::Local(0)), &mut actions);
        assert!(actions.is_empty());
    }
}
