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
}
