//! Plugin definition and reference types per R2-DEF §4 and §7.3.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::DefError;

/// Reference to a plugin declared elsewhere in the ensemble or hive
/// manifest. The sentant only names what plugins it uses; the
/// implementation lives in the manifest.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginRef {
    /// Plugin identifier (reverse-DNS RECOMMENDED).
    pub name: String,
}

/// Full plugin manifest entry as it appears in an ensemble's `plugins`
/// array (R2-DEF §7.3).
///
/// R2-PLUGIN §3 is the authoritative manifest schema. r2-def captures the
/// fields it needs to validate uniqueness and compile-target restrictions;
/// everything else is held in `extra` and forwarded verbatim to the
/// runtime.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginDef {
    /// Plugin identifier (reverse-DNS RECOMMENDED). Unique within an
    /// ensemble (R2-DEF §7.3).
    pub name: String,
    /// Optional human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// Plugin kind per R2-PLUGIN §4 (e.g. `"inline"`, `"http"`).
    #[serde(default)]
    pub kind: Option<String>,
    /// Restricted set of hive tiers this plugin compiles for.
    /// Default: `[linux]` per R2-DEF §7.3 (unavailable on MCU tiers).
    #[serde(default)]
    pub compile_target: Vec<String>,
    /// All other plugin-manifest fields (R2-PLUGIN §3 — type-specific).
    /// Forwarded to the runtime without interpretation by r2-def.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

impl PluginDef {
    /// Returns the manifest's `type` field (R2-PLUGIN §3.1) if present.
    ///
    /// Recognised values include `"process"`, `"in-process"`, `"http"`,
    /// and `"web"` (R2-PLUGIN §13). Unknown values are returned verbatim
    /// so a forward-compatible runtime may handle them.
    pub fn plugin_type(&self) -> Option<&str> {
        self.extra.get("type").and_then(|v| v.as_str())
    }

    /// Returns a typed view of this plugin if it declares
    /// `type: "web"` (R2-PLUGIN §13.2). Returns `Ok(None)` for any
    /// other type. Returns `Err` if the manifest is shaped as a web
    /// plugin but is invalid (missing `bundle`, illegal `mount`,
    /// channel without `target_sentant`, or the forbidden combination
    /// of `type: "web"` with `ipc` / `run`).
    pub fn as_web(&self) -> Result<Option<WebPluginManifest>, DefError> {
        if self.plugin_type() != Some("web") {
            return Ok(None);
        }

        if self.extra.contains_key("ipc") || self.extra.contains_key("run") {
            return Err(DefError::Validation(format!(
                "plugin {:?}: type:\"web\" forbids `ipc` and `run` (R2-PLUGIN §13.2)",
                self.name
            )));
        }

        let bundle = self
            .extra
            .get("bundle")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                DefError::Validation(format!(
                    "plugin {:?}: web plugin requires `bundle` field (R2-PLUGIN §13.2)",
                    self.name
                ))
            })?;

        let mount = match self.extra.get("mount") {
            Some(v) => {
                let s = v.as_str().ok_or_else(|| {
                    DefError::Validation(format!(
                        "plugin {:?}: `mount` must be a string",
                        self.name
                    ))
                })?;
                validate_mount(&self.name, s)?;
                Some(s.to_string())
            }
            None => None,
        };

        let channels = match self.extra.get("channels") {
            None => Vec::new(),
            Some(v) => {
                let seq = v.as_sequence().ok_or_else(|| {
                    DefError::Validation(format!(
                        "plugin {:?}: `channels` must be a sequence",
                        self.name
                    ))
                })?;
                let mut out = Vec::with_capacity(seq.len());
                for (i, item) in seq.iter().enumerate() {
                    out.push(parse_channel(&self.name, i, item)?);
                }
                out
            }
        };

        let graphql_schema = self
            .extra
            .get("graphql")
            .and_then(|v| v.as_mapping())
            .and_then(|m| m.get(serde_yaml::Value::String("schema".into())))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let csp = match self.extra.get("csp") {
            None => None,
            Some(v) => Some(parse_csp(&self.name, v)?),
        };

        Ok(Some(WebPluginManifest {
            name: self.name.clone(),
            bundle,
            mount,
            channels,
            subscriptions: Vec::new(), // registration-model field; legacy path doesn't carry it
            graphql_schema,
            csp,
        }))
    }
}

