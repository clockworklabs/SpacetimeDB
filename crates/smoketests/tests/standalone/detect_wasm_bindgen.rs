use spacetimedb_smoketests::build_rust_module;

/// Module code that uses wasm_bindgen (should be rejected)
const MODULE_CODE_WASM_BINDGEN: &str = r#"
use spacetimedb::{log, ReducerContext};

#[spacetimedb::reducer]
pub fn test(_ctx: &ReducerContext) {
    log::info!("Hello! {}", now());
}

#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    fn now() -> i32;
}
"#;

/// Module code that uses getrandom via rand (should be rejected)
const MODULE_CODE_GETRANDOM: &str = r#"
use spacetimedb::{log, ReducerContext};

#[spacetimedb::reducer]
pub fn test(_ctx: &ReducerContext) {
    log::info!("Hello! {}", rand::random::<u8>());
}
"#;

/// Ensure that spacetime build properly catches wasm_bindgen imports
/// Standalone-only: this validates local CLI build diagnostics without publishing a module.
#[test]
fn test_detect_wasm_bindgen() {
    let output = build_rust_module(MODULE_CODE_WASM_BINDGEN, r#"wasm-bindgen = "0.2""#);
    assert!(!output.status.success(), "Expected build to fail with wasm_bindgen");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("wasm-bindgen detected"),
        "Expected 'wasm-bindgen detected' in stderr, got: {}",
        stderr
    );
}

/// Ensure that spacetime build properly catches getrandom usage
/// Standalone-only: this validates local CLI build diagnostics without publishing a module.
#[test]
fn test_detect_getrandom() {
    let output = build_rust_module(MODULE_CODE_GETRANDOM, r#"rand = "0.8""#);
    assert!(!output.status.success(), "Expected build to fail with getrandom");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("getrandom usage detected"),
        "Expected 'getrandom usage detected' in stderr, got: {}",
        stderr
    );
}
