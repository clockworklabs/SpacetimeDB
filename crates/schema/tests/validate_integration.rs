//! Test that our module validation succeeds with the existing v8 modules in-codebase.

//! Validate a module.
//! The name should refer to a path in the `modules` directory of this repo.

use spacetimedb_cli::generate::extract_descriptions;
use spacetimedb_lib::{db::raw_def::v9::RawModuleDefV9, ser::serde::SerializeWrapper, RawModuleDef};
use spacetimedb_primitives::TableId;
use spacetimedb_schema::{
    def::{ModuleDef, TableDef},
    identifier::Identifier,
    schema::TableSchema,
};
use spacetimedb_testing::modules::{CompiledModule, ReleaseLevel};

const TEST_TABLE_ID: TableId = TableId(1337);

#[allow(clippy::disallowed_macros)] // LET ME PRINTLN >:(
fn validate_module(module_name: &str) {
    let module = CompiledModule::compile(module_name, ReleaseLevel::Debug);
    let raw_module_def: RawModuleDef =
        extract_descriptions(module.path()).expect("failed to extract module descriptions");
    let RawModuleDef::V8BackCompat(raw_module_def) = raw_module_def else {
        panic!("no more v8 - rewrite to v9")
    };

    // v8 -> ModuleDef
    let result = ModuleDef::try_from(raw_module_def.clone());

    // we don't use `expect` here because we want to format via `Display`, not `Debug`.
    let result = match result {
        Ok(result) => result,
        Err(err) => panic!("Module {} is invalid: \n{}", module_name, err),
    };

    // (ModuleDef -> v9 -> ModuleDef) == noop
    let result_as_raw: RawModuleDefV9 = result.clone().into();
    let result_from_raw = ModuleDef::try_from(result_as_raw).expect("failed to convert back to ModuleDef");
    assert_identical(result.clone(), result_from_raw);

    let mut tables = vec![];

    // (v8 -> ModuleDef -> TableSchema) == (v8 -> TableSchema)
    let mut failed = false;
    for table in raw_module_def.tables.into_iter() {
        let name = Identifier::new(table.schema.table_name.clone()).expect("already validated");
        let new_def: &TableDef = result.lookup(&name).expect("already validated");

        #[allow(deprecated)]
        let mut schema_old_path = TableSchema::from_def(TEST_TABLE_ID, table.schema);
        let mut schema_new_path = TableSchema::from_module_def(new_def, TEST_TABLE_ID);

        schema_old_path.janky_fix_column_defs(&result);
        schema_old_path.normalize();
        schema_new_path.janky_fix_column_defs(&result);
        schema_new_path.normalize();

        if schema_old_path != schema_new_path {
            failed = true;
            eprintln!("Mismatched TableSchemas: Old path:\n{schema_old_path:#?}\nNew path:\n{schema_new_path:#?}");
        }

        schema_old_path.validated().expect("TableSchema is invalid");
        tables.push(schema_new_path.validated().expect("TableSchema is invalid"));
    }
    if failed {
        panic!("TableSchemas mismatched");
    }
}

/// Assert that two ModuleDefs are identical.
/// TODO: implement better ModuleDef comparison. This just checks that their serialized forms are the same,
/// which is fine for this test, but we need a more relaxed comparison in general.
/// Allowing typespace permutations, etc.
fn assert_identical(module_def_1: ModuleDef, module_def_2: ModuleDef) {
    let module_def_1: RawModuleDefV9 = module_def_1.into();
    let module_def_2: RawModuleDefV9 = module_def_2.into();
    let s1 = serde_json::to_string_pretty(&SerializeWrapper::new(&module_def_1)).unwrap();
    let s2 = serde_json::to_string_pretty(&SerializeWrapper::new(&module_def_2)).unwrap();
    assert_eq!(s1, s2, "ModuleDefs are not identical");
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
fn validate_cs_modules() {
    // These need to be called in sequence because running them in parallel locks the codegen DLLs
    // which causes a compilation failure. Thanks, .NET
    validate_module("sdk-test-cs");
    validate_module("sdk-test-connect-disconnect-cs");
    validate_module("spacetimedb-quickstart-cs");
}

#[test]
fn validate_sdk_test_connect_disconnect() {
    validate_module("sdk-test-connect-disconnect");
}

#[test]
fn validate_spacetimedb_quickstart() {
    validate_module("spacetimedb-quickstart");
}
