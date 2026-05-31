//! `Author` sentant — handles the chat-driven authoring flow.
//!
//! Phase 1.7d. On `r2.compiler.author.prompt`, constructs a brief from
//! the payload (user message + canvas state + chat history) and
//! dispatches it to the `claude-code` plugin configured for the author
//! event-name set. The plugin's `poll()` emits reply / done / error
//! events; this sentant re-broadcasts the plugin-sourced ones so the
//! WS layer's outbound queue picks them up and forwards them to the
//! browser's chat pane.
//!
//! Same shape as [`super::BuilderSentant`] — thin FSM router
//! (per [[feedback-sentants-vs-plugins-terminology]]). The imperative
//! work happens in the plugin.
//!
//! Brief construction for v0.1 is intentionally minimal: just the
//! canvas slugs + chat history + the new user message. The spec-section
//! splicing called for in SPEC-CATALOGUE-LAYOUT §7.1 is Phase 1.8 work —
//! it requires distinguishing author intent (board / ensemble / plugin /
//! sentant) from the prompt itself, which is design work the user
//! hasn't been asked to do yet.

use r2_engine::action::PayloadBuf;
use r2_engine::plugin::PluginId;
use r2_engine::{Action, ActionBuf, Event, EventSource, Sentant, StateId, Target};

use crate::bridge::registry;
use crate::plugins::claude_code::{self, BriefSlot};

/// Idle → Working → Idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum State {
    Idle = 0,
    Working = 1,
}

pub struct AuthorSentant {
    state: State,
    author_plugin_id: PluginId,
    /// Brief delivery slot shared with the `claude-code` plugin
    /// instance. Bus payload is capped at 256 bytes; real briefs are
    /// kilobytes, so the brief goes through this slot instead.
    brief_slot: BriefSlot,
    prompt_hash: u32,
    reply_hash: u32,
    done_hash: u32,
    error_hash: u32,
}

impl AuthorSentant {
    pub fn new(author_plugin_id: PluginId, brief_slot: BriefSlot) -> Self {
        let reg = registry();
        Self {
            state: State::Idle,
            author_plugin_id,
            brief_slot,
            prompt_hash: reg.hash_of("r2.compiler.author.prompt").unwrap(),
            reply_hash:  reg.hash_of("r2.compiler.author.reply").unwrap(),
            done_hash:   reg.hash_of("r2.compiler.author.done").unwrap(),
            error_hash:  reg.hash_of("r2.compiler.author.error").unwrap(),
        }
    }
}

