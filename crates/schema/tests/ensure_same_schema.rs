// Wrap these tests in a `mod` whose name contains `csharp`
// so that we can run tests with `--skip csharp` in environments without dotnet installed.
use serial_test::serial;
use spacetimedb_schema::auto_migrate::{ponder_auto_migrate, AutoMigrateStep};
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_schema::identifier::NamespacePath;
use spacetimedb_testing::modules::{CompilationMode, CompiledModule};

fn get_normalized_schema(module_name: &str) -> ModuleDef {
    let module = CompiledModule::compile(module_name, CompilationMode::Debug);
    module.extract_schema_blocking()
}

/// The namespace a step's item lives in, if the step refers to a named item.
///
/// Returns `None` for steps whose payload is not an item key (row-level security, which is
/// keyed by SQL text, and `DisconnectAllUsers`, which has no payload).
fn step_namespace<'a, 'def>(step: &'a AutoMigrateStep<'def>) -> Option<&'a NamespacePath> {
    match step {
        AutoMigrateStep::RemoveSchedule((ns, _))
        | AutoMigrateStep::RemoveView((ns, _))
        | AutoMigrateStep::RemoveTable((ns, _))
        | AutoMigrateStep::ChangeColumns((ns, _))
        | AutoMigrateStep::ReschemaEventTable((ns, _))
        | AutoMigrateStep::AddColumns((ns, _))
        | AutoMigrateStep::AddTable((ns, _))
        | AutoMigrateStep::AddSchedule((ns, _))
        | AutoMigrateStep::AddView((ns, _))
        | AutoMigrateStep::ChangeAccess((ns, _))
        | AutoMigrateStep::ChangePrimaryKey((ns, _))
        | AutoMigrateStep::UpdateView((ns, _))
        | AutoMigrateStep::RemoveIndex((ns, _))
        | AutoMigrateStep::RemoveConstraint((ns, _))
        | AutoMigrateStep::RemoveSequence((ns, _))
        | AutoMigrateStep::ChangeIndexSourceName((ns, _))
        | AutoMigrateStep::ChangeTableAccessorName((ns, _))
        | AutoMigrateStep::ChangeColumnAccessorName((ns, _), _)
        | AutoMigrateStep::AddIndex((ns, _))
        | AutoMigrateStep::AddConstraint((ns, _))
        | AutoMigrateStep::AddSequence((ns, _)) => Some(ns),
        AutoMigrateStep::AddRowLevelSecurity(_)
        | AutoMigrateStep::RemoveRowLevelSecurity(_)
        | AutoMigrateStep::DisconnectAllUsers => None,
    }
}

fn assert_identical_modules(module_name_prefix: &str, lang_name: &str, suffix: &str) {
    let rs = get_normalized_schema(module_name_prefix);
    let cs = get_normalized_schema(&format!("{module_name_prefix}-{suffix}"));
    let mut diff = ponder_auto_migrate(&cs, &rs)
        .unwrap_or_else(|e| panic!("could not compute a diff between Rust and {lang_name}: {e:?}"))
        .steps;

    // In any migration plan, all `RowLevelSecurityDef`s are ALWAYS removed and
    // re-added to ensure the core engine reinitializes the policies.
    // This is slightly silly (and arguably should be hidden inside `core`),
    // but for now, we just ignore these steps and manually compare the `RowLevelSecurityDef`s.
    diff.retain(|step| {
        !matches!(
            step,
            AutoMigrateStep::AddRowLevelSecurity(_) | AutoMigrateStep::RemoveRowLevelSecurity(_)
        )
    });

    // TODO: Remove this once we have view bindings for C# and TypeScript
    diff.retain(|step| {
        !matches!(
            step,
            AutoMigrateStep::DisconnectAllUsers
                | AutoMigrateStep::AddView(_)
                | AutoMigrateStep::RemoveView(_)
                | AutoMigrateStep::UpdateView(_)
                | AutoMigrateStep::ChangeIndexSourceName(_)
                | AutoMigrateStep::ChangeTableAccessorName(_)
                | AutoMigrateStep::ChangeColumnAccessorName(_, _)
        )
    });

    // TODO: Remove this once Rust and C# support submodules.
    // Only TypeScript can mount submodules today, so its modules emit items in a namespace
    // that the other languages cannot produce. Ignoring those steps keeps the root-level
    // schema comparison meaningful instead of disabling it wholesale.
    diff.retain(|step| step_namespace(step).is_none_or(|ns| ns.is_empty()));

    assert!(
        diff.is_empty(),
        "Rust and {lang_name} modules are not identical. Here are the steps to migrate from {lang_name} to Rust: {diff:#?}"
    );

    let mut rls_rs = rs.row_level_security().collect::<Vec<_>>();
    rls_rs.sort();
    let mut rls_cs = cs.row_level_security().collect::<Vec<_>>();
    rls_cs.sort();
    assert_eq!(
        rls_rs, rls_cs,
        "Rust and {lang_name} modules are not identical: different row level security policies"
    )
}

macro_rules! declare_tests {
        ($($name:ident => $path:literal,)*) => {
            mod ensure_same_schema_rust_csharp {
                use super::*;
                $(
                    #[test]
                    #[serial]
                    fn $name() {
                        super::assert_identical_modules($path, "C#", "cs");
                    }
                )*
            }
            mod ensure_same_schema_rust_typescript {
                use super::*;
                $(
                    #[test]
                    #[serial]
                    fn $name() {
                        super::assert_identical_modules($path, "typescript", "ts");
                    }
                )*
            }
        }
    }

declare_tests! {
    benchmarks => "benchmarks",
    module_test => "module-test",
    sdk_test_connect_disconnect => "sdk-test-connect-disconnect",
    sdk_test => "sdk-test",
}

#[test]
#[serial]
fn ensure_same_schema_rust_csharp_benchmarks() {
    assert_identical_modules("benchmarks", "C#", "cs");
}

#[test]
#[serial]
fn esure_same_schema_case_conversion() {
    assert_identical_modules("sdk-test-case-conversion", "", "ts");
}
