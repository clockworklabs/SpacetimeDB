//! Exercise real Rust Wasm bindings and host admission without a server endpoint.
use serial_test::serial;
use spacetimedb::host::FunctionArgs;
use spacetimedb_lib::{AlgebraicValue, Identity};
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, DEFAULT_CONFIG};
use std::time::Duration;

#[test]
#[serial]
fn wasm_invocation_flags_do_not_infer_authority_from_identity_or_connection_absence() {
    CompiledModule::compile("invocation-flags-test", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |handle| async move {
            let module = handle.client.module();
            for sender in [Identity::ZERO, Identity::ONE, handle.db_identity] {
                module
                    .call_reducer(sender, None, None, None, None, "external", FunctionArgs::Nullary)
                    .await
                    .unwrap()
                    .outcome
                    .into_result()
                    .unwrap();
                for name in ["internal", "init", "scheduled"] {
                    assert!(module
                        .call_reducer(sender, None, None, None, None, name, FunctionArgs::Nullary)
                        .await
                        .is_err());
                }
                let result = module
                    .call_procedure(sender, None, None, "external_procedure", FunctionArgs::Nullary)
                    .await;
                assert_eq!(result.result.unwrap().return_val, AlgebraicValue::Bool(true));
                assert!(module
                    .call_procedure(sender, None, None, "internal_procedure", FunctionArgs::Nullary)
                    .await
                    .result
                    .is_err());
                assert_eq!(
                    module
                        .call_reducer(sender, None, None, None, None, "private", FunctionArgs::Nullary)
                        .await
                        .is_ok(),
                    sender == Identity::ZERO,
                );
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
            .expect("the real scheduled reducer did not observe trusted internal authority");
        },
    );
}
