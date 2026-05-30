//! Bridge between the async axum layer and the synchronous `r2-engine`
//! EventBus running on a dedicated OS thread.
//!
//! Conversion: the browser sends/receives JSON envelopes; the engine
//! works with `QueuedEvent` (FNV-hashed name + opaque payload bytes).
//!
//! ## JSON envelope (browser-side wire format)
//!
//! ```json
//! { "kind": "event", "name": "r2.compiler.build.start", "payload": "..." }
//! { "kind": "event", "name": "r2.compiler.build.progress", "payload": "{\"phase\":\"compiling\"}" }
//! ```
//!
//! `name` is the canonical R2 event name; the orchestrator FNV-hashes
//! it. `payload` is opaque to the bridge — the consuming sentant
//! interprets it. For Phase 1.7a we treat the payload as a JSON-encoded
//! UTF-8 string; Phase 1.7+ moves to canonical CBOR per R2-CBOR.

use std::collections::HashMap;
use std::sync::OnceLock;

use r2_engine::queue::QueuedEvent;
use serde::{Deserialize, Serialize};

/// A JSON envelope as exchanged on the `/r2` WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum WireEnvelope {
    /// An R2 event traversing the bus.
    #[serde(rename = "event")]
    Event {
        /// Canonical event name (e.g. `"r2.compiler.build.start"`).
        name: String,
        /// Opaque payload (Phase 1.7a: JSON-encoded UTF-8 string;
        /// Phase 1.7+: canonical CBOR encoded as base64).
        #[serde(default)]
        payload: serde_json::Value,
    },
    /// Connection-handshake hello.
    #[serde(rename = "hello")]
    Hello {
        /// Where the message originated.
        from: String,
        /// Sender version.
        version: String,
        /// Optional human-readable note.
        #[serde(default)]
        note: Option<String>,
    },
    /// Stub ack while Phase 1.7a stands up.
    #[serde(rename = "ack")]
    Ack {
        /// What the orchestrator received.
        echo: String,
    },
}

/// Canonical event-name registry — maps name strings to their FNV hashes
/// in both directions so the bridge can translate both ways.
pub struct EventNames {
    by_name: HashMap<&'static str, u32>,
    by_hash: HashMap<u32, &'static str>,
}

impl EventNames {
    /// Build the registry from the known r2.compiler.* names.
    pub fn new() -> Self {
        let mut by_name = HashMap::new();
        let mut by_hash = HashMap::new();
        for &name in KNOWN_EVENTS {
            let hash = r2_fnv::fnv1a_32(name.as_bytes());
            by_name.insert(name, hash);
            by_hash.insert(hash, name);
        }
        Self { by_name, by_hash }
    }

    /// Look up the hash for a known name.
    pub fn hash_of(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied().or_else(|| {
            // Unknown name — still compute the hash so the bus can dispatch.
            // The reverse-lookup pane won't show a friendly name, only the hash.
            Some(r2_fnv::fnv1a_32(name.as_bytes()))
        })
    }

    /// Look up the name for a known hash. Returns `None` for hashes we
    /// haven't pre-registered (the bridge falls back to displaying the
    /// hex hash in that case).
    pub fn name_of(&self, hash: u32) -> Option<&'static str> {
        self.by_hash.get(&hash).copied()
    }
}

impl Default for EventNames {
    fn default() -> Self {
        Self::new()
    }
}

/// Global singleton so handlers across modules use the same registry.
pub fn registry() -> &'static EventNames {
    static REG: OnceLock<EventNames> = OnceLock::new();
    REG.get_or_init(EventNames::new)
}

