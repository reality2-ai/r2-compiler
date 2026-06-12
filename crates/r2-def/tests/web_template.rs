//! Verifies that the CANONICAL web-plugin template ensemble
//! (`r2-specifications/templates/plugins/web/ensemble.yaml`, R2-ENSEMBLE §2.1.2 /
//! R2-DEF §7.4) parses through r2-def and yields a typed web manifest via the
//! REGISTRATION model: a web UI is a `registrations.r2-web` entry with the
//! hive-shared R2-WEB singleton — NOT an ensemble-owned `plugins:` entry.
//!
//! RESILIENT FIXTURE: the template is read at RUNTIME (sibling checkout, or the
//! `R2_WEB_TEMPLATE` env override) and the test SKIPS if absent — a compile-time
//! `include_str!` would break the whole workspace build when missing.

use r2_def::parse_ensemble_yaml;

#[test]
fn template_parses_and_yields_web_registration() {
    let default_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../r2-specifications/templates/plugins/web/ensemble.yaml"
    );
    let path = std::env::var("R2_WEB_TEMPLATE").unwrap_or_else(|_| default_path.to_string());
    let template = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("skip: web-plugin template not present at {path}");
            return;
        }
    };

    let score = parse_ensemble_yaml(&template).expect("canonical template must parse");

    // Template specifics (specs-confirmed).
    assert_eq!(score.name, "web-template");
    assert_eq!(score.ensemble_version, "0.1");
    assert_eq!(score.sentants.len(), 1, "exactly one anchor sentant");
    assert!(
        score.plugins.is_empty(),
        "canon: web is a registration, NOT an ensemble-owned plugins[] entry (R2-ENSEMBLE §2.1.2)"
    );

    // Registration model mapping (registrations.r2-web -> WebPluginManifest).
    let web = score
        .web_registration()
        .expect("r2-web registration payload valid")
        .expect("template registers with r2-web");
    assert_eq!(web.name, "web-template", "singleton namespaces by ENSEMBLE name");
    assert_eq!(web.bundle, "./ui/", "static_bundle -> bundle");
    assert_eq!(web.mount.as_deref(), Some("/"), "route_prefix -> mount (default '/')");
    assert!(web.subscriptions.is_empty(), "no subscriptions declared");
    assert!(web.channels.is_empty(), "channels are legacy-path only");
    assert!(web.graphql_schema.is_none(), "no graphql fragment");
    assert!(web.csp.is_none(), "csp is parked out of canon — always None");
}
