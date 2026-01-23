//! Tests translated from smoketests/tests/detect_wasm_bindgen.py

use spacetimedb_smoketests::Smoketest;

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
#[test]
fn test_detect_wasm_bindgen() {
    let test = Smoketest::builder()
        .module_code(MODULE_CODE_WASM_BINDGEN)
        .extra_deps(r#"wasm-bindgen = "0.2""#)
        .autopublish(false)
        .build();

    let output = test.spacetime_build();
    assert!(!output.status.success(), "Expected build to fail with wasm_bindgen");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("wasm-bindgen detected"),
        "Expected 'wasm-bindgen detected' in stderr, got: {}",
        stderr
    );
}

/// Ensure that spacetime build properly catches getrandom usage
#[test]
fn test_detect_getrandom() {
    let test = Smoketest::builder()
        .module_code(MODULE_CODE_GETRANDOM)
        .extra_deps(r#"rand = "0.8""#)
        .autopublish(false)
        .build();

    let output = test.spacetime_build();
    assert!(!output.status.success(), "Expected build to fail with getrandom");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("getrandom usage detected"),
        "Expected 'getrandom usage detected' in stderr, got: {}",
        stderr
    );
}
