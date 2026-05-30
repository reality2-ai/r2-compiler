//! Tests for the web plugin manifest view (R2-PLUGIN §13.2).

use r2_def::{parse_ensemble_yaml, PluginDef};

fn plugin_from_yaml(plugins_block: &str) -> PluginDef {
    let yaml = format!(
        "ensemble:\n  name: web-test\n  description: \"web plugin parser fixture\"\n  version: \"0.1.0\"\n  ensemble_version: \"0.1\"\n  sentants:\n    - name: stub\n      description: stub\n      automations:\n        - name: main\n          initial: idle\n          transitions: []\n  plugins:\n{plugins_block}"
    );
    let score = parse_ensemble_yaml(&yaml).expect("parse");
    score.plugins.into_iter().next().expect("one plugin")
}

#[test]
fn non_web_plugin_returns_none() {
    let p = plugin_from_yaml(
        "    - name: example.http\n      type: \"http\"\n      url: \"https://example.com\"\n",
    );
    assert_eq!(p.plugin_type(), Some("http"));
    assert!(p.as_web().expect("ok").is_none());
}

#[test]
fn web_plugin_minimal_parses() {
    let p = plugin_from_yaml(
        "    - name: notekeeper-ui\n      type: \"web\"\n      bundle: \"ui/\"\n",
    );
    let w = p.as_web().expect("ok").expect("web");
    assert_eq!(w.name, "notekeeper-ui");
    assert_eq!(w.bundle, "ui/");
    assert!(w.mount.is_none());
    assert!(w.channels.is_empty());
    assert!(w.csp.is_none());
}

#[test]
fn web_plugin_full_parses() {
    let p = plugin_from_yaml(
        "    - name: notekeeper-ui\n      type: \"web\"\n      bundle: \"ui/\"\n      mount: \"/ensemble/notekeeper\"\n      channels:\n        - name: live\n          target_sentant: notekeeper-core\n          max_frame_bytes: 32768\n        - name: status.events\n          target_sentant: notekeeper-core\n      csp:\n        script_src: [\"'self'\"]\n        connect_src: [\"'self'\"]\n",
    );
    let w = p.as_web().expect("ok").expect("web");
    assert_eq!(w.mount.as_deref(), Some("/ensemble/notekeeper"));
    assert_eq!(w.channels.len(), 2);
    assert_eq!(w.channels[0].name, "live");
    assert_eq!(w.channels[0].max_frame_bytes, 32768);
    assert_eq!(w.channels[1].name, "status.events");
    assert_eq!(w.channels[1].max_frame_bytes, 65536);
    let csp = w.csp.expect("csp");
    assert_eq!(csp.script_src, vec!["'self'".to_string()]);
}

#[test]
fn web_plugin_missing_bundle_rejected() {
    let p = plugin_from_yaml("    - name: bad\n      type: \"web\"\n");
    let err = p.as_web().expect_err("rejected");
    assert!(err.to_string().contains("bundle"));
}

#[test]
fn web_plugin_with_run_rejected() {
    let p = plugin_from_yaml(
        "    - name: bad\n      type: \"web\"\n      bundle: \"ui/\"\n      run: \"/usr/bin/echo\"\n",
    );
    let err = p.as_web().expect_err("rejected");
    assert!(err.to_string().contains("run"));
}

#[test]
fn web_plugin_with_ipc_rejected() {
    let p = plugin_from_yaml(
        "    - name: bad\n      type: \"web\"\n      bundle: \"ui/\"\n      ipc: \"unix_socket\"\n",
    );
    let err = p.as_web().expect_err("rejected");
    assert!(err.to_string().contains("ipc"));
}

#[test]
fn web_plugin_bad_mount_rejected() {
    let p = plugin_from_yaml(
        "    - name: bad\n      type: \"web\"\n      bundle: \"ui/\"\n      mount: \"/admin\"\n",
    );
    let err = p.as_web().expect_err("rejected");
    assert!(err.to_string().contains("mount"));
}

#[test]
fn web_plugin_bad_channel_name_rejected() {
    let p = plugin_from_yaml(
        "    - name: bad\n      type: \"web\"\n      bundle: \"ui/\"\n      channels:\n        - name: \"with space\"\n          target_sentant: x\n",
    );
    let err = p.as_web().expect_err("rejected");
    assert!(err.to_string().contains("URL-safe"));
}

#[test]
fn web_plugin_unsafe_eval_rejected() {
    let p = plugin_from_yaml(
        "    - name: bad\n      type: \"web\"\n      bundle: \"ui/\"\n      csp:\n        script_src: [\"'unsafe-eval'\"]\n",
    );
    let err = p.as_web().expect_err("rejected");
    assert!(err.to_string().contains("unsafe-eval"));
}
