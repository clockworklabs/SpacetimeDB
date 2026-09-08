//! Actual module calls exercise environment ABI, bindings, and snapshot semantics.
use serial_test::serial;
use spacetimedb::host::{FunctionArgs, ModuleHost};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::{bsatn, sats::product, AlgebraicValue, Identity};
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, DEFAULT_CONFIG};

async fn sql(module: &ModuleHost, statement: String) -> Vec<spacetimedb_lib::ProductValue> {
    spacetimedb::sql::execute::run(
        module.relational_db().clone(),
        statement,
        AuthCtx::for_current(Identity::ZERO),
        Some(module.info.subscriptions.clone()),
        Some(module.clone()),
        &mut vec![],
    )
    .await
    .unwrap()
    .rows
}

async fn set_environment(module: &ModuleHost, key: &str, value: &str) {
    sql(module, format!("SET env.{key} = '{}'", value.replace('\'', "''"))).await;
}

fn exercise_fixture(name: &str) {
    CompiledModule::compile(name, CompilationMode::Debug).with_module_async(DEFAULT_CONFIG, |handle| async move {
        let module = handle.client.module();
        for (key, expected) in [
            ("MISSING", None),
            ("EMPTY", Some("".to_string())),
            ("UTF8", Some("héllo 🌍".to_string())),
            ("NUL", Some("before\0after".to_string())),
            ("MAXIMUM", Some("é".repeat(4096))),
        ] {
            if let Some(value) = &expected {
                set_environment(&module, key, value).await;
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
                set_environment(&module, key, "updated").await;
                let result = module
                    .call_procedure(Identity::ZERO, None, None, "read_environment", read())
                    .await
                    .result
                    .unwrap()
                    .return_val;
                assert_eq!(result, AlgebraicValue::from(Some("updated".to_string())));
                sql(&module, format!("DELETE env.{key}")).await;
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
            // This view first reads a missing key. Its dependency must survive
            // absence, and normal SQL mutations must invalidate its cached row.
            let read_view = || "SELECT * FROM environment_value".to_string();
            assert_eq!(sql(&module, read_view()).await, vec![product![None::<String>]]);
            set_environment(&module, "WATCHED", "first").await;
            assert_eq!(
                sql(&module, read_view()).await,
                vec![product![Some("first".to_string())]]
            );
            set_environment(&module, "WATCHED", "second").await;
            assert_eq!(
                sql(&module, read_view()).await,
                vec![product![Some("second".to_string())]]
            );
            sql(&module, "DELETE env.WATCHED".into()).await;
            assert_eq!(sql(&module, read_view()).await, vec![product![None::<String>]]);
            set_environment(&module, "LIMIT", &"x".repeat(8192)).await;
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
