//! Check `is_internal()` in real Rust and C# Wasm modules:
//! it is true exactly when the sender is the database.
use serial_test::serial;
use spacetimedb::host::FunctionArgs;
use spacetimedb_lib::{AlgebraicValue, Identity};
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, DEFAULT_CONFIG};
use std::time::Duration;

#[test]
#[serial]
fn rust_is_internal_is_whether_the_sender_is_the_database() {
    check_is_internal("is-internal-test");
}

#[test]
#[serial]
fn csharp_is_internal_is_whether_the_sender_is_the_database() {
    check_is_internal("is-internal-test-cs");
}

fn check_is_internal(module: &str) {
    // Publishing runs `init`, which checks that its sender, the owner, is not internal.
    CompiledModule::compile(module, CompilationMode::Debug).with_module_async(DEFAULT_CONFIG, |handle| async move {
        let module = handle.client.module();
        for sender in [Identity::ZERO, Identity::ONE, handle.db_identity] {
            module
                .call_reducer(sender, None, None, None, None, "check", FunctionArgs::Nullary)
                .await
                .unwrap()
                .outcome
                .into_result()
                .unwrap();
            let result = module
                .call_procedure(sender, None, None, "check_procedure", FunctionArgs::Nullary)
                .await;
            assert_eq!(result.result.unwrap().return_val, AlgebraicValue::Bool(true));
        }
        module
            .call_reducer(
                Identity::ZERO,
                None,
                None,
                None,
                None,
                "schedule",
                FunctionArgs::Nullary,
            )
            .await
            .unwrap()
            .outcome
            .into_result()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let result = module
                    .call_procedure(Identity::ZERO, None, None, "scheduled_finished", FunctionArgs::Nullary)
                    .await;
                if result.result.unwrap().return_val == AlgebraicValue::Bool(true) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the scheduled reducer did not observe that it is internal");
    });
}
