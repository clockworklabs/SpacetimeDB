use spacetimedb_cli::generate::extract_descriptions;
use spacetimedb_schema::auto_migrate::ponder_auto_migrate;
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_testing::modules::{CompilationMode, CompiledModule};

fn get_normalized_schema(module_name: &str) -> ModuleDef {
    let module = CompiledModule::compile(module_name, CompilationMode::Debug);
    extract_descriptions(module.path())
        .expect("failed to extract module descriptions")
        .try_into()
        .expect("failed to convert raw module def to module def")
}

fn assert_identical_modules(module_name_prefix: &str) {
    let rs = get_normalized_schema(module_name_prefix);
    let cs = get_normalized_schema(&format!("{module_name_prefix}-cs"));
    let diff = ponder_auto_migrate(&cs, &rs)
        .expect("could not compute a diff between Rust and C#")
        .steps;
    assert!(
        diff.is_empty(),
        "Rust and C# modules are not identical. Here are the steps to migrate from C# to Rust: {diff:#?}"
    );
}

macro_rules! declare_tests {
    ($($name:ident => $path:literal,)*) => {
        $(
            #[test]
            fn $name() {
                assert_identical_modules($path);
            }
        )*
    }
}

declare_tests! {
    spacetimedb_quickstart => "spacetimedb-quickstart",
    sdk_test_connect_disconnect => "sdk-test-connect-disconnect",
    sdk_test => "sdk-test",
    benchmarks => "benchmarks",
}
