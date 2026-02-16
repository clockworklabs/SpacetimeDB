// Wrap these tests in a `mod` whose name contains `csharp`
// so that we can run tests with `--skip csharp` in environments without dotnet installed.
use serial_test::serial;
use spacetimedb_sats::raw_identifier::RawIdentifier;
use spacetimedb_schema::auto_migrate::{ponder_auto_migrate, AutoMigrateStep};
use spacetimedb_schema::def::{
    ColumnDef, ConstraintDef, IndexDef, ModuleDef, ModuleDefLookup as _, ProcedureDef, ReducerDef, ScheduleDef,
    ScopedTypeName, SequenceDef, TableDef, TypeDef, ViewColumnDef, ViewDef,
};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::reducer_name::ReducerName;
use spacetimedb_testing::modules::{CompilationMode, CompiledModule};

fn get_normalized_schema(module_name: &str) -> ModuleDef {
    let module = CompiledModule::compile(module_name, CompilationMode::Debug);
    module.extract_schema_blocking()
}

fn assert_identical_modules(module_name_prefix: &str, lang_name: &str, suffix: &str) {
    let rs = get_normalized_schema(module_name_prefix);
    let cs = get_normalized_schema(&format!("{module_name_prefix}-{suffix}"));
    let mut diff = ponder_auto_migrate(&cs, &rs)
        .unwrap_or_else(|e| panic!("could not compute a diff between Rust and {lang_name}: {e:?}"))
        .steps;

    // In any migration plan, all `RowLevelSecurityDef`s are ALWAYS removed and
    // re-added to ensure the core engine reinintializes the policies.
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
        )
    });

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
fn test_case_converted_names() {
    let module_def: ModuleDef = get_normalized_schema("module-test");

    //  println!("Types {:?}", module_def.lookup::<TableDef>::(Identifier::for_test("person")).unwrap().columns().collect::<Vec<_>>());

    // println!("Types space {:?}", module_def.typespace());

    // Test Tables
    let table_names = [
        // canonical name, accessor name
        ("test_a", "TestATable"),
    ];
    for (name, accessor) in table_names {
        let def = TableDef::lookup(&module_def, &Identifier::for_test(name));

        assert!(def.is_some(), "Table '{}' not found", name);

        assert_eq!(&*def.unwrap().accessor_name, &*accessor, "Table '{}' not found", name);
    }

    // Test Reducers
    let reducer_names = ["list_over_age", "repeating_test"];
    for name in reducer_names {
        assert!(
            ReducerDef::lookup(&module_def, &ReducerName::for_test(name)).is_some(),
            "Reducer '{}' not found",
            name
        );
    }

    // Test Procedures
    let procedure_names = ["get_my_test_via_http"];
    for name in procedure_names {
        assert!(
            ProcedureDef::lookup(&module_def, &Identifier::for_test(name)).is_some(),
            "Procedure '{}' not found",
            name
        );
    }

    //  Test Views
    let view_names = ["my_player"];
    for name in view_names {
        assert!(
            ViewDef::lookup(&module_def, &Identifier::for_test(name)).is_some(),
            "View '{}' not found",
            name
        );
    }

    // Test Types
    let type_names = [
        // types are Pascal case
        "TestB", "Person",
    ];
    for name in type_names {
        assert!(
            TypeDef::lookup(&module_def, &ScopedTypeName::new([].into(), Identifier::for_test(name))).is_some(),
            "Type '{}' not found",
            name
        );
    }

    // Test Indexes (using lookup via stored_in_table_def)
    let index_names = [
        // index name should be generated from canonical name
        "test_a_x_idx_btree",
        "person_id_idx_btree",
    ];
    for index_name in index_names {
        assert!(
            IndexDef::lookup(&module_def, &RawIdentifier::new(index_name)).is_some(),
            "Index '{}' not found",
            index_name
        );
    }

    // Test Constraints
    let constraint_names = ["person_id_key"];
    for constraint_name in constraint_names {
        assert!(
            ConstraintDef::lookup(&module_def, &RawIdentifier::new(constraint_name)).is_some(),
            "Constraint '{}' not found",
            constraint_name
        );
    }

    // Test Sequences
    let sequence_names = ["person_id_seq"];
    for sequence_name in sequence_names {
        assert!(
            SequenceDef::lookup(&module_def, &RawIdentifier::new(sequence_name)).is_some(),
            "Sequence '{}' not found",
            sequence_name
        );
    }

    // Test Schedule
    let schedule_name = "repeating_test_arg_sched";
    assert!(
        ScheduleDef::lookup(&module_def, &Identifier::for_test(schedule_name)).is_some(),
        "Schedule '{}' not found",
        schedule_name
    );

    // Test Columns (using composite key: table_name, column_name)
    // Id has bigger case in accessor
    let column_names = [("person", "id")];
    for (table_name, col_name) in column_names {
        assert!(
            ColumnDef::lookup(
                &module_def,
                (&Identifier::for_test(table_name), &Identifier::for_test(col_name))
            )
            .is_some(),
            "Column '{}.{}' not found",
            table_name,
            col_name
        );
    }
}
