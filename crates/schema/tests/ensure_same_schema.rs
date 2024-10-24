//! Test that our module validation succeeds with the existing v8 modules in-codebase.

//! Validate a module.
//! The name should refer to a path in the `modules` directory of this repo.

use pretty_assertions::assert_eq;
use serial_test::serial;
use spacetimedb_cli::generate::extract_descriptions;
use spacetimedb_lib::{db::raw_def::v9::RawModuleDefV9, RawModuleDef};
use spacetimedb_testing::modules::{CompilationMode, CompiledModule};

fn get_normalized_schema(module_name: &str) -> RawModuleDefV9 {
    let module = CompiledModule::compile(module_name, CompilationMode::Debug);
    let raw_module_def: RawModuleDef =
        extract_descriptions(module.path()).expect("failed to extract module descriptions");
    let RawModuleDef::V9(mut raw_module_def) = raw_module_def else {
        panic!("Expected V9 schema")
    };
    raw_module_def
}

fn assert_identical_modules(module_name_prefix: &str) {
    let rs = get_normalized_schema(module_name_prefix);
    let cs = get_normalized_schema(&format!("{module_name_prefix}-cs"));
    assert_eq!(rs, cs);
}

// These need to be called in sequence because running them in parallel locks the codegen DLLs
// which causes a compilation failure. Thanks, .NET

#[test]
#[serial]
fn spacetimedb_quickstart() {
    assert_identical_modules("spacetimedb-quickstart");
}

#[test]
#[serial]
fn sdk_test_connect_disconnect() {
    assert_identical_modules("sdk-test-connect-disconnect");
}

#[test]
#[serial]
fn sdk_test() {
    assert_identical_modules("sdk-test");
}