/// Typed view of a web plugin manifest (R2-PLUGIN §13.2).
///
/// Returned by [`PluginDef::as_web`]. r2-def does not resolve paths or
/// open files — the runtime is responsible for resolving `bundle`
/// against the score's directory and serving it.
#[derive(Debug, Clone)]
pub struct WebPluginManifest {
    /// Plugin identifier (mirrors `PluginDef::name`).
    pub name: String,
    /// Bundle directory path, relative to the score file. Required.
    pub bundle: String,
    /// URL mount path. Registration model: `route_prefix`, default `"/"`
    /// (R2-DEF §7.4). Legacy owned-plugin view: optional `mount`.
    pub mount: Option<String>,
    /// LEGACY owned-plugin channel descriptors (non-canonical shape; see
    /// [`WebChannelDef`]). Always empty on the registration path.
    pub channels: Vec<WebChannelDef>,
    /// CANONICAL registration-model subscriptions (`{name, event}` per
    /// R2-DEF §7.4). Always empty on the legacy owned-plugin path.
    pub subscriptions: Vec<WebSubscriptionDef>,
    /// Optional GraphQL schema path (v0.2; v0.1 hives ignore with a warning).
    pub graphql_schema: Option<String>,
    /// Optional CSP overrides (§13.9).
    pub csp: Option<WebCspOverride>,
}

impl WebPluginManifest {
    /// Build a typed web manifest from an ensemble's `registrations.r2-web`
    /// payload — the CANONICAL model per R2-ENSEMBLE §2.1.2 / R2-DEF §7.4: a web
    /// UI is a registration with the hive-shared R2-WEB singleton, NOT an
    /// ensemble-owned `plugins:` entry.
    ///
    /// Field mapping (specs-confirmed):
    /// - `route_prefix` → `mount` (DEFAULT `"/"` when absent)
    /// - `static_bundle` → `bundle` (required)
    /// - `subscriptions` → `channels` (default empty)
    /// - `graphql` → `graphql_schema` (optional)
    /// - `csp`: parked OUT of canon (no home in R2-WEB §3 / R2-DEF §7.4) — always
    ///   `None` here; never required.
    ///
    /// The registration model has no plugin name — the R2-WEB singleton
    /// namespaces by ENSEMBLE name, so `name` is set to `ensemble_name`.
    pub fn from_registration(
        ensemble_name: &str,
        payload: &serde_yaml::Value,
    ) -> Result<WebPluginManifest, DefError> {
        let map = payload.as_mapping().ok_or_else(|| {
            DefError::Validation(format!(
                "ensemble {ensemble_name:?}: registrations.r2-web must be a mapping (R2-DEF §7.4)"
            ))
        })?;
        let get = |k: &str| map.get(serde_yaml::Value::String(k.into()));

        let bundle = get("static_bundle")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                DefError::Validation(format!(
                    "ensemble {ensemble_name:?}: registrations.r2-web requires `static_bundle` (R2-DEF §7.4)"
                ))
            })?;

        let mount = match get("route_prefix") {
            None => "/".to_string(), // default mount (R2-DEF §7.4)
            Some(v) => {
                let s = v.as_str().ok_or_else(|| {
                    DefError::Validation(format!(
                        "ensemble {ensemble_name:?}: `route_prefix` must be a string"
                    ))
                })?;
                if !s.starts_with('/') {
                    return Err(DefError::Validation(format!(
                        "ensemble {ensemble_name:?}: `route_prefix` must start with '/' (R2-DEF §7.4)"
                    )));
                }
                s.to_string()
            }
        };

        // Canon shape (R2-DEF §7.4 example — the payload's only canon today):
        // subscriptions: [{name, event}].
        let subscriptions = match get("subscriptions") {
            None => Vec::new(),
            Some(v) => {
                let seq = v.as_sequence().ok_or_else(|| {
                    DefError::Validation(format!(
                        "ensemble {ensemble_name:?}: `subscriptions` must be a sequence"
                    ))
                })?;
                let mut out = Vec::with_capacity(seq.len());
                for (i, item) in seq.iter().enumerate() {
                    let m = item.as_mapping().ok_or_else(|| {
                        DefError::Validation(format!(
                            "ensemble {ensemble_name:?}: subscriptions[{i}] must be a mapping"
                        ))
                    })?;
                    let field = |k: &str| {
                        m.get(serde_yaml::Value::String(k.into()))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .ok_or_else(|| {
                                DefError::Validation(format!(
                                    "ensemble {ensemble_name:?}: subscriptions[{i}] requires `{k}` (R2-DEF §7.4)"
                                ))
                            })
                    };
                    out.push(WebSubscriptionDef { name: field("name")?, event: field("event")? });
                }
                out
            }
        };

        let graphql_schema = get("graphql")
            .and_then(|v| v.as_mapping())
            .and_then(|m| m.get(serde_yaml::Value::String("schema".into())))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok(WebPluginManifest {
            name: ensemble_name.to_string(),
            bundle,
            mount: Some(mount),
            channels: Vec::new(), // legacy owned-plugin view only — not part of the registration model
            subscriptions,
            graphql_schema,
            csp: None, // parked out of canon — never read from the registration
        })
    }
}

/// One WebSocket event subscription in an `r2-web` registration payload —
/// the CANONICAL shape per the R2-DEF §7.4 example: `{name, event}`.
/// (R2-DEF §7.4's "defined normatively in R2-WEB §3" is a dangling ref —
/// the §7.4 example is the payload's only canon today; fix queued with specs.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSubscriptionDef {
    /// Channel name exposed to the browser (e.g. `noteStream`).
    pub name: String,
    /// Event name this subscription forwards (e.g. `note.changed`).
    pub event: String,
}

