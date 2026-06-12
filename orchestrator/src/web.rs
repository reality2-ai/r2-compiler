//! r2-web **host** capability (Phase 3 Part C(i)).
//!
//! The orchestrator is the *controller/host* for an ensemble's web UX: it
//! reads the ensemble's `registrations.r2-web` block, serves the declared
//! `static_bundle` at its `route_prefix`, and (in a later slice) exposes a
//! `/r2` WebSocket that bridges raw R2-WIRE frames to a browser **wasm-hive**.
//! Per the north-star, composer **hosts**, it does not own UX state — the UX
//! lives in the browser hive (workshop's two-hive recipe).
//!
//! This module is the first slice: the **registration model + parser** and the
//! **static-mount router builder**. The `/r2` frame channel + Ed25519 auth are
//! the next slice (they touch the design's open review questions).
//!
//! Built to R2-WEB v0.3 §3 (static serving) + the `registrations.r2-web`
//! shape from `notekeeper.ensemble.yaml` and workshop's own-hive recipe (which
//! adds `channels`/`blob_routes`/`diagnostics`); both shapes parse here.

use std::path::{Path, PathBuf};

use axum::extract::ws::{Message, WebSocket};
use axum::Router;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use r2_engine::queue::QueuedEvent;
use r2_trust::GroupHmac;
use r2_wire::{decode_extended, verify_extended};
use serde::Deserialize;
use tower_http::services::{ServeDir, ServeFile};

use crate::hive::EngineHandle;

/// A parsed `registrations.r2-web` block from an ensemble.yaml.
///
/// Field names follow the **canonical** registration model (core 375a83f /
/// r2-def `web_registration()`): `mount` (default `/`) + `bundle`. The earlier
/// notekeeper/workshop names `route_prefix` + `static_bundle` are accepted as
/// aliases during the transition.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct R2WebRegistration {
    /// Path prefix the bundle mounts at (canonical `mount`, default `/`; e.g.
    /// `/proof`). Normalised via [`Self::mount_prefix`].
    #[serde(rename = "mount", alias = "route_prefix", default = "default_mount")]
    pub route_prefix: String,
    /// Bundle directory, relative to the ensemble dir (canonical `bundle`;
    /// e.g. `./ui/`).
    #[serde(rename = "bundle", alias = "static_bundle")]
    pub static_bundle: String,
    /// Raw-R2-WIRE frame channels (workshop recipe). Empty for a
    /// notekeeper-style GraphQL registration.
    #[serde(default)]
    pub channels: Vec<Channel>,
    /// Engine events streamed to connected browsers.
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
    /// Optional GraphQL endpoint (notekeeper-style registration).
    #[serde(default)]
    pub graphql: Option<GraphQl>,
    /// Optional large-asset GET routes.
    #[serde(default)]
    pub blob_routes: Vec<String>,
    /// Whether to expose diagnostics routes.
    #[serde(default)]
    pub diagnostics: bool,
}

/// A raw-frame `/r2` channel binding (workshop recipe).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Channel {
    /// Channel name (e.g. `r2`).
    pub name: String,
    /// Sentant the browser's frames are delivered to.
    #[serde(default)]
    pub target_sentant: Option<String>,
    /// Max inbound frame size (bytes).
    #[serde(default = "default_max_frame_bytes")]
    pub max_frame_bytes: usize,
}

fn default_max_frame_bytes() -> usize {
    65536
}

/// Canonical default mount point when `mount` is omitted (R2-WEB / r2-def).
fn default_mount() -> String {
    "/".to_string()
}

/// An event → browser subscription.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Subscription {
    /// Subscription label (scenario-local).
    pub name: String,
    /// The R2 event name forwarded to subscribers.
    pub event: String,
}

/// Optional GraphQL endpoint config (notekeeper-style).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GraphQl {
    /// Schema path.
    pub schema: String,
    /// Resolvers identifier.
    #[serde(default)]
    pub resolvers: Option<String>,
}

impl R2WebRegistration {
    /// Mount prefix for `nest_service`: leading `/`, no trailing `/`
    /// (`/` itself stays `/`).
    pub fn mount_prefix(&self) -> String {
        normalise_prefix(&self.route_prefix)
    }
}

/// Normalise a route prefix: ensure exactly one leading `/`, strip a trailing
/// `/` (except the root). `""`/`"/"` → `"/"`.
pub fn normalise_prefix(raw: &str) -> String {
    let trimmed = raw.trim();
    let no_lead = trimmed.trim_start_matches('/');
    let no_trail = no_lead.trim_end_matches('/');
    if no_trail.is_empty() {
        "/".to_string()
    } else {
        format!("/{no_trail}")
    }
}

// ── Parsing ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct EnsembleFile {
    ensemble: EnsembleBlock,
}

#[derive(Deserialize)]
struct EnsembleBlock {
    #[serde(default)]
    registrations: Option<Registrations>,
}

#[derive(Deserialize)]
struct Registrations {
    #[serde(rename = "r2-web", default)]
    r2_web: Option<R2WebRegistration>,
}

