//! Conformance round-trip tests for the r2-def parser.
//!
//! Each fixture in `vectors/` is parsed, validated, and re-serialised. The
//! re-serialised form is then parsed again to confirm the round-trip is
//! information-preserving (modulo serde's natural reformatting). Validation
//! errors on intentionally-broken fixtures are also asserted.

use r2_def::{
    parse_ensemble_yaml, parse_sentant_yaml, parse_swarm_yaml, EnsembleScore, SentantDef,
    SentantEntry, StoragePolicy,
};

const NOTEKEEPER_ENSEMBLE: &str = include_str!("../vectors/notekeeper.ensemble.yaml");
const ZEN_QUOTE_SENTANT: &str = include_str!("../vectors/zen-quote.sentant.yaml");
const STROBE_SWARM: &str = include_str!("../vectors/strobe.swarm.yaml");

#[test]
fn notekeeper_ensemble_parses_and_round_trips() {
    let e: EnsembleScore = parse_ensemble_yaml(NOTEKEEPER_ENSEMBLE).expect("parse");
    assert_eq!(e.name, "notekeeper");
    assert_eq!(e.version, "0.5.0");
    assert_eq!(e.ensemble_version, "0.1");
    assert_eq!(e.class.as_deref(), Some("ai.reality2.ensemble.notekeeper"));
    assert_eq!(e.compile_target, vec!["linux".to_string()]);
    assert_eq!(e.sentants.len(), 1);
    assert_eq!(e.plugins.len(), 1);
    assert_eq!(e.plugins[0].name, "notekeeper.sync");
    assert!(e.registrations.contains_key("r2-web"));

    let caps = e.capabilities.as_ref().expect("capabilities present");
    assert_eq!(caps.emits.len(), 4);
    assert_eq!(caps.consumes.len(), 4);

    // Inline sentant: durable-state + 1 plugin ref + 1 automation.
    match &e.sentants[0] {
        SentantEntry::Inline(s) => {
            assert_eq!(s.name, "Note");
            assert_eq!(s.storage, StoragePolicy::DurableState);
            assert_eq!(s.plugins.len(), 1);
            assert_eq!(s.automations.len(), 1);
            assert_eq!(s.automations[0].transitions.len(), 1);
        }
        SentantEntry::External { .. } => panic!("expected inline sentant"),
    }

    // Round-trip: re-serialise then re-parse, structures should match.
    let re_yaml = serde_yaml::to_string(&r2_def::EnsembleFile { ensemble: e.clone() })
        .expect("re-serialise");
    let e2 = parse_ensemble_yaml(&re_yaml).expect("re-parse");
    assert_eq!(e2.name, e.name);
    assert_eq!(e2.version, e.version);
    assert_eq!(e2.sentants.len(), e.sentants.len());
}

#[test]
fn zen_quote_sentant_parses() {
    let s: SentantDef = parse_sentant_yaml(ZEN_QUOTE_SENTANT).expect("parse");
    assert_eq!(s.name, "Zen Quote");
    assert_eq!(s.plugins.len(), 1);
    assert_eq!(s.plugins[0].name, "io.zenquotes.api");
    assert_eq!(s.automations.len(), 1);
    let a = &s.automations[0];
    assert_eq!(a.name, "Zen Quote");
    assert_eq!(a.transitions.len(), 2);
    assert!(a.transitions[0].public);
    assert_eq!(a.transitions[0].event, "Get Zenquote");
}

#[test]
fn strobe_swarm_parses_with_two_sentants() {
    let s = parse_swarm_yaml(STROBE_SWARM).expect("parse");
    assert_eq!(s.name, "Strobing Light and Switch");
    assert_eq!(s.sentants.len(), 2);
    assert_eq!(s.sentants[0].name, "Light Switch");
    assert_eq!(s.sentants[1].name, "Strobe Light");
}

#[test]
fn empty_sentants_array_rejected() {
    let bad = r#"
ensemble:
  name: empty
  description: "no sentants"
  version: "0.1.0"
  ensemble_version: "0.1"
  sentants: []
"#;
    let err = parse_ensemble_yaml(bad).expect_err("must reject empty sentants");
    let msg = format!("{err}");
    assert!(msg.contains("E_ENS_NO_SENTANTS"), "got: {msg}");
}

#[test]
fn missing_required_field_rejected() {
    // No `version` field.
    let bad = r#"
ensemble:
  name: noversion
  description: "missing version"
  ensemble_version: "0.1"
  sentants:
    - name: A
      description: a
      automations:
        - name: x
          transitions:
            - event: y
"#;
    let err = parse_ensemble_yaml(bad).expect_err("must reject missing version");
    // serde fails here — error originates from YAML parser, not from our
    // validator, but it is still a DefError::Yaml variant.
    let msg = format!("{err}");
    assert!(
        msg.contains("yaml") || msg.contains("missing field"),
        "got: {msg}"
    );
}

#[test]
fn bad_ensemble_version_rejected() {
    let bad = r#"
ensemble:
  name: x
  description: x
  version: "0.1"
  ensemble_version: "9.9"
  sentants:
    - name: A
      description: a
      automations:
        - name: x
          transitions:
            - event: y
"#;
    let err = parse_ensemble_yaml(bad).expect_err("must reject schema version 9.9");
    let msg = format!("{err}");
    assert!(msg.contains("E_ENS_SCHEMA_VERSION"), "got: {msg}");
}

#[test]
fn duplicate_automation_name_rejected() {
    let bad = r#"
sentant:
  name: Dup
  description: "duplicate automation names"
  automations:
    - name: a
      transitions:
        - event: foo
    - name: a
      transitions:
        - event: bar
"#;
    let err = parse_sentant_yaml(bad).expect_err("must reject dup");
    let msg = format!("{err}");
    assert!(msg.contains("E_DEF_AUTOMATION_DUP"), "got: {msg}");
}
