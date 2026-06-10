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

use axum::Router;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use tower_http::services::{ServeDir, ServeFile};

/// A parsed `registrations.r2-web` block from an ensemble.yaml.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct R2WebRegistration {
    /// Path prefix the bundle mounts at (e.g. `/proof`). Normalised via
    /// [`Self::mount_prefix`].
    pub route_prefix: String,
    /// Bundle directory, relative to the ensemble dir (e.g. `./web/`).
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
}