/// Parse the `ensemble.registrations.r2-web` block from an ensemble.yaml.
/// Returns `None` if the file has no such block (a headless ensemble — R2-WEB
/// §9.2 — is valid and simply has no web UX).
pub fn parse_r2web(ensemble_yaml: &str) -> Option<R2WebRegistration> {
    let file: EnsembleFile = serde_yaml::from_str(ensemble_yaml).ok()?;
    file.ensemble.registrations?.r2_web
}

// ── Static-mount router builder ──────────────────────────────────────────

/// Build the static-serving route for one registration: the bundle is served
/// at `route_prefix` with SPA fallback — unmatched sub-paths return
/// `index.html` (R2-WEB §3.1). `ensemble_dir` is the directory the
/// registration's `static_bundle` is resolved against.
///
/// NOTE on the trailing-slash papercut (a bare `/proof` makes the browser
/// resolve relative asset URLs against `/proof`, not `/proof/`): axum can't
/// both `nest_service` at `/proof` *and* route a redirect at `/proof` (they
/// conflict — the nest owns the prefix). The correct fix is in the **bundle**
/// — its `index.html` MUST set `<base href="<route_prefix>/">` (or use
/// absolute asset paths) so assets resolve regardless of trailing slash. The
/// wasm-hive bundle (Part C ii) owns that.
pub fn registration_router(reg: &R2WebRegistration, ensemble_dir: &Path) -> Router {
    let bundle: PathBuf = ensemble_dir.join(reg.static_bundle.trim_start_matches("./"));
    let index = bundle.join("index.html");
    let serve = ServeDir::new(&bundle)
        .append_index_html_on_directories(true)
        .not_found_service(ServeFile::new(index));

    let prefix = reg.mount_prefix();
    if prefix == "/" {
        // axum 0.8 forbids nesting at root — serve the bundle as the fallback.
        Router::new().fallback_service(serve)
    } else {
        Router::new().nest_service(&prefix, serve)
    }
}

// ── /r2 WebSocket auth (R2-WEB v0.3 §4.2/§4.4) ───────────────────────────
//
// Roy's decision (Q1): per-message **Ed25519** auth from day one — NO
// trusted_local/anonymous mode (keeps us §10.2-conformant). Every /r2 message
// is a signed envelope; the browser wasm-hive must be a provisioned, non-revoked
// member of the apiary TrustGroup (its `device_id` IS its Ed25519 DEV_PK).

/// A signed WebSocket message envelope (R2-WEB §4.2).
#[derive(Debug, Clone, Deserialize)]
pub struct WsAuthEnvelope {
    /// 64-hex = 32-byte Ed25519 public key (DEV_PK).
    pub device_id: String,
    /// Unix seconds (decimal) — checked against the replay window.
    pub timestamp: i64,
    /// 128-hex = 64-byte Ed25519 signature.
    pub signature: String,
    /// The actual message as a JSON string (signed, not interpreted here).
    pub payload: String,
}

/// Why a WS message failed authentication. All causes → silently drop the
/// message (R2-WEB §4.2: never close the socket on a single failure).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    /// Envelope not valid JSON / signature not 64 bytes hex.
    Malformed,
    /// `device_id` is not 32-byte hex / not a valid Ed25519 point.
    BadDeviceId,
    /// `device_id` is not a provisioned, non-revoked TG member.
    NotMember,
    /// `timestamp` outside the replay window.
    Expired,
    /// Ed25519 signature did not verify against `device_id`.
    BadSignature,
}

/// Replay window (R2-WEB §4.2): timestamp must be within this of server time.
pub const REPLAY_WINDOW_SECS: i64 = 60;

/// Verify a signed `/r2` envelope per R2-WEB §4.2. On success returns the
/// authenticated `DEV_PK`. `is_live_member` answers "is this DEV_PK a
/// provisioned, non-revoked member of the active apiary TG?" (wired to
/// `substrate/tg_state` + the roster at the call site; abstracted here so the
/// crypto + framing logic is unit-testable in isolation).
///
/// The signed bytes are `"<device_id>:<timestamp>:<payload>"` (lowercase-hex
/// device_id, decimal timestamp, payload JSON string) — R2-WEB §4.2.
pub fn verify_ws_auth(
    raw: &str,
    now_unix: i64,
    replay_window_secs: i64,
    is_live_member: impl Fn(&[u8; 32]) -> bool,
) -> Result<[u8; 32], AuthError> {
    let env: WsAuthEnvelope = serde_json::from_str(raw).map_err(|_| AuthError::Malformed)?;

    let pk_bytes = decode_hex_32(&env.device_id).ok_or(AuthError::BadDeviceId)?;
    let vk = VerifyingKey::from_bytes(&pk_bytes).map_err(|_| AuthError::BadDeviceId)?;

    // Membership first: an unknown/revoked device must not even reach crypto.
    if !is_live_member(&pk_bytes) {
        return Err(AuthError::NotMember);
    }
    // Replay window.
    if (now_unix - env.timestamp).abs() > replay_window_secs {
        return Err(AuthError::Expired);
    }
    // Signature over device_id:timestamp:payload.
    let sig_bytes = decode_hex_64(&env.signature).ok_or(AuthError::Malformed)?;
    let signed = format!("{}:{}:{}", env.device_id, env.timestamp, env.payload);
    vk.verify(signed.as_bytes(), &Signature::from_bytes(&sig_bytes))
        .map_err(|_| AuthError::BadSignature)?;
    Ok(pk_bytes)
}

