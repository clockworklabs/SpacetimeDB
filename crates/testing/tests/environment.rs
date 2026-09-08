//! Actual module calls exercise environment ABI, bindings, and snapshot semantics.
use serial_test::serial;
use spacetimedb::db::environment;
use spacetimedb::host::FunctionArgs;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_lib::{bsatn, sats::product, AlgebraicValue, Identity};
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, DEFAULT_CONFIG};

fn exercise_fixture(name: &str) {
    CompiledModule::compile(name, CompilationMode::Debug).with_module_async(DEFAULT_CONFIG, |handle| async move {
        let module = handle.client.module();
        let db = module.relational_db();
        for (key, expected) in [
            ("MISSING", None),
            ("EMPTY", Some("".to_string())),
            ("UTF8", Some("héllo 🌍".to_string())),
            ("NUL", Some("before\0after".to_string())),
            ("MAXIMUM", Some("é".repeat(4096))),
        ] {
            if let Some(value) = &expected {
                db.with_auto_commit(Workload::Internal, |tx| environment::set(db, tx, key, value))
                    .unwrap();
            }
            let args = product![key, expected.clone()];
            let result = module
                .call_reducer(
                    Identity::ZERO,
                    None,
                    None,
                    None,
                    None,
                    "expect_environment",
                    FunctionArgs::Bsatn(bsatn::to_vec(&args).unwrap().into()),
                )
                .await;
            let result = result
                .map_err(anyhow::Error::from)
                .and_then(|r| r.outcome.into_result());
            assert!(
                result.is_ok(),
                "{name} {key}: {result:?}; module log: {}",
                handle.read_log(None).await
            );
            let read = || FunctionArgs::Bsatn(bsatn::to_vec(&product![key]).unwrap().into());
            let result = module
                .call_procedure(Identity::ZERO, None, None, "read_environment", read())
                .await
                .result
                .unwrap()
                .return_val;
            assert_eq!(result, AlgebraicValue::from(expected.clone()));
            if expected.is_some() {
                db.with_auto_commit(Workload::Internal, |tx| environment::set(db, tx, key, "updated"))
                    .unwrap();
                let result = module
                    .call_procedure(Identity::ZERO, None, None, "read_environment", read())
                    .await
                    .result
                    .unwrap()
                    .return_val;
                assert_eq!(result, AlgebraicValue::from(Some("updated".to_string())));
                db.with_auto_commit(Workload::Internal, |tx| environment::delete(db, tx, key))
                    .unwrap();
                let result = module
                    .call_procedure(Identity::ZERO, None, None, "read_environment", read())
                    .await
                    .result
                    .unwrap()
                    .return_val;
                assert_eq!(result, AlgebraicValue::from(None::<String>));
            }
        }
        if name == "environment-test" {
            db.with_auto_commit(Workload::Internal, |tx| {
                environment::set(db, tx, "LIMIT", &"x".repeat(8192))
            })
            .unwrap();
            for _ in 0..2 {
                module
                    .call_reducer(
                        Identity::ZERO,
                        None,
                        None,
                        None,
                        None,
                        "bounded_environment_sources",
                        FunctionArgs::Nullary,
                    )
                    .await
                    .unwrap()
                    .outcome
                    .into_result()
                    .unwrap();
            }
        }
        let args = product!["A=B", None::<String>];
        let result = module
            .call_reducer(
                Identity::ZERO,
                None,
                None,
                None,
                None,
                "expect_environment",
                FunctionArgs::Bsatn(bsatn::to_vec(&args).unwrap().into()),
            )
            .await;
        assert!(result.is_err() || result.unwrap().outcome.into_result().is_err());
    });
}

#[test]
#[serial]
fn rust_environment_reads_are_not_cached_and_preserve_missing_empty_utf8_and_nul() {
    exercise_fixture("environment-test");
}

#[test]
#[serial]
fn typescript_environment_reads_are_not_cached_and_preserve_missing_empty_utf8_and_nul() {
    exercise_fixture("module-test-ts");
}

#[test]
#[serial]
fn cpp_environment_reads_are_not_cached_and_preserve_missing_empty_utf8_and_nul() {
    exercise_fixture("module-test-cpp");
}

#[test]
#[serial]
fn csharp_environment_reads_are_not_cached_and_preserve_missing_empty_utf8_and_nul() {
    exercise_fixture("module-test-cs");
}