impl Sentant for AuthorSentant {
    fn handle_event(&mut self, event: &Event, actions: &mut ActionBuf) {
        // r2.compiler.author.prompt  →  build brief + dispatch to claude-code.
        // Brief goes into the side-channel slot (bus payload caps at 256B,
        // way too small); the PluginCall data is just a trigger.
        if event.hash == self.prompt_hash {
            self.state = State::Working;
            let brief = construct_brief(event.payload);
            *self.brief_slot.lock().unwrap() = Some(brief);
            actions.push(Action::PluginCall {
                plugin_id: self.author_plugin_id,
                command: claude_code::CMD_START,
                data: PayloadBuf::empty(),
            });
            return;
        }

        // Plugin-sourced reply / done / error → re-broadcast so the WS
        // outbound queue picks them up. Same guard as BuilderSentant:
        // only forward Plugin-sourced events to avoid the loop.
        let is_plugin_source = matches!(event.source, EventSource::Plugin(_));
        if !is_plugin_source {
            return;
        }

        if event.hash == self.reply_hash {
            actions.push(Action::Send {
                target: Target::Broadcast,
                event_hash: self.reply_hash,
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
        r2_fnv::fnv1a_32(b"ai.reality2.r2-compiler.author")
    }

    fn name(&self) -> &str {
        "Author"
    }

    fn subscriptions(&self) -> &[u32] {
        use std::sync::OnceLock;
        static SUBS: OnceLock<&'static [u32]> = OnceLock::new();
        SUBS.get_or_init(|| {
            let reg = registry();
            let subs = vec![
                reg.hash_of("r2.compiler.author.prompt").unwrap(),
                reg.hash_of("r2.compiler.author.reply").unwrap(),
                reg.hash_of("r2.compiler.author.done").unwrap(),
                reg.hash_of("r2.compiler.author.error").unwrap(),
            ];
            Box::leak(subs.into_boxed_slice())
        })
    }
}

/// Build the prompt that goes to `claude -p`'s stdin. v0.1: lean
/// context-priming + chat history + the new user message. Phase 1.8
/// will replace this with a Tera template that splices in the relevant
/// SPEC-CATALOGUE-LAYOUT section per §7.1.
///
/// The webapp's `sendChat` sends:
///   {
///     "message": "<user text>",
///     "canvas":  {"board": "<slug>"|null, "ensemble": "<slug>"|null},
///     "history": [{"role": "user"|"assistant", "content": "..."}, ...]
///   }
fn construct_brief(payload: &[u8]) -> String {
    let v: serde_json::Value =
        serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);
    let message = v.get("message").and_then(|x| x.as_str()).unwrap_or("");
    let board = v
        .get("canvas")
        .and_then(|c| c.get("board"))
        .and_then(|x| x.as_str())
        .unwrap_or("(none selected)");
    let ensemble = v
        .get("canvas")
        .and_then(|c| c.get("ensemble"))
        .and_then(|x| x.as_str())
        .unwrap_or("(none selected)");
    let history = v.get("history").and_then(|h| h.as_array());

    let mut brief = String::new();
    brief.push_str(
        "You are Claude Code assisting an operator using r2-compiler — \
         a visual composer for Reality2 firmware. The operator drags a \
         carrier board and an ensemble onto a canvas; the canvas plus \
         their chat with you produces the build brief for the per-carrier \
         firmware crate.\n\n",
    );
    brief.push_str("Canvas state right now:\n");
    brief.push_str(&format!("  Board:    {board}\n"));
    brief.push_str(&format!("  Ensemble: {ensemble}\n\n"));

    if let Some(history_arr) = history {
        if !history_arr.is_empty() {
            brief.push_str("Prior conversation:\n\n");
            for turn in history_arr {
                let role = turn.get("role").and_then(|x| x.as_str()).unwrap_or("?");
                let content = turn.get("content").and_then(|x| x.as_str()).unwrap_or("");
                brief.push_str(&format!("--- {role} ---\n{content}\n\n"));
            }
        }
    }

    brief.push_str("--- operator now says ---\n");
    brief.push_str(message);
    brief.push('\n');
    brief
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(hash: u32, payload: &[u8], source: EventSource) -> Event<'_> {
        Event { hash, payload, source, msg_id: 0 }
    }

    #[test]
    fn author_prompt_dispatches_plugin_call() {
        let mut a = AuthorSentant::new(11, std::sync::Arc::new(std::sync::Mutex::new(None)));
        let mut actions = ActionBuf::new();
        let prompt_hash = r2_fnv::fnv1a_32(b"r2.compiler.author.prompt");
        let payload = br#"{"message":"hi","canvas":{"board":"x","ensemble":"y"},"history":[]}"#;
        a.handle_event(&ev(prompt_hash, payload, EventSource::Local(0)), &mut actions);

        let collected: Vec<_> = actions.drain().collect();
        assert_eq!(collected.len(), 1);
        match &collected[0] {
            Action::PluginCall { plugin_id, command, .. } => {
                assert_eq!(*plugin_id, 11);
                assert_eq!(*command, claude_code::CMD_START);
            }
            other => panic!("expected PluginCall, got {other:?}"),
        }
        assert_eq!(a.state, State::Working);
    }

    #[test]
    fn plugin_sourced_reply_rebroadcasts() {
        let mut a = AuthorSentant::new(11, std::sync::Arc::new(std::sync::Mutex::new(None)));
        let mut actions = ActionBuf::new();
        let h = r2_fnv::fnv1a_32(b"r2.compiler.author.reply");
        a.handle_event(&ev(h, b"{}", EventSource::Plugin(0)), &mut actions);
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
    fn locally_sourced_reply_does_not_rebroadcast() {
        let mut a = AuthorSentant::new(11, std::sync::Arc::new(std::sync::Mutex::new(None)));
        let mut actions = ActionBuf::new();
        let h = r2_fnv::fnv1a_32(b"r2.compiler.author.reply");
        a.handle_event(&ev(h, b"{}", EventSource::Local(0)), &mut actions);
        assert!(actions.is_empty(), "must not re-broadcast our own emissions");
    }

    #[test]
    fn brief_includes_canvas_and_history() {
        let payload = br#"{
            "message":"add an OTA plugin",
            "canvas":{"board":"esp32-c6-dfr1117","ensemble":"rocker-sensor"},
            "history":[{"role":"user","content":"hello"},{"role":"assistant","content":"hi back"}]
        }"#;
        let brief = construct_brief(payload);
        assert!(brief.contains("Board:    esp32-c6-dfr1117"));
        assert!(brief.contains("Ensemble: rocker-sensor"));
        assert!(brief.contains("--- user ---"));
        assert!(brief.contains("hello"));
        assert!(brief.contains("--- assistant ---"));
        assert!(brief.contains("hi back"));
        assert!(brief.contains("add an OTA plugin"));
    }

    #[test]
    fn brief_with_empty_canvas() {
        let payload = br#"{"message":"hi","canvas":{},"history":[]}"#;
        let brief = construct_brief(payload);
        assert!(brief.contains("Board:    (none selected)"));
        assert!(brief.contains("Ensemble: (none selected)"));
    }

    #[test]
    fn plugin_sourced_done_transitions_to_idle() {
        let mut a = AuthorSentant::new(11, std::sync::Arc::new(std::sync::Mutex::new(None)));
        a.state = State::Working;
        let mut actions = ActionBuf::new();
        let h = r2_fnv::fnv1a_32(b"r2.compiler.author.done");
        a.handle_event(&ev(h, b"{}", EventSource::Plugin(0)), &mut actions);
        assert_eq!(a.state, State::Idle);
    }
}
