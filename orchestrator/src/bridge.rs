//! Bridge between the async axum layer and the synchronous `r2-engine`
//! EventBus running on a dedicated OS thread.
//!
//! Conversion: the browser sends/receives JSON envelopes; the engine
//! works with `QueuedEvent` (FNV-hashed name + opaque payload bytes).
//!
//! ## JSON envelope (browser-side wire format)
//!
//! ```json
//! { "kind": "event", "name": "r2.composer.build.start", "payload": "..." }
//! { "kind": "event", "name": "r2.composer.build.progress", "payload": "{\"phase\":\"compiling\"}" }
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
        /// Canonical event name (e.g. `"r2.composer.build.start"`).
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
    /// Build the registry from the known r2.composer.* names.
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
    "r2.composer.apiary.list",
    "r2.composer.apiary.entry",
    "r2.composer.apiary.open",
    "r2.composer.apiary.active",
    "r2.composer.apiary.create",
    "r2.composer.apiary.save",
    "r2.composer.apiary.close",
    "r2.composer.apiary.git.init",
    "r2.composer.apiary.git.publish",

    // Build flow (SPEC-R2-COMPOSER §4.3)
    "r2.composer.build.start",
    "r2.composer.build.progress",
    "r2.composer.build.done",
    "r2.composer.build.error",

    // Deploy flow (SPEC-R2-COMPOSER §12.2)
    "r2.composer.flash.devices",
    "r2.composer.deploy.start",
    "r2.composer.deploy.progress",
    "r2.composer.deploy.done",
    "r2.composer.deploy.error",

    // Device roster (SPEC-APIARY-FLASH §2, §8.1) — F1+
    "r2.composer.device.slot.create",
    "r2.composer.device.list",
    "r2.composer.device.entry",
    "r2.composer.device.transition",
    "r2.composer.device.unaccounted",
    "r2.composer.device.revoke",
    "r2.composer.device.retire",
    "r2.composer.device.purge",

    // USB watcher (SPEC-APIARY-FLASH §4.4) — F2+
    "r2.composer.usb.attached",
    "r2.composer.usb.detached",
    "r2.composer.usb.list",
    "r2.composer.usb.identify",

    // First-install (SPEC-APIARY-FLASH §4) — F2+
    "r2.composer.deploy.first_install.start",
    "r2.composer.deploy.first_install.progress",
    "r2.composer.deploy.first_install.done",
    "r2.composer.deploy.first_install.error",

    // Provision / WiFi (SPEC-APIARY-FLASH §5) — F3+
    "r2.composer.provision.network.upsert",
    "r2.composer.provision.network.upserted",
    "r2.composer.provision.networks.list",
    "r2.composer.provision.networks.listed",
    "r2.composer.provision.offer.start",
    "r2.composer.provision.offer.progress",
    "r2.composer.provision.offer.composed",
    "r2.composer.provision.cert_issued",
    "r2.composer.provision.cert_error",
    "r2.composer.provision.error",
    "r2.composer.device.identity_observed",
    "r2.composer.device.enrolled",

    // OTA batch (SPEC-APIARY-FLASH §6) — F5+
    "r2.composer.deploy.batch.start",
    "r2.composer.deploy.batch.done",
    "r2.composer.deploy.device.progress",
    "r2.composer.deploy.device.done",
    "r2.composer.deploy.device.error",

    // Author flow (SPEC-R2-COMPOSER §4.4)
    "r2.composer.author.start",
    "r2.composer.author.prompt",
    "r2.composer.author.reply",
    "r2.composer.author.file_added",
    "r2.composer.author.done",
    "r2.composer.author.error",

    // TG management (SPEC-R2-COMPOSER §11)
    "r2.composer.tg.status",
    "r2.composer.tg.list_members",
    "r2.composer.tg.member",
    "r2.composer.tg.revoke_device",
    "r2.composer.tg.rotate_keyholder",
    "r2.composer.tg.reset",
    "r2.composer.tg.export_keyholder",
    "r2.composer.tg.import_keyholder",

    // Catalogue
    "r2.composer.catalogue.list",
    "r2.composer.catalogue.entry",
    "r2.composer.source.request",
    "r2.composer.source.delivered",

    // Sync
    "r2.composer.sync.start",
    "r2.composer.sync.progress",
    "r2.composer.sync.done",

    // Material collection + processing
    "r2.composer.material.upload",
    "r2.composer.material.link",
    "r2.composer.material.find",
    "r2.composer.material.found",
    "r2.composer.material.list",
    "r2.composer.material.item",
    "r2.composer.material.process.start",
    "r2.composer.material.process.progress",
    "r2.composer.material.process.done",
    "r2.composer.material.process.error",
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
        let h = reg.hash_of("r2.composer.unknown.future-event").unwrap();
        assert_eq!(h, r2_fnv::fnv1a_32(b"r2.composer.unknown.future-event"));
        // But the reverse lookup returns None for unknown hashes.
        assert!(reg.name_of(h).is_none());
    }

    #[test]
    fn envelope_round_trip() {
        let env = WireEnvelope::Event {
            name: "r2.composer.build.start".into(),
            payload: serde_json::json!({"score": "rocker-sensor.yaml", "target": "esp32-c6-dfr1117"}),
        };
        let q = envelope_to_queued(&env).expect("known name converts");
        let back = queued_to_envelope(&q);
        match back {
            WireEnvelope::Event { name, payload } => {
                assert_eq!(name, "r2.composer.build.start");
                assert_eq!(payload["target"], "esp32-c6-dfr1117");
            }
            _ => panic!("unexpected kind"),
        }
    }
}