/// One WebSocket channel descriptor on a LEGACY owned web plugin
/// (`plugins:` entry). NON-CANONICAL: this shape has no definition anywhere in
/// the spec corpus (the canonical web-UI surface is the `registrations.r2-web`
/// model, whose subscriptions are [`WebSubscriptionDef`]). Retained for the
/// legacy `plugins[]` view only.
#[derive(Debug, Clone)]
pub struct WebChannelDef {
    /// URL-safe channel name (`[a-zA-Z0-9._-]+`).
    pub name: String,
    /// Sentant id within the same ensemble that handles this channel.
    pub target_sentant: String,
    /// Maximum frame size in bytes. Default 65536.
    pub max_frame_bytes: u32,
}

/// CSP overrides as declared by a web plugin manifest (R2-PLUGIN §13.9).
///
/// Only directives that may be added to are exposed here; `'unsafe-eval'`
/// and `'unsafe-inline'` for `script-src` cannot be set via manifest.
#[derive(Debug, Clone, Default)]
pub struct WebCspOverride {
    /// Additional `script-src` sources.
    pub script_src: Vec<String>,
    /// Additional `style-src` sources.
    pub style_src: Vec<String>,
    /// Additional `connect-src` sources.
    pub connect_src: Vec<String>,
    /// Additional `img-src` sources.
    pub img_src: Vec<String>,
    /// Additional `font-src` sources.
    pub font_src: Vec<String>,
}

fn validate_mount(plugin: &str, mount: &str) -> Result<(), DefError> {
    if !(mount.starts_with("/ensemble/") || mount.starts_with("/plugin/")) {
        return Err(DefError::Validation(format!(
            "plugin {plugin:?}: `mount` must start with /ensemble/ or /plugin/ (R2-PLUGIN §13.2)"
        )));
    }
    if mount.contains("..") || mount.contains('?') || mount.contains('#') {
        return Err(DefError::Validation(format!(
            "plugin {plugin:?}: `mount` must not contain `..` or query/fragment components"
        )));
    }
    Ok(())
}

fn parse_channel(
    plugin: &str,
    index: usize,
    v: &serde_yaml::Value,
) -> Result<WebChannelDef, DefError> {
    let map = v.as_mapping().ok_or_else(|| {
        DefError::Validation(format!(
            "plugin {plugin:?}: channels[{index}] must be a mapping"
        ))
    })?;
    let get_str = |key: &str| -> Option<String> {
        map.get(serde_yaml::Value::String(key.into()))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    let name = get_str("name").ok_or_else(|| {
        DefError::Validation(format!(
            "plugin {plugin:?}: channels[{index}] missing `name`"
        ))
    })?;
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        || name.is_empty()
    {
        return Err(DefError::Validation(format!(
            "plugin {plugin:?}: channel name {name:?} is not URL-safe"
        )));
    }
    let target_sentant = get_str("target_sentant").ok_or_else(|| {
        DefError::Validation(format!(
            "plugin {plugin:?}: channels[{index}] missing `target_sentant`"
        ))
    })?;
    let max_frame_bytes = map
        .get(serde_yaml::Value::String("max_frame_bytes".into()))
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
        .unwrap_or(65536);
    Ok(WebChannelDef {
        name,
        target_sentant,
        max_frame_bytes,
    })
}

fn parse_csp(plugin: &str, v: &serde_yaml::Value) -> Result<WebCspOverride, DefError> {
    let map = v.as_mapping().ok_or_else(|| {
        DefError::Validation(format!("plugin {plugin:?}: `csp` must be a mapping"))
    })?;
    let take = |key: &str| -> Result<Vec<String>, DefError> {
        let Some(value) = map.get(serde_yaml::Value::String(key.into())) else {
            return Ok(Vec::new());
        };
        let seq = value.as_sequence().ok_or_else(|| {
            DefError::Validation(format!(
                "plugin {plugin:?}: csp.{key} must be a sequence of strings"
            ))
        })?;
        seq.iter()
            .map(|s| {
                s.as_str()
                    .ok_or_else(|| {
                        DefError::Validation(format!(
                            "plugin {plugin:?}: csp.{key} entries must be strings"
                        ))
                    })
                    .map(str::to_string)
            })
            .collect()
    };
    let out = WebCspOverride {
        script_src: take("script_src")?,
        style_src: take("style_src")?,
        connect_src: take("connect_src")?,
        img_src: take("img_src")?,
        font_src: take("font_src")?,
    };
    let forbidden = ["'unsafe-eval'", "'unsafe-inline'"];
    for src in &out.script_src {
        if forbidden.contains(&src.as_str()) {
            return Err(DefError::Validation(format!(
                "plugin {plugin:?}: csp.script_src must not contain {src:?} (R2-PLUGIN §13.9)"
            )));
        }
    }
    Ok(out)
}
