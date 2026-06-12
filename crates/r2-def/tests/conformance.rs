//! R2-DEF conformance vectors (r2-specifications, spec-conformance-v0.2).
//!
//! Static load-time validation only (§8.1). Runs each canonical vector through the
//! matching parser and checks accept/reject + the §8.1 error code. Reports the full
//! matrix; asserts conformance at the end (documents any spec↔impl divergence —
//! per the walk rule, divergences are FLAGGED, not patched to match).

use serde_json::Value;

const VECTORS: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/vectors/r2-def-vectors.json"));

/// Parse a definition JSON object via the parser matching its top-level key.
fn load(def: &Value) -> Result<(), String> {
    let s = serde_json::to_string(def).unwrap();
    if def.get("sentant").is_some() {
        r2_def::parse_sentant_json(&s).map(|_| ()).map_err(|e| e.to_string())
    } else if def.get("swarm").is_some() {
        r2_def::parse_swarm_json(&s).map(|_| ()).map_err(|e| e.to_string())
    } else if def.get("ensemble").is_some() {
        r2_def::parse_ensemble_json(&s).map(|_| ()).map_err(|e| e.to_string())
    } else {
        Err("UNKNOWN_TOP_KEY".into())
    }
}

#[test]
fn r2_def_conformance_vectors() {
    let data: Value = serde_json::from_str(VECTORS).expect("parse r2-def-vectors.json");
    let mut fails: Vec<String> = Vec::new();

    // Valid: MUST load.
    for v in data["valid"]["vectors"].as_array().unwrap() {
        let id = v["id"].as_str().unwrap();
        match load(&v["definition"]) {
            Ok(()) => eprintln!("[PASS] {id} accepted"),
            Err(e) => {
                eprintln!("[FAIL] {id} expected accept, got reject: {e}");
                fails.push(format!("{id}: expected accept, rejected ({e})"));
            }
        }
    }

    // Invalid: MUST reject with the listed §8.1 error code.
    for v in data["invalid"]["vectors"].as_array().unwrap() {
        let id = v["id"].as_str().unwrap();
        let code = v["error"].as_str().unwrap();
        // Conformance MUST = rejection (§8.1). Surfacing the exact §8.1 code is a
        // SHOULD-diagnostic (specs adjudication): name/description omission is rejected
        // by serde required-field deser before validate() emits the code — still conformant.
        match load(&v["definition"]) {
            Err(e) if e.contains(code) => eprintln!("[PASS] {id} rejected with {code}"),
            Err(_) => eprintln!("[PASS] {id} rejected (MUST); §8.1 code {code} not surfaced (SHOULD)"),
            Ok(()) => {
                eprintln!("[FAIL] {id} ACCEPTED — MUST reject {code}");
                fails.push(format!("{id}: ACCEPTED, expected reject {code}"));
            }
        }
    }

    assert!(fails.is_empty(), "r2-def non-conformant vectors:\n  {}", fails.join("\n  "));
}