/// Membership predicate for [`verify_ws_auth`], backed by the active apiary's
/// roster: a `DEV_PK` is a **live member** iff a roster row has
/// `device_pk == hex(pk)`, `cert_status == "valid"`, and a `state` that is
/// neither `revoked` nor `retired`. This is the "provisioned, non-revoked
/// apiary-TG member" check R2-WEB §4.2 requires (the browser wasm-hive must be
/// enrolled — slice 2b's enrolment path adds it as such a row).
pub fn roster_is_live_member(apiary_dir: &std::path::Path, pk: &[u8; 32]) -> bool {
    let want = hex::encode(pk);
    crate::roster::load(apiary_dir).devices.iter().any(|d| {
        d.device_pk.as_deref() == Some(want.as_str())
            && d.cert_status == "valid"
            && d.state != "revoked"
            && d.state != "retired"
    })
}

fn decode_hex_32(s: &str) -> Option<[u8; 32]> {
    let v = hex::decode(s).ok()?;
    (v.len() == 32).then(|| {
        let mut a = [0u8; 32];
        a.copy_from_slice(&v);
        a
    })
}

fn decode_hex_64(s: &str) -> Option<[u8; 64]> {
    let v = hex::decode(s).ok()?;
    (v.len() == 64).then(|| {
        let mut a = [0u8; 64];
        a.copy_from_slice(&v);
        a
    })
}

// ── /r2/wire raw-R2-WIRE node channel (R2-WEB §4.6, CONFORMANT-PENDING-SPEC) ──
//
// The wasm-hive's primary channel: opaque R2-WIRE frames in/out (workshop
// two-hive recipe). This is the channel **routing logic** — frame-size gate,
// subscription matching, inbound target. It is deliberately auth-agnostic:
// the LIVE axum `/r2/wire` route is GATED on native R2-WIRE/R2-TRUST frame auth
// (core territory; depends on r2-wire/r2-trust frame verification) so we never
// expose an unauthenticated channel (R2-WEB §10.2). The async WS glue to the
// engine bus + the frame codec/auth attach when core's frame-auth API and
// R2-WEB §4.6 ("Raw R2-WIRE frame channel") land.

/// What to do with an inbound browser frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundAction {
    /// Forward the frame to the channel's target sentant on the engine bus.
    Inject {
        /// The channel's `target_sentant` (None = broadcast to subscribers).
        target: Option<String>,
        /// The opaque frame bytes.
        frame: Vec<u8>,
    },
    /// Frame exceeded `max_frame_bytes` — dropped.
    RejectOversize {
        /// Received length.
        len: usize,
        /// Configured cap.
        max: usize,
    },
}

/// Routing logic for one raw-R2-WIRE `/r2/wire` channel binding (from a
/// registration's `channels[]` + its `subscriptions[]`).
pub struct WireChannel {
    name: String,
    target_sentant: Option<String>,
    max_frame_bytes: usize,
    /// FNV-1a-32 hashes of the subscribed event names (the canonical R2 event
    /// hash — works for any name, not just composer's registry set).
    subscribed_hashes: Vec<u32>,
}

impl WireChannel {
    /// Build from a registration `Channel` + the registration's subscriptions.
    pub fn new(ch: &Channel, subscriptions: &[Subscription]) -> Self {
        Self {
            name: ch.name.clone(),
            target_sentant: ch.target_sentant.clone(),
            max_frame_bytes: ch.max_frame_bytes,
            subscribed_hashes: subscriptions
                .iter()
                .map(|s| r2_fnv::fnv1a_32(s.event.as_bytes()))
                .collect(),
        }
    }

    /// Channel name (e.g. `r2`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Decide what to do with an inbound browser frame: size-gate, then inject
    /// to the target sentant.
    pub fn on_inbound(&self, frame: &[u8]) -> InboundAction {
        if frame.len() > self.max_frame_bytes {
            InboundAction::RejectOversize { len: frame.len(), max: self.max_frame_bytes }
        } else {
            InboundAction::Inject { target: self.target_sentant.clone(), frame: frame.to_vec() }
        }
    }

    /// If this channel subscribes to `event_hash`, the (opaque) payload to push
    /// to the browser; else `None`. Wrapping into a proper R2-WIRE frame is the
    /// codec step attached with §4.6.
    pub fn outbound_for(&self, event_hash: u32, payload: &[u8]) -> Option<Vec<u8>> {
        self.subscribed_hashes
            .contains(&event_hash)
            .then(|| payload.to_vec())
    }

