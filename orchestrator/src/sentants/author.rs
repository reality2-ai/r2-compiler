//! `Author` sentant — handles the chat-driven authoring flow.
//!
//! Phase 1.7d. On `r2.composer.author.prompt`, constructs a brief from
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
use crate::composer::claude_code::{self, BriefSlot};

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
            prompt_hash: reg.hash_of("r2.composer.author.prompt").unwrap(),
            reply_hash:  reg.hash_of("r2.composer.author.reply").unwrap(),
            done_hash:   reg.hash_of("r2.composer.author.done").unwrap(),
            error_hash:  reg.hash_of("r2.composer.author.error").unwrap(),
        }
    }
}

impl Sentant for AuthorSentant {
    fn handle_event(&mut self, event: &Event, actions: &mut ActionBuf) {
        // r2.composer.author.prompt  →  build brief + dispatch to claude-code.
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
        r2_fnv::fnv1a_32(b"ai.reality2.r2-composer.author")
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
                reg.hash_of("r2.composer.author.prompt").unwrap(),
                reg.hash_of("r2.composer.author.reply").unwrap(),
                reg.hash_of("r2.composer.author.done").unwrap(),
                reg.hash_of("r2.composer.author.error").unwrap(),
            ];
            Box::leak(subs.into_boxed_slice())
        })
    }
}

/// Build the prompt that goes to `claude -p`'s stdin via Tera template
/// dispatch (per SPEC-CATALOGUE-LAYOUT §7.1 + SPEC-APIARY-CREATE §1.3).
/// Templates live under `orchestrator/prompts/` and are baked into the
/// binary at build time via `include_str!`.
///
/// Dispatch: the prompt payload carries an OPTIONAL `kind` field; we
/// pick the matching template, defaulting to the freeform `chat` case.
///
///   "kind": "chat"     →  author-chat.md.tera     (default — open chat)
///   "kind": "apiary"   →  author-apiary.md.tera   (new apiary)
///   "kind": "board"    →  author-board.md.tera    (new carrier board)
///   "kind": "ensemble" →  author-ensemble.md.tera (new ensemble)
///   "kind": "plugin"   →  author-plugin.md.tera   (new plugin)
///   "kind": "sentant"  →  author-sentant.md.tera  (new sentant)
///
/// The webapp's `sendChat` currently emits no `kind`, so chat behaviour
/// is preserved unchanged. Kind-specific dispatch is opt-in by the
/// webapp / AI tool-call channel — set `kind` in the payload to switch.
fn construct_brief(payload: &[u8]) -> String {
    let v: serde_json::Value =
        serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);
    let kind = v.get("kind").and_then(|x| x.as_str()).unwrap_or("chat");

    let template_name = match kind {
        "apiary"   => "author-apiary.md.tera",
        "board"    => "author-board.md.tera",
        "ensemble" => "author-ensemble.md.tera",
        "plugin"   => "author-plugin.md.tera",
        "sentant"  => "author-sentant.md.tera",
        "flash"    => "author-flash.md.tera",
        "provision" => "author-provision.md.tera",
        _          => "author-chat.md.tera",
    };

    let mut ctx = tera::Context::new();
    ctx.insert("kind", kind);
    ctx.insert("message", v.get("message").and_then(|x| x.as_str()).unwrap_or(""));
    let canvas = v.get("canvas").cloned().unwrap_or(serde_json::Value::Null);
    ctx.insert("canvas", &canvas);
    let history = v
        .get("history")
        .cloned()
        .unwrap_or(serde_json::Value::Array(Vec::new()));
    ctx.insert("history", &history);

    match template_engine().render(template_name, &ctx) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                "author brief: template '{template_name}' failed to render ({e}); \
                 falling back to minimal brief"
            );
            // Calm-degradation fallback if a template parse / render fails.
            format!(
                "You are Claude Code, assisting an operator using r2-composer.\n\
                 Operator says:\n\n{}\n",
                v.get("message").and_then(|x| x.as_str()).unwrap_or("")
            )
        }
    }
}

