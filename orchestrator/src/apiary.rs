//! Apiary state — parses `apiary.toml` per SPEC-APIARY-LAYOUT §3 +
//! SPEC-APIARY-COMPOSE §4 and produces the structured state the
//! webapp's apiary canvas renders.
//!
//! Supports both the simple form (`carriers = [...]` shorthand from
//! SPEC-APIARY-LAYOUT §3) and the full form
//! (`[[role_ensembles.targets]]` blocks from SPEC-APIARY-COMPOSE §4.2).
//! Both expand to the same internal `RoleEnsemble { targets: Vec<Target> }`
//! shape so the renderer doesn't care which the operator wrote.
//!
//! Target-type inference (per SPEC-APIARY-COMPOSE §3):
//! - `esp32-*`     → `mcu-fw`
//! - `nrf52*`      → `mcu-fw`
//! - `rp2040*`     → `mcu-fw`
//! - `wasm32-*`    → `wasm`
//! - other (`linux-x86_64`, `darwin-arm64`, …) → `native` by default,
//!   but **MUST be explicit** in TOML to disambiguate from `beam`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The state the webapp renders. Sent as the payload of
/// `r2.composer.apiary.active`.
#[derive(Debug, Clone, Serialize)]
pub struct ApiaryState {
    pub name: String,
    pub description: String,
    pub class: String,
    pub class_hash: String, // 0xHHHHHHHH
    pub version: String,
    pub tg: ApiaryTg,
    pub roles: Vec<RoleEnsemble>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiaryTg {
    pub keyholder_fp: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoleEnsemble {
    pub role: String,
    pub ensemble: String,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Target {
    /// Stable per-target id; format `<role>:<host>`.
    pub id: String,
    pub target_type: String,
    pub host: String,
    #[serde(rename = "plugin_overrides")]
    pub overrides: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub co_located_with: Option<String>,
}

// ── On-disk TOML shape (private — only the renderer sees ApiaryState) ──

#[derive(Debug, Deserialize)]
struct TomlApiary {
    apiary: TomlApiaryHeader,
    tg: Option<TomlApiaryTg>,
    #[serde(default, rename = "role_ensembles")]
    role_ensembles: Vec<TomlRoleEnsemble>,
}

#[derive(Debug, Deserialize)]
struct TomlApiaryHeader {
    name: String,
    #[serde(default)]
    description: String,
    class: String,
    #[serde(default)]
    version: String,
}

#[derive(Debug, Deserialize)]
struct TomlApiaryTg {
    #[serde(default)]
    keyholder_fp: String,
}

#[derive(Debug, Deserialize)]
struct TomlRoleEnsemble {
    role: String,
    ensemble: String,
    /// Simple shorthand: one target per carrier slug, inferred target_type.
    #[serde(default)]
    carriers: Vec<String>,
    /// Full form: each target explicit.
    #[serde(default)]
    targets: Vec<TomlTarget>,
}

#[derive(Debug, Deserialize)]
struct TomlTarget {
    target_type: Option<String>,
    host: String,
    #[serde(default)]
    plugin_overrides: BTreeMap<String, String>,
    co_located_with: Option<String>,
}

// ── Public API ─────────────────────────────────────────────────────────

/// Load + parse an `apiary.toml` from the given apiary directory.
pub fn load(apiary_dir: &Path) -> Result<ApiaryState, String> {
    let toml_path = apiary_dir.join("apiary.toml");
    let raw = std::fs::read_to_string(&toml_path)
        .map_err(|e| format!("read {}: {e}", toml_path.display()))?;
    let parsed: TomlApiary = toml::from_str(&raw)
        .map_err(|e| format!("parse {}: {e}", toml_path.display()))?;
    Ok(materialise(parsed))
}

fn materialise(t: TomlApiary) -> ApiaryState {
    let class_hash = format!("0x{:08x}", r2_fnv::fnv1a_32(t.apiary.class.as_bytes()));
    let tg = ApiaryTg {
        keyholder_fp: t.tg.map(|x| x.keyholder_fp).unwrap_or_default(),
    };

    let roles = t.role_ensembles.into_iter().map(materialise_role).collect();

    ApiaryState {
        name: t.apiary.name,
        description: t.apiary.description,
        class: t.apiary.class,
        class_hash,
        version: t.apiary.version,
        tg,
        roles,
    }
}

fn materialise_role(r: TomlRoleEnsemble) -> RoleEnsemble {
    let role_name = r.role;
    let mut targets = Vec::new();

    // Full form takes precedence — if the operator wrote both, the
    // SPEC-APIARY-COMPOSE §4.3 says it's invalid; we accept and let
    // validation flag it later.
    if !r.targets.is_empty() {
        for t in r.targets {
            let target_type = t.target_type.unwrap_or_else(|| infer_target_type(&t.host));
            targets.push(Target {
                id: format!("{role_name}:{}", t.host),
                target_type,
                host: t.host,
                overrides: t.plugin_overrides,
                co_located_with: t.co_located_with,
            });
        }
    } else {
        for carrier in r.carriers {
            let target_type = infer_target_type(&carrier);
            targets.push(Target {
                id: format!("{role_name}:{carrier}"),
                target_type,
                host: carrier,
                overrides: BTreeMap::new(),
                co_located_with: None,
            });
        }
    }

    RoleEnsemble {
        role: role_name,
        ensemble: r.ensemble,
        targets,
    }
}

/// Per SPEC-APIARY-COMPOSE §3: target type inferred from host slug.
/// Falls back to `native` for unknown hosts; the TOML SHOULD be explicit
/// when the host could be BEAM rather than native.
fn infer_target_type(host: &str) -> String {
    if host.starts_with("esp32") || host.starts_with("nrf52") || host.starts_with("rp2040") {
        "mcu-fw".into()
    } else if host.starts_with("wasm32") {
        "wasm".into()
    } else {
        "native".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tmp(content: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let mut f = std::fs::File::create(dir.path().join("apiary.toml")).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        dir
    }

    #[test]
    fn parses_full_form_with_overrides_and_coloc() {
        let dir = write_tmp(
            r#"
            [apiary]
            name = "rocker-rig"
            class = "nz.ac.auckland.rocker"
            version = "0.2.0"

            [tg]
            keyholder_fp = "ab12cd34"

            [[role_ensembles]]
            role = "sensor"
            ensemble = "rocker-sensor"

            [[role_ensembles.targets]]
            target_type = "mcu-fw"
            host = "esp32-c6-dfr1117"
            plugin_overrides = { "ai.reality2.cap.accel.triaxial" = "lis2dh" }

            [[role_ensembles]]
            role = "webapp-server"
            ensemble = "rocker-webapp-server"

            [[role_ensembles.targets]]
            target_type = "beam"
            host = "linux-x86_64"
            co_located_with = "controller"
            "#,
        );
        let st = load(dir.path()).expect("parses");
        assert_eq!(st.name, "rocker-rig");
        assert_eq!(st.class, "nz.ac.auckland.rocker");
        assert_eq!(st.class_hash, "0x624c47bc");
        assert_eq!(st.tg.keyholder_fp, "ab12cd34");
        assert_eq!(st.roles.len(), 2);

        let sensor = &st.roles[0];
        assert_eq!(sensor.role, "sensor");
        assert_eq!(sensor.targets.len(), 1);
        let t = &sensor.targets[0];
        assert_eq!(t.id, "sensor:esp32-c6-dfr1117");
        assert_eq!(t.target_type, "mcu-fw");
        assert_eq!(t.overrides.get("ai.reality2.cap.accel.triaxial").map(|s| s.as_str()), Some("lis2dh"));
        assert!(t.co_located_with.is_none());

        let webapp = &st.roles[1];
        assert_eq!(webapp.targets[0].target_type, "beam");
        assert_eq!(webapp.targets[0].co_located_with.as_deref(), Some("controller"));
    }

    #[test]
    fn expands_simple_carriers_shorthand() {
        let dir = write_tmp(
            r#"
            [apiary]
            name = "demo"
            class = "ai.reality2.demo"

            [[role_ensembles]]
            role = "sensor"
            ensemble = "rocker-sensor"
            carriers = ["esp32-s3-devkitc", "esp32-s3-xiao", "esp32-c6-dfr1117"]
            "#,
        );
        let st = load(dir.path()).expect("parses");
        let sensor = &st.roles[0];
        assert_eq!(sensor.targets.len(), 3);
        for t in &sensor.targets {
            assert_eq!(t.target_type, "mcu-fw");
            assert!(t.overrides.is_empty());
        }
        assert_eq!(sensor.targets[2].id, "sensor:esp32-c6-dfr1117");
    }

    #[test]
    fn target_type_inference() {
        assert_eq!(infer_target_type("esp32-s3-devkitc"), "mcu-fw");
        assert_eq!(infer_target_type("nrf52840-dk"),     "mcu-fw");
        assert_eq!(infer_target_type("rp2040-pico"),     "mcu-fw");
        assert_eq!(infer_target_type("wasm32-browser"),  "wasm");
        assert_eq!(infer_target_type("linux-x86_64"),    "native");
        assert_eq!(infer_target_type("darwin-arm64"),    "native");
    }
}
