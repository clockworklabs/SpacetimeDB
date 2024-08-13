//! Test that our module validation succeeds with the existing v8 modules in-codebase.

//! Validate a module.
//! The name should refer to a path in the `modules` directory of this repo.

use spacetimedb_cli::generate::extract_descriptions;
use spacetimedb_lib::RawModuleDefV8;
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_testing::modules::{CompilationMode, CompiledModule};

#[allow(clippy::disallowed_macros)] // LET ME PRINTLN >:(
fn validate_module(module_name: &str) {
    let module = CompiledModule::compile(module_name, CompilationMode::Debug);
    let raw_module_def: RawModuleDefV8 =
        extract_descriptions(module.path()).expect("failed to extract module descriptions");

    if let Err(err) = ModuleDef::try_from(raw_module_def) {
        // use `{}` so we get prettily-formatted errors
        panic!("Failed to validate module: {module_name}\n{err}");
    }
    // TODO: once we have the conversion TableDef -> TableSchema, go through and validate that the old path
    // from RawModuleDefV8 -> TableSchema is equivalent to the new path.
}

#[test]
fn validate_rust_wasm_test() {
    validate_module("rust-wasm-test");
}

#[test]
fn validate_sdk_test() {
    validate_module("sdk-test");
}

#[test]
fn validate_sdk_test_cs() {
    validate_module("sdk-test-cs");
}