/// Lazy singleton — parses the six baked-in `.tera` templates once and
/// reuses them for every brief construction.
fn template_engine() -> &'static tera::Tera {
    use std::sync::OnceLock;
    static ENGINE: OnceLock<tera::Tera> = OnceLock::new();
    ENGINE.get_or_init(|| {
        // Baked into the binary at build time. The compiler embeds the
        // bytes directly from `orchestrator/prompts/*.md.tera`.
        const T_BASE:     &str = include_str!("../../prompts/_base.md.tera");
        const T_CHAT:     &str = include_str!("../../prompts/author-chat.md.tera");
        const T_APIARY:   &str = include_str!("../../prompts/author-apiary.md.tera");
        const T_BOARD:    &str = include_str!("../../prompts/author-board.md.tera");
        const T_ENSEMBLE: &str = include_str!("../../prompts/author-ensemble.md.tera");
        const T_PLUGIN:   &str = include_str!("../../prompts/author-plugin.md.tera");
        const T_SENTANT:  &str = include_str!("../../prompts/author-sentant.md.tera");
        const T_FLASH:    &str = include_str!("../../prompts/author-flash.md.tera");
        const T_PROVISION: &str = include_str!("../../prompts/author-provision.md.tera");

        let mut tera = tera::Tera::default();
        tera.add_raw_templates(vec![
            ("_base.md.tera",            T_BASE),
            ("author-chat.md.tera",      T_CHAT),
            ("author-apiary.md.tera",    T_APIARY),
            ("author-board.md.tera",     T_BOARD),
            ("author-ensemble.md.tera",  T_ENSEMBLE),
            ("author-plugin.md.tera",    T_PLUGIN),
            ("author-sentant.md.tera",   T_SENTANT),
            ("author-flash.md.tera",     T_FLASH),
            ("author-provision.md.tera", T_PROVISION),
        ])
        .expect("Tera template parse failed — fix the .tera files");
        tera
    })
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
        let prompt_hash = r2_fnv::fnv1a_32(b"r2.composer.author.prompt");
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
        let h = r2_fnv::fnv1a_32(b"r2.composer.author.reply");
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
        let h = r2_fnv::fnv1a_32(b"r2.composer.author.reply");
        a.handle_event(&ev(h, b"{}", EventSource::Local(0)), &mut actions);
        assert!(actions.is_empty(), "must not re-broadcast our own emissions");
    }

    #[test]
    fn brief_includes_canvas_and_history() {
        // Chat-mode brief (default — no `kind` field) must surface the
        // canvas state + chat history + the new user message.
        let payload = br#"{
            "message":"add an OTA plugin",
            "canvas":{"board":"esp32-c6-dfr1117","ensemble":"rocker-sensor"},
            "history":[{"role":"user","content":"hello"},{"role":"assistant","content":"hi back"}]
        }"#;
        let brief = construct_brief(payload);
        assert!(brief.contains("esp32-c6-dfr1117"), "board surfaces");
        assert!(brief.contains("rocker-sensor"),   "ensemble surfaces");
        assert!(brief.contains("hello"),           "user history line");
        assert!(brief.contains("hi back"),         "assistant history line");
        assert!(brief.contains("add an OTA plugin"), "current message");
        assert!(brief.contains("open chat"),       "chat-template marker");
    }

    #[test]
    fn brief_with_empty_canvas() {
        // Empty canvas object should still render — _base.md.tera's
        // `{%- if canvas.board ... %}` falls through to the
        // "(empty — no apiary open, no canvas selection)" branch.
        let payload = br#"{"message":"hi","canvas":{},"history":[]}"#;
        let brief = construct_brief(payload);
        assert!(brief.contains("empty"),                "empty-canvas marker");
        assert!(brief.contains("no apiary open"),       "empty-canvas marker 2");
    }

    #[test]
    fn kind_dispatch_apiary() {
        // kind: "apiary" picks the apiary template, which mentions
        // SPEC-APIARY-CREATE.md.
        let payload = br#"{"kind":"apiary","message":"new greenhouse rig","canvas":{},"history":[]}"#;
        let brief = construct_brief(payload);
        assert!(brief.contains("author a new APIARY"));
        assert!(brief.contains("SPEC-APIARY-CREATE.md"));
        assert!(brief.contains("new greenhouse rig"));
    }

    #[test]
    fn kind_dispatch_board() {
        let payload = br#"{"kind":"board","message":"add esp32-s3-xiao-sense","canvas":{},"history":[]}"#;
        let brief = construct_brief(payload);
        assert!(brief.contains("author a new CARRIER BOARD"));
        assert!(brief.contains("SPEC-CATALOGUE-LAYOUT.md"));
        assert!(brief.contains("add esp32-s3-xiao-sense"));
    }

    #[test]
    fn kind_dispatch_plugin() {
        let payload = br#"{"kind":"plugin","message":"i2c temperature sensor","canvas":{"ensemble":"rocker-sensor"},"history":[]}"#;
        let brief = construct_brief(payload);
        assert!(brief.contains("author a new PLUGIN"));
        assert!(brief.contains("R2-PLUGIN §12"));
        assert!(brief.contains("rocker-sensor"));
    }

    #[test]
    fn kind_dispatch_unknown_falls_back_to_chat() {
        // An unknown kind silently uses the chat template — never
        // refuse a brief.
        let payload = br#"{"kind":"future-thing","message":"hi","canvas":{},"history":[]}"#;
        let brief = construct_brief(payload);
        assert!(brief.contains("open chat"));
    }

    #[test]
    fn kind_dispatch_flash() {
        let payload = br#"{"kind":"flash","message":"flash esp32-s3-xiao on /dev/ttyACM0 as the kitchen sensor","canvas":{},"history":[]}"#;
        let brief = construct_brief(payload);
        assert!(brief.contains("FLASH a board"));
        assert!(brief.contains("SPEC-APIARY-FLASH.md"));
        assert!(brief.contains("device.slot.create"));
        assert!(brief.contains("deploy.first_install.start"));
        assert!(brief.contains("flash esp32-s3-xiao on /dev/ttyACM0"));
    }

    #[test]
    fn plugin_sourced_done_transitions_to_idle() {
        let mut a = AuthorSentant::new(11, std::sync::Arc::new(std::sync::Mutex::new(None)));
        a.state = State::Working;
        let mut actions = ActionBuf::new();
        let h = r2_fnv::fnv1a_32(b"r2.composer.author.done");
        a.handle_event(&ev(h, b"{}", EventSource::Plugin(0)), &mut actions);
        assert_eq!(a.state, State::Idle);
    }
}