    /// Inbound frame-size cap (bytes).
    pub fn max_frame_bytes(&self) -> usize {
        self.max_frame_bytes
    }
}

/// Encode + group-HMAC-sign an outbound R2-WIRE **extended** frame for the
/// `/r2/wire` channel (the inverse of [`verify_wire_frame`]). Returns the
/// frame bytes, or `None` if encoding fails.
pub fn encode_signed_wire_frame(event_hash: u32, payload: &[u8], hmac: &GroupHmac) -> Option<Vec<u8>> {
    use r2_wire::types::{ExtendedHeader, ExtendedMessage};
    use r2_wire::{encode_extended, sign_extended, Flags, MsgType};

    let mut msg = ExtendedMessage {
        header: ExtendedHeader {
            version: 0,
            msg_type: MsgType::Event,
            flags: Flags::default(),
            ttl: 1,
            k: 0,
            msg_id: 0,
            event_hash,
            payload_len: payload.len() as u32,
            target_group: 0,
            target_hive: 0,
        },
        route: None,
        payload,
        hmac_tag: None,
    };
    let (flags, tag) = sign_extended(&msg, hmac);
    msg.header.flags = flags;
    msg.hmac_tag = Some(tag);
    let mut buf = vec![0u8; payload.len() + 64];
    let len = encode_extended(&msg, &mut buf).ok()?;
    buf.truncate(len);
    Some(buf)
}

/// Derive the active apiary TG's group-HMAC from the off-tree TG signing key
/// (`<config_root>/apiaries/<name>/tg_signer/tg_priv.bin` — the keyholder
/// substrate's key path). `None` if no key is present (→ the `/r2/wire` route
/// refuses connections rather than bypass §10.2). The HK is volatile — never
/// persisted or logged (R2-TRUST §3.3).
pub fn derive_apiary_group_hmac(apiary_dir: &Path, config_root: &Path) -> Option<GroupHmac> {
    let apiary_name = apiary_dir.file_name()?.to_str()?;
    let priv_path = config_root
        .join("apiaries")
        .join(apiary_name)
        .join("tg_signer/tg_priv.bin");
    let seed = std::fs::read(&priv_path).ok()?;
    if seed.len() != 32 {
        return None;
    }
    let mut s = [0u8; 32];
    s.copy_from_slice(&seed);
    let sk = ed25519_dalek::SigningKey::from_bytes(&s);
    let keys = r2_trust::derive_group_keys(&sk).ok()?;
    Some(GroupHmac::new(keys.hk))
}

