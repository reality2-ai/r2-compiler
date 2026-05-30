//! # r2-compiler-webapp
//!
//! The browser-side Rust foundation of r2-compiler's webapp role-ensemble.
//! Compiled to `wasm32-unknown-unknown` via `wasm-pack build --target web`.
//!
//! ## v0.1 scope
//!
//! A minimal wasm-bindgen surface that proves the WASM path works
//! end-to-end. Exposes:
//!
//! - [`fnv1a_32`] — FNV-1a-32 hashing for arbitrary strings, via the
//!   vendored `r2-fnv` crate. Used by JS to display class hashes +
//!   verify the manifest's pre-computed `class_hash` fields.
//! - [`class_hash_hex`] — same hash, formatted as `"0x<8-hex>"` for
//!   consistency with how the catalogue's TOML files spell it.
//! - [`verify_class_hash`] — boolean check: does `string`'s FNV-1a-32
//!   match the provided hex literal (e.g. `"0x624c47bc"`)? Used by the
//!   webapp to flag mismatches in `board.toml` / `ensemble.yaml`.
//! - [`version`] — semver string for this WASM build, for debugging.
//!
//! ## Phase 2-full scope (future commits)
//!
//! The full set of webapp-hive sentants (Catalogue / Composition /
//! SourceViewer / Builder / Author / Apiary) per SPEC-R2-COMPILER §3.2,
//! using the vendored `r2-wasm` crate to bring the R2 hive into the
//! browser. The class-hash functions in this v0.1 stay; everything
//! else grows alongside them.

use wasm_bindgen::prelude::*;

/// Compute the FNV-1a-32 hash of a UTF-8 string. Returns a `u32`.
///
/// Used for R2-CAP class hashes and R2-FNV event-name hashes. The
/// browser's manifest viewer displays this alongside the class string
/// so operators can verify the pre-computed hash in `board.toml` /
/// `apiary.toml` matches.
#[wasm_bindgen]
pub fn fnv1a_32(s: &str) -> u32 {
    r2_fnv::fnv1a_32(s.as_bytes())
}

/// Hash + hex-format in one call. Output shape: `"0x624c47bc"`
/// (lowercase, zero-padded, eight hex digits). Matches the spelling
/// used in catalogue TOML files.
#[wasm_bindgen]
pub fn class_hash_hex(s: &str) -> String {
    format!("0x{:08x}", fnv1a_32(s))
}

/// Verify that `s`'s FNV-1a-32 matches `expected_hex`. Returns `true`
/// on match. Accepts `expected_hex` in either `"0x624c47bc"` or
/// `"624c47bc"` form, case-insensitive.
#[wasm_bindgen]
pub fn verify_class_hash(s: &str, expected_hex: &str) -> bool {
    let parse_u32_hex = |raw: &str| -> Option<u32> {
        let trimmed = raw.trim().trim_start_matches("0x").trim_start_matches("0X");
        u32::from_str_radix(trimmed, 16).ok()
    };
    match parse_u32_hex(expected_hex) {
        Some(expected) => fnv1a_32(s) == expected,
        None => false,
    }
}

/// Returns this crate's semver. Useful for the webapp's "About" pane
/// to confirm which WASM bundle is loaded.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// One-shot init called by JS on module load. Wires a panic hook
/// that routes Rust panics to `console.error` so the operator sees
/// something useful when WASM panics in production.
#[wasm_bindgen(start)]
pub fn on_load() {
    // Wire up panics to the browser console. No external dep — we
    // synthesise the hook inline with js-sys so the cdylib stays small.
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("r2-compiler-webapp WASM panic: {info}");
        let s = wasm_bindgen::JsValue::from_str(&msg);
        let console = js_sys::Reflect::get(
            &js_sys::global(),
            &wasm_bindgen::JsValue::from_str("console"),
        ).unwrap_or(wasm_bindgen::JsValue::NULL);
        let err = js_sys::Reflect::get(&console, &wasm_bindgen::JsValue::from_str("error"))
            .unwrap_or(wasm_bindgen::JsValue::NULL);
        if let Ok(func) = err.dyn_into::<js_sys::Function>() {
            let _ = func.call1(&console, &s);
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    // The reference class hash for the rocker deployment (from
    // r2-workshop/specifications/SPEC-R2-WORKSHOP-ENSEMBLE.md §2.1):
    //   class = "nz.ac.auckland.rocker"
    //   FNV-1a-32 = 0x624c47bc
    const ROCKER_CLASS: &str = "nz.ac.auckland.rocker";
    const ROCKER_HASH: u32 = 0x624c47bc;

    #[test]
    fn rocker_class_hash_matches_spec() {
        assert_eq!(fnv1a_32(ROCKER_CLASS), ROCKER_HASH);
    }

    #[test]
    fn class_hash_hex_formats_consistently() {
        assert_eq!(class_hash_hex(ROCKER_CLASS), "0x624c47bc");
        // Empty string is a documented edge case — FNV-1a's offset basis.
        assert_eq!(class_hash_hex(""), "0x811c9dc5");
    }

    #[test]
    fn verify_class_hash_accepts_both_hex_forms() {
        assert!(verify_class_hash(ROCKER_CLASS, "0x624c47bc"));
        assert!(verify_class_hash(ROCKER_CLASS, "624C47BC"));     // uppercase, no prefix
        assert!(verify_class_hash(ROCKER_CLASS, "0X624C47BC"));   // uppercase prefix
        assert!(verify_class_hash(ROCKER_CLASS, "  624c47bc  ")); // surrounding whitespace
    }

    #[test]
    fn verify_class_hash_rejects_mismatch_or_bad_hex() {
        assert!(!verify_class_hash(ROCKER_CLASS, "0xdeadbeef"));
        assert!(!verify_class_hash(ROCKER_CLASS, "not-hex"));
        assert!(!verify_class_hash(ROCKER_CLASS, ""));
    }

    #[test]
    fn version_is_semver_shaped() {
        let v = version();
        // Loose check: three dot-separated numbers.
        let parts: Vec<&str> = v.split('.').collect();
        assert_eq!(parts.len(), 3, "expected x.y.z, got {v}");
        for p in parts {
            assert!(p.chars().all(|c| c.is_ascii_digit() || c == '-' || c.is_alphanumeric()), "non-version chars in {p}");
        }
    }
}