/// Event names the orchestrator knows about in Phase 1.7a. Growing
/// this list when new events are introduced gives the webapp a
/// friendly-name display; the bus dispatches by hash regardless of
/// whether the name is in this list.
const KNOWN_EVENTS: &[&str] = &[
    // Apiary lifecycle (SPEC-APIARY-LAYOUT §7)
    "r2.compiler.apiary.list",
    "r2.compiler.apiary.entry",
    "r2.compiler.apiary.open",
    "r2.compiler.apiary.active",
    "r2.compiler.apiary.create",
    "r2.compiler.apiary.save",
    "r2.compiler.apiary.close",
    "r2.compiler.apiary.git.init",
    "r2.compiler.apiary.git.publish",

    // Build flow (SPEC-R2-COMPILER §4.3)
    "r2.compiler.build.start",
    "r2.compiler.build.progress",
    "r2.compiler.build.done",
    "r2.compiler.build.error",

    // Deploy flow (SPEC-R2-COMPILER §12.2)
    "r2.compiler.flash.devices",
    "r2.compiler.deploy.start",
    "r2.compiler.deploy.progress",
    "r2.compiler.deploy.done",
    "r2.compiler.deploy.error",

    // Author flow (SPEC-R2-COMPILER §4.4)
    "r2.compiler.author.start",
    "r2.compiler.author.prompt",
    "r2.compiler.author.reply",
    "r2.compiler.author.file_added",
    "r2.compiler.author.done",
    "r2.compiler.author.error",

    // TG management (SPEC-R2-COMPILER §11)
    "r2.compiler.tg.status",
    "r2.compiler.tg.list_members",
    "r2.compiler.tg.member",
    "r2.compiler.tg.revoke_device",
    "r2.compiler.tg.rotate_keyholder",
    "r2.compiler.tg.reset",
    "r2.compiler.tg.export_keyholder",
    "r2.compiler.tg.import_keyholder",

    // Catalogue
    "r2.compiler.catalogue.list",
    "r2.compiler.catalogue.entry",
    "r2.compiler.source.request",
    "r2.compiler.source.delivered",

    // Sync
    "r2.compiler.sync.start",
    "r2.compiler.sync.progress",
    "r2.compiler.sync.done",

    // Material collection + processing
    "r2.compiler.material.upload",
    "r2.compiler.material.link",
    "r2.compiler.material.find",
    "r2.compiler.material.found",
    "r2.compiler.material.list",
    "r2.compiler.material.item",
    "r2.compiler.material.process.start",
    "r2.compiler.material.process.progress",
    "r2.compiler.material.process.done",
    "r2.compiler.material.process.error",
];

/// Translate a JSON envelope received from the WebSocket into a `QueuedEvent`
/// for the bus. Source ID 0xFF marks "external" per `r2-engine` convention.
pub fn envelope_to_queued(env: &WireEnvelope) -> Option<QueuedEvent> {
    match env {
        WireEnvelope::Event { name, payload } => {
            let hash = registry().hash_of(name)?;
            // Payload as JSON-encoded UTF-8 bytes for v0.1. R2-CBOR migration
            // is a Phase 1.7+ task; the wire format change is internal to
            // the bridge.
            let payload_bytes = serde_json::to_vec(payload).unwrap_or_default();
            Some(QueuedEvent::new(hash, 0xFF, false, 0, &payload_bytes))
        }
        _ => None,
    }
}

/// Translate a `QueuedEvent` (from the bus's outbound queue) into a JSON
/// envelope for the WebSocket.
pub fn queued_to_envelope(q: &QueuedEvent) -> WireEnvelope {
    let name = registry()
        .name_of(q.hash)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("0x{:08x}", q.hash));
    let payload: serde_json::Value = serde_json::from_slice(q.payload())
        .unwrap_or(serde_json::Value::Null);
    WireEnvelope::Event { name, payload }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_events_round_trip_by_name() {
        let reg = registry();
        for &name in KNOWN_EVENTS {
            let h = reg.hash_of(name).expect("known name has a hash");
            assert_eq!(reg.name_of(h), Some(name));
        }
    }

    #[test]
    fn unknown_event_name_still_hashes() {
        let reg = registry();
        let h = reg.hash_of("r2.compiler.unknown.future-event").unwrap();
        assert_eq!(h, r2_fnv::fnv1a_32(b"r2.compiler.unknown.future-event"));
        // But the reverse lookup returns None for unknown hashes.
        assert!(reg.name_of(h).is_none());
    }

    #[test]
    fn envelope_round_trip() {
        let env = WireEnvelope::Event {
            name: "r2.compiler.build.start".into(),
            payload: serde_json::json!({"score": "rocker-sensor.yaml", "target": "esp32-c6-dfr1117"}),
        };
        let q = envelope_to_queued(&env).expect("known name converts");
        let back = queued_to_envelope(&q);
        match back {
            WireEnvelope::Event { name, payload } => {
                assert_eq!(name, "r2.compiler.build.start");
                assert_eq!(payload["target"], "esp32-c6-dfr1117");
            }
            _ => panic!("unexpected kind"),
        }
    }
}