/// The `/r2/wire` raw-R2-WIRE WebSocket loop (R2-WEB §4.6,
/// CONFORMANT-PENDING-SPEC). Bridges the browser wasm-hive ↔ the engine bus:
/// - **inbound:** each binary frame is size-gated, then `verify_wire_frame`d
///   against the apiary group-HMAC; verified frames are injected into the
///   engine as a `QueuedEvent` (source `0xFF` = external). Bad-auth frames are
///   dropped silently (R2-WEB §4.2 — never close on one bad frame).
/// - **outbound:** engine events whose hash the channel subscribes to are
///   encoded + signed and pushed as binary frames.
///
/// (Mesh leg — forwarding to/from the wider TCP mesh — attaches to core's D3a
/// transport later; this is the in-process engine-bus path.)
pub async fn wire_socket_loop(
    mut socket: WebSocket,
    engine: EngineHandle,
    channel: WireChannel,
    hmac: GroupHmac,
) {
    let mut outbound_rx = engine.subscribe_outbound();
    let max = channel.max_frame_bytes();
    loop {
        tokio::select! {
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Binary(bytes))) => {
                        if bytes.len() > max {
                            continue; // oversize — drop
                        }
                        if let Ok(f) = verify_wire_frame(&bytes, &hmac) {
                            let q = QueuedEvent::new(f.event_hash, 0xFF, false, 0, &f.payload);
                            let _ = engine.inbound_tx.try_send(q);
                        }
                        // unverified frames are dropped silently (§4.2)
                    }
                    Some(Ok(Message::Ping(p))) => {
                        if socket.send(Message::Pong(p)).await.is_err() { break; }
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    _ => {}
                }
            }
            engine_msg = outbound_rx.recv() => {
                if let Ok(q) = engine_msg {
                    if let Some(payload) = channel.outbound_for(q.hash, q.payload()) {
                        if let Some(frame) = encode_signed_wire_frame(q.hash, &payload, &hmac) {
                            if socket.send(Message::Binary(frame.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A verified inbound R2-WIRE frame on the `/r2/wire` channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireFrame {
    /// FNV-1a-32 event hash from the frame header.
    pub event_hash: u32,
    /// Decoded payload bytes.
    pub payload: Vec<u8>,
}

/// Why a `/r2/wire` frame failed native R2-WIRE/R2-TRUST authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameAuthError {
    /// Bytes did not decode as a compact R2-WIRE message.
    Malformed,
    /// HMAC tag absent or did not verify against the group key.
    BadHmac,
}

/// Verify + decode one raw R2-WIRE **extended** frame against the apiary
/// TrustGroup's group-HMAC (the `/r2/wire` channel's native frame auth —
/// R2-WEB §4.6, CONFORMANT-PENDING-SPEC). The WS/TCP path carries the
/// **extended** format (32-byte tag); the compact 8-byte-tag path is only for
/// un-transcoded BLE/LoRa frames and is not handled here.
///
/// This **consumes** `r2-wire` (`decode_extended`/`verify_extended`) +
/// `r2-trust` (`GroupHmac`) — it does **not** reimplement frame crypto.
/// `verify_extended` recomputes the §10.2 authenticated envelope
/// (`type ‖ event_hash ‖ target ‖ payload`; TTL/K/msg_id/route are mutable and
/// deliberately NOT authenticated) and compares the tag.
///
/// The `hmac` is built at the call site (the WS handler) from the active apiary
/// TG's group-HMAC key — derive via `r2_trust::derive_group_keys(&tg_signing_key)
/// .hk` (the keyholder substrate holds the TG Ed25519 key); keep HK volatile,
/// never persist (R2-TRUST §3.3). composer is an **endpoint/host** on
/// `/r2/wire`, so verifying here is correct (pure relays forward opaquely);
/// an unauthenticated frame can never be injected (R2-WEB §10.2).
pub fn verify_wire_frame(frame: &[u8], hmac: &GroupHmac) -> Result<WireFrame, FrameAuthError> {
    let msg = decode_extended(frame).map_err(|_| FrameAuthError::Malformed)?;
    if !verify_extended(&msg, hmac) {
        return Err(FrameAuthError::BadHmac);
    }
    Ok(WireFrame {
        event_hash: msg.header.event_hash,
        payload: msg.payload.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The registrations.r2-web block shape from notekeeper.ensemble.yaml
    // (crates/r2-def/vectors/), embedded so the test is self-contained.
    const NOTEKEEPER: &str = r#"
ensemble:
  name: notekeeper
  class: ai.reality2.ensemble.notekeeper
  sentants:
    - name: Note
  registrations:
    r2-web:
      route_prefix: /notekeeper
      static_bundle: ./web/
      graphql:
        schema: ./web/schema.graphql
        resolvers: notekeeper.GraphqlResolvers
      subscriptions:
        - name: noteStream
          event: note.changed
  compile_target: [linux]
"#;

    // A workshop-recipe-style registration (channels + diagnostics).
    const PROOF: &str = r#"
ensemble:
  name: test-ux
  registrations:
    r2-web:
      route_prefix: /proof
      static_bundle: ./web/
      channels:
        - { name: r2, target_sentant: TestCoordinator, max_frame_bytes: 65536 }
      subscriptions:
        - { name: delivery, event: r2.tn.delivered }
      diagnostics: true
"#;

    const HEADLESS: &str = r#"
ensemble:
  name: sensor
  sentants:
    - name: sensor-reader
"#;

    #[test]
    fn parses_notekeeper_graphql_registration() {
        let r = parse_r2web(NOTEKEEPER).expect("has r2-web");
        assert_eq!(r.route_prefix, "/notekeeper");
        assert_eq!(r.static_bundle, "./web/");
        assert_eq!(r.subscriptions.len(), 1);
        assert_eq!(r.subscriptions[0].event, "note.changed");
        assert!(r.channels.is_empty());
        assert_eq!(r.graphql.as_ref().unwrap().schema, "./web/schema.graphql");
    }

    #[test]
    fn parses_workshop_channels_registration() {
        let r = parse_r2web(PROOF).expect("has r2-web");
        assert_eq!(r.route_prefix, "/proof");
        assert_eq!(r.channels.len(), 1);
        assert_eq!(r.channels[0].name, "r2");
        assert_eq!(r.channels[0].target_sentant.as_deref(), Some("TestCoordinator"));
        assert_eq!(r.channels[0].max_frame_bytes, 65536);
        assert!(r.diagnostics);
        assert!(r.graphql.is_none());
    }

    #[test]
    fn channel_max_frame_defaults() {
        let r = parse_r2web(
            "ensemble:\n  registrations:\n    r2-web:\n      route_prefix: /x\n      static_bundle: ./w\n      channels:\n        - { name: r2 }\n",
        )
        .unwrap();
        assert_eq!(r.channels[0].max_frame_bytes, default_max_frame_bytes());
        assert_eq!(r.channels[0].target_sentant, None);
    }

    #[test]
    fn parses_canonical_mount_bundle_shape() {
        // The canonical r2-def shape: `mount` (default '/') + `bundle`.
        let canon = "ensemble:\n  registrations:\n    r2-web:\n      mount: /dash\n      bundle: ./ui/\n";
        let r = parse_r2web(canon).unwrap();
        assert_eq!(r.route_prefix, "/dash");
        assert_eq!(r.static_bundle, "./ui/");
        // mount defaults to '/' when omitted
        let dflt = "ensemble:\n  registrations:\n    r2-web:\n      bundle: ./ui/\n";
        assert_eq!(parse_r2web(dflt).unwrap().route_prefix, "/");
    }

    #[test]
    fn headless_ensemble_has_no_registration() {
        assert!(parse_r2web(HEADLESS).is_none());
    }

    #[test]
    fn garbage_yaml_is_none_not_panic() {
        assert!(parse_r2web(").:\n  not yaml at all\n::").is_none());
        assert!(parse_r2web("").is_none());
    }

    #[test]
    fn prefix_normalisation() {
        assert_eq!(normalise_prefix("/proof"), "/proof");
        assert_eq!(normalise_prefix("proof"), "/proof");
        assert_eq!(normalise_prefix("/proof/"), "/proof");
        assert_eq!(normalise_prefix("///proof///"), "/proof");
        assert_eq!(normalise_prefix("/"), "/");
        assert_eq!(normalise_prefix(""), "/");
        assert_eq!(normalise_prefix("  /a/b/  "), "/a/b");
    }

    #[test]
    fn registration_router_builds() {
        // Smoke: the builder produces a Router without panicking for both
        // a sub-path prefix (redirect added) and the root prefix.
        let r = parse_r2web(PROOF).unwrap();
        let _ = registration_router(&r, Path::new("/tmp/ensemble"));
        let mut root = r.clone();
        root.route_prefix = "/".into();
        let _ = registration_router(&root, Path::new("/tmp/ensemble"));
    }

    #[test]
    fn parses_the_real_transient_test_ensemble() {
        // The actual proof-surface ensemble (not a fixture) must parse +
        // produce a usable WireChannel — regression-guards slice 1 + 2b-ii
        // against the committed catalogue artifact.
        const ENSEMBLE: &str =
            include_str!("../../catalogue/ensembles/transient-test/ensemble.yaml");
        let reg = parse_r2web(ENSEMBLE).expect("transient-test has registrations.r2-web");
        assert_eq!(reg.route_prefix, "/proof"); // canonical `mount`
        assert_eq!(reg.static_bundle, "./ui/"); // canonical `bundle`
        assert!(reg.diagnostics);
        assert_eq!(reg.channels.len(), 1);
        assert_eq!(reg.channels[0].target_sentant.as_deref(), Some("TestCoordinator"));
        assert_eq!(reg.subscriptions.len(), 6);
        // and it drives a WireChannel
        let ch = WireChannel::new(&reg.channels[0], &reg.subscriptions);
        assert_eq!(ch.name(), "r2");
        let delivered = r2_fnv::fnv1a_32(b"r2.tn.delivered");
        assert_eq!(ch.outbound_for(delivered, b"x"), Some(b"x".to_vec()));
    }

    // ── /r2 Ed25519 auth ──────────────────────────────────────────────
    use ed25519_dalek::{Signer, SigningKey};

    fn test_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn signed_envelope(sk: &SigningKey, timestamp: i64, payload: &str) -> String {
        let device_id = hex::encode(sk.verifying_key().to_bytes());
        let signed = format!("{device_id}:{timestamp}:{payload}");
        let signature = hex::encode(sk.sign(signed.as_bytes()).to_bytes());
        serde_json::json!({
            "device_id": device_id,
            "timestamp": timestamp,
            "signature": signature,
            "payload": payload,
        })
        .to_string()
    }

    #[test]
    fn valid_signed_envelope_authenticates() {
        let sk = test_key(0x11);
        let pk = sk.verifying_key().to_bytes();
        let env = signed_envelope(&sk, 1_000_000, r#"{"type":"authenticate"}"#);
        let got = verify_ws_auth(&env, 1_000_000, REPLAY_WINDOW_SECS, |p| p == &pk).unwrap();
        assert_eq!(got, pk);
        // small clock skew within the window still passes
        assert!(verify_ws_auth(&env, 1_000_030, REPLAY_WINDOW_SECS, |p| p == &pk).is_ok());
    }

    #[test]
    fn expired_timestamp_rejected() {
        let sk = test_key(0x22);
        let pk = sk.verifying_key().to_bytes();
        let env = signed_envelope(&sk, 1_000_000, "{}");
        let r = verify_ws_auth(&env, 1_000_000 + 120, REPLAY_WINDOW_SECS, |p| p == &pk);
        assert_eq!(r, Err(AuthError::Expired));
    }

    #[test]
    fn tampered_payload_fails_signature() {
        let sk = test_key(0x33);
        let pk = sk.verifying_key().to_bytes();
        // Sign payload A, then swap in payload B keeping the signature.
        let device_id = hex::encode(pk);
        let signed = format!("{device_id}:1000000:A");
        let signature = hex::encode(sk.sign(signed.as_bytes()).to_bytes());
        let tampered = serde_json::json!({
            "device_id": device_id, "timestamp": 1_000_000,
            "signature": signature, "payload": "B",
        })
        .to_string();
        assert_eq!(
            verify_ws_auth(&tampered, 1_000_000, REPLAY_WINDOW_SECS, |p| p == &pk),
            Err(AuthError::BadSignature)
        );
    }

    #[test]
    fn non_member_rejected_before_crypto() {
        let sk = test_key(0x44);
        let env = signed_envelope(&sk, 1_000_000, "{}");
        // is_live_member says no → NotMember (even though the sig is valid).
        assert_eq!(
            verify_ws_auth(&env, 1_000_000, REPLAY_WINDOW_SECS, |_| false),
            Err(AuthError::NotMember)
        );
    }

    #[test]
    fn malformed_and_bad_device_id() {
        assert_eq!(
            verify_ws_auth("not json", 0, REPLAY_WINDOW_SECS, |_| true),
            Err(AuthError::Malformed)
        );
        // valid JSON envelope but device_id is not 32-byte hex
        let bad = serde_json::json!({
            "device_id": "xyz", "timestamp": 0, "signature": "00", "payload": "{}",
        })
        .to_string();
        assert_eq!(
            verify_ws_auth(&bad, 0, REPLAY_WINDOW_SECS, |_| true),
            Err(AuthError::BadDeviceId)
        );
    }

    #[test]
    fn wrong_key_signature_rejected() {
        // Envelope signed by a different key than its device_id claims.
        let real = test_key(0x55);
        let pk = real.verifying_key().to_bytes();
        let device_id = hex::encode(pk);
        let imposter = test_key(0x56);
        let signed = format!("{device_id}:1000000:{{}}");
        let signature = hex::encode(imposter.sign(signed.as_bytes()).to_bytes());
        let env = serde_json::json!({
            "device_id": device_id, "timestamp": 1_000_000,
            "signature": signature, "payload": "{}",
        })
        .to_string();
        assert_eq!(
            verify_ws_auth(&env, 1_000_000, REPLAY_WINDOW_SECS, |p| p == &pk),
            Err(AuthError::BadSignature)
        );
    }

    fn seed_roster(dir: &std::path::Path, rows: &[([u8; 32], &str, &str)]) {
        use crate::roster;
        let mut r = roster::Roster::default();
        for (pk, cert_status, state) in rows {
            let mut row = roster::new_placeholder(
                "sensor", "e", "esp32-s3-xiao", "a", "2026-01-01T00:00:00Z");
            row.device_pk = Some(hex::encode(pk));
            row.cert_status = (*cert_status).into();
            row.state = (*state).into();
            r.devices.push(row);
        }
        roster::save(dir, &r).unwrap();
    }

    #[test]
    fn roster_membership_gates_by_cert_and_state() {
        let dir = tempfile::tempdir().unwrap();
        let live = test_key(0x77).verifying_key().to_bytes();
        let revoked = test_key(0x78).verifying_key().to_bytes();
        let unknown = test_key(0x79).verifying_key().to_bytes();
        seed_roster(dir.path(), &[
            (live, "valid", "reachable"),
            (revoked, "revoked", "revoked"),
        ]);
        assert!(roster_is_live_member(dir.path(), &live));
        assert!(!roster_is_live_member(dir.path(), &revoked));
        assert!(!roster_is_live_member(dir.path(), &unknown));
    }

    #[test]
    fn verify_ws_auth_backed_by_roster() {
        let dir = tempfile::tempdir().unwrap();
        let sk = test_key(0x7A);
        let pk = sk.verifying_key().to_bytes();
        seed_roster(dir.path(), &[(pk, "valid", "reachable")]);
        let env = signed_envelope(&sk, 1_000_000, "{}");
        // Authenticates against real roster state.
        let got = verify_ws_auth(&env, 1_000_000, REPLAY_WINDOW_SECS, |p| {
            roster_is_live_member(dir.path(), p)
        })
        .unwrap();
        assert_eq!(got, pk);
        // A valid signature from a non-enrolled key is refused (NotMember).
        let stranger = test_key(0x7B);
        let env2 = signed_envelope(&stranger, 1_000_000, "{}");
        assert_eq!(
            verify_ws_auth(&env2, 1_000_000, REPLAY_WINDOW_SECS, |p| roster_is_live_member(dir.path(), p)),
            Err(AuthError::NotMember)
        );
    }

    // ── /r2/wire channel routing logic ────────────────────────────────

    fn proof_channel() -> WireChannel {
        let reg = parse_r2web(PROOF).unwrap();
        WireChannel::new(&reg.channels[0], &reg.subscriptions)
    }

    #[test]
    fn wire_channel_built_from_registration() {
        let ch = proof_channel();
        assert_eq!(ch.name(), "r2");
    }

    #[test]
    fn inbound_within_limit_injects_to_target() {
        let ch = proof_channel(); // target_sentant = TestCoordinator, max 65536
        match ch.on_inbound(&[0u8; 100]) {
            InboundAction::Inject { target, frame } => {
                assert_eq!(target.as_deref(), Some("TestCoordinator"));
                assert_eq!(frame.len(), 100);
            }
            other => panic!("expected Inject, got {other:?}"),
        }
    }

    #[test]
    fn inbound_oversize_rejected() {
        let ch = proof_channel();
        let big = vec![0u8; 65536 + 1];
        assert_eq!(
            ch.on_inbound(&big),
            InboundAction::RejectOversize { len: 65537, max: 65536 }
        );
    }

    #[test]
    fn outbound_only_for_subscribed_events() {
        let ch = proof_channel(); // subscribes to "r2.tn.delivered"
        let subscribed = r2_fnv::fnv1a_32(b"r2.tn.delivered");
        let other = r2_fnv::fnv1a_32(b"r2.tn.dropped");
        assert_eq!(ch.outbound_for(subscribed, b"payload"), Some(b"payload".to_vec()));
        assert_eq!(ch.outbound_for(other, b"payload"), None);
    }

    // ── /r2/wire native frame auth (consumes r2-wire + r2-trust) ──────
    use r2_wire::types::{ExtendedHeader, ExtendedMessage};
    use r2_wire::{encode_extended, sign_extended, Flags, MsgType};

    /// Build the bytes of a group-HMAC-signed EXTENDED R2-WIRE frame (the
    /// `/r2/wire` TCP format), mirroring r2-wire's own sign→encode flow.
    fn signed_wire_frame(hmac: &GroupHmac, event_hash: u32, payload: &[u8]) -> Vec<u8> {
        let mut msg = ExtendedMessage {
            header: ExtendedHeader {
                version: 0,
                msg_type: MsgType::Event,
                flags: Flags::default(),
                ttl: 1,
                k: 0,
                msg_id: 7,
                event_hash,
                payload_len: payload.len() as u32,
                target_group: 0,
                target_hive: 0,
            },
            route: None,
            payload,
            hmac_tag: None,
        };
        let (flags, tag) = sign_extended(&msg, hmac);
        msg.header.flags = flags;
        msg.hmac_tag = Some(tag);
        let mut buf = [0u8; 512];
        let len = encode_extended(&msg, &mut buf).unwrap();
        buf[..len].to_vec()
    }

    #[test]
    fn wire_frame_valid_verifies_and_decodes() {
        let hmac = GroupHmac::new([0x5A; 32]);
        let frame = signed_wire_frame(&hmac, 0xABCD_1234, b"hello");
        let got = verify_wire_frame(&frame, &hmac).unwrap();
        assert_eq!(got.event_hash, 0xABCD_1234);
        assert_eq!(got.payload, b"hello");
    }

    #[test]
    fn wire_frame_wrong_group_key_rejected() {
        let signer = GroupHmac::new([0x01; 32]);
        let frame = signed_wire_frame(&signer, 0x1, b"x");
        let other_group = GroupHmac::new([0x02; 32]);
        assert_eq!(verify_wire_frame(&frame, &other_group), Err(FrameAuthError::BadHmac));
    }

    #[test]
    fn wire_frame_unsigned_rejected() {
        // No HMAC tag (has_hmac=false) → must not pass (no §10.2 bypass).
        let hmac = GroupHmac::new([0x07; 32]);
        let msg = ExtendedMessage {
            header: ExtendedHeader {
                version: 0, msg_type: MsgType::Event, flags: Flags::default(),
                ttl: 1, k: 0, msg_id: 1, event_hash: 9, payload_len: 1,
                target_group: 0, target_hive: 0,
            },
            route: None,
            payload: b"x",
            hmac_tag: None,
        };
        let mut buf = [0u8; 128];
        let len = encode_extended(&msg, &mut buf).unwrap();
        assert_eq!(verify_wire_frame(&buf[..len], &hmac), Err(FrameAuthError::BadHmac));
    }

    #[test]
    fn wire_frame_malformed_rejected() {
        let hmac = GroupHmac::new([0u8; 32]);
        assert_eq!(verify_wire_frame(&[0xFF, 0xFF, 0xFF], &hmac), Err(FrameAuthError::Malformed));
        assert_eq!(verify_wire_frame(&[], &hmac), Err(FrameAuthError::Malformed));
    }

    #[test]
    fn outbound_encode_roundtrips_through_verify() {
        // encode_signed_wire_frame (orchestrator→browser) must produce a frame
        // that verify_wire_frame (browser→orchestrator) accepts under the same
        // group key, recovering the event_hash + payload.
        let hmac = GroupHmac::new([0xC3; 32]);
        let frame = encode_signed_wire_frame(0xFEED_BEEF, b"snapshot", &hmac).unwrap();
        let got = verify_wire_frame(&frame, &hmac).unwrap();
        assert_eq!(got.event_hash, 0xFEED_BEEF);
        assert_eq!(got.payload, b"snapshot");
        // a different group key rejects it
        assert_eq!(
            verify_wire_frame(&frame, &GroupHmac::new([0xC4; 32])),
            Err(FrameAuthError::BadHmac)
        );
    }
}
