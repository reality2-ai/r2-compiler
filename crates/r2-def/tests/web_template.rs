//! Verifies that the web-plugin template ensemble at
//! `r2-specifications/templates/plugins/web/ensemble.yaml` parses through
//! r2-def and yields a typed web plugin manifest. If the template drifts
//! from R2-PLUGIN §13, this test fails.

use r2_def::parse_ensemble_yaml;

const TEMPLATE: &str = include_str!(
    "../../../../r2-specifications/templates/plugins/web/ensemble.yaml"
);

#[test]
fn template_parses_and_yields_web_plugin() {
    let score = parse_ensemble_yaml(TEMPLATE).expect("template parse");
    assert_eq!(score.name, "web-template");
    assert_eq!(score.plugins.len(), 1);

    let plugin = &score.plugins[0];
    assert_eq!(plugin.name, "ui");
    assert_eq!(plugin.plugin_type(), Some("web"));

    let web = plugin
        .as_web()
        .expect("web manifest valid")
        .expect("is a web plugin");
    assert_eq!(web.bundle, "ui/");
    assert!(web.mount.is_none(), "default mount expected");
    assert!(web.channels.is_empty(), "no channels declared by default");
    assert!(web.csp.is_none(), "no csp override by default");
}
