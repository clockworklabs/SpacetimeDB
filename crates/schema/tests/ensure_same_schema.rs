use serial_test::serial;
use spacetimedb_cli::generate::extract_descriptions;
use spacetimedb_schema::auto_migrate::{ponder_auto_migrate, AutoMigrateStep};
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
    let mut diff = ponder_auto_migrate(&cs, &rs)
        .expect("could not compute a diff between Rust and C#")
        .steps;

    // There are always AddRowLevelSecurity / RemoveRowLevelSecurity steps,
    // to ensure the core engine reinitializes the policies.
    diff.retain(|step| {
        !matches!(
            step,
            AutoMigrateStep::AddRowLevelSecurity(_) | AutoMigrateStep::RemoveRowLevelSecurity(_)
        )
    });

    assert!(
        diff.is_empty(),
        "Rust and C# modules are not identical. Here are the steps to migrate from C# to Rust: {diff:#?}"
    );

    let mut rls_rs = rs.row_level_security().collect::<Vec<_>>();
    rls_rs.sort();
    let mut rls_cs = cs.row_level_security().collect::<Vec<_>>();
    rls_cs.sort();
    assert_eq!(
        rls_rs, rls_cs,
        "Rust and C# modules are not identical: different row level security policies"
    )
}

macro_rules! declare_tests {
    ($($name:ident => $path:literal,)*) => {
        $(
            #[test]
            #[serial]
            fn $name() {
                assert_identical_modules($path);
            }
        )*
    }
}

declare_tests! {
    module_test => "module-test",
    sdk_test_connect_disconnect => "sdk-test-connect-disconnect",
    sdk_test => "sdk-test",
    benchmarks => "benchmarks",
}
