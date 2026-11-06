//! Guards the `brotli` dependency policy described in #6430.
//!
//! `brotli`'s default features include `ffi-api`, which exports the `Brotli*`
//! C symbols from any `cdylib` that links the SDK. A host process that also
//! loads a libcrypto built with Brotli support then binds those imports to the
//! extension, keeps it mapped after unload, and breaks hot reloading. The SDK
//! only uses the Rust API, so `ffi-api` must stay off.

use toml::Value;

const WORKSPACE_MANIFEST: &str = include_str!("../../../Cargo.toml");
const SDK_MANIFEST: &str = include_str!("../Cargo.toml");

#[test]
fn brotli_does_not_enable_ffi_api() {
    let workspace: Value = toml::from_str(WORKSPACE_MANIFEST).expect("workspace manifest is not valid TOML");
    let sdk: Value = toml::from_str(SDK_MANIFEST).expect("SDK manifest is not valid TOML");

    assert_eq!(
        sdk["dependencies"]["brotli"]["workspace"].as_bool(),
        Some(true),
        "the SDK must inherit `brotli` from the workspace so the `ffi-api` policy cannot be \
         overridden locally"
    );

    let brotli = &workspace["workspace"]["dependencies"]["brotli"];
    assert_eq!(
        brotli["default-features"].as_bool(),
        Some(false),
        "`brotli`'s default features include `ffi-api`, which exports `Brotli*` C symbols from \
         `cdylib` clients (#6430)"
    );
    let enables_ffi_api = brotli["features"]
        .as_array()
        .is_some_and(|features| features.iter().any(|feature| feature.as_str() == Some("ffi-api")));
    assert!(!enables_ffi_api, "`brotli` must not enable `ffi-api` (#6430)");
}
