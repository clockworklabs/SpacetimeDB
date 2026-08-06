use std::panic::{catch_unwind, take_hook, AssertUnwindSafe};

use serial_test::serial;
use spacetimedb_testing::sdk::Test;

const TEST_CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/sdk-test-client");
const CONNECT_DISCONNECT_CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/connect-disconnect-client");
const PROCEDURE_CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/procedure-client");
const VIEW_PK_CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/view-pk-client");
const PROCEDURAL_VIEW_PK_CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/procedural-view-pk-client");

fn make_test(subcommand: &str) -> Test {
    Test::builder()
        .with_name(format!("csharp-client-{subcommand}"))
        .with_module("sdk-test-cs")
        .with_client(TEST_CLIENT)
        .with_language("csharp")
        .with_bindings_dir("module_bindings")
        .with_compile_command("bash ../build-client.sh")
        .with_run_command(format!("dotnet ./bin~/Debug/net8.0/sdk-test-client.dll {subcommand}"))
        .build()
}

fn make_procedure_test(subcommand: &str) -> Test {
    Test::builder()
        .with_name(format!("csharp-client-{subcommand}"))
        .with_module("sdk-test-procedure-cs")
        .with_client(PROCEDURE_CLIENT)
        .with_language("csharp")
        .with_generate_private_items(true)
        .with_bindings_dir("module_bindings")
        .with_compile_command("bash ../build-client.sh")
        .with_run_command(format!("dotnet ./bin~/Debug/net8.0/procedure-client.dll {subcommand}"))
        .build()
}

fn make_view_pk_test(subcommand: &str) -> Test {
    Test::builder()
        .with_name(format!("csharp-client-{subcommand}"))
        .with_module("sdk-test-view-pk-cs")
        .with_client(VIEW_PK_CLIENT)
        .with_language("csharp")
        .with_bindings_dir("module_bindings")
        .with_compile_command("bash ../build-client.sh")
        .with_run_command(format!("dotnet ./bin~/Debug/net8.0/view-pk-client.dll {subcommand}"))
        .build()
}

fn make_procedural_view_pk_test(subcommand: &str) -> Test {
    Test::builder()
        .with_name(format!("csharp-client-{subcommand}"))
        .with_module("sdk-test-procedural-view-pk-cs")
        .with_client(PROCEDURAL_VIEW_PK_CLIENT)
        .with_language("csharp")
        .with_bindings_dir("module_bindings")
        .with_compile_command("bash ../build-client.sh")
        .with_run_command(format!(
            "dotnet ./bin~/Debug/net8.0/procedural-view-pk-client.dll {subcommand}"
        ))
        .build()
}

fn run_expected_client_failure(test: Test, expected_message: &str) {
    let panic_hook = take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = catch_unwind(AssertUnwindSafe(|| test.run()));
    std::panic::set_hook(panic_hook);

    let panic = result.expect_err("Expected C# SDK harness client to fail");
    let message = if let Some(message) = panic.downcast_ref::<String>() {
        message.as_str()
    } else if let Some(message) = panic.downcast_ref::<&str>() {
        message
    } else {
        panic!("C# SDK harness failed with non-string panic");
    };

    assert!(
        message.contains("(running): Error running") && message.contains(expected_message),
        "Expected C# SDK harness client failure containing {expected_message:?}, got:\n{message}"
    );
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_primitive() {
    make_test("insert-primitive").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_subscribe_and_cancel() {
    make_test("subscribe-and-cancel").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_subscribe_and_unsubscribe() {
    make_test("subscribe-and-unsubscribe").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_subscription_error_smoke_test() {
    make_test("subscription-error-smoke-test").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_subscribe_all_select_star() {
    make_test("subscribe-all-select-star").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_delete_primitive() {
    make_test("delete-primitive").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_update_primitive() {
    make_test("update-primitive").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_identity() {
    make_test("insert-identity").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_caller_identity() {
    make_test("insert-caller-identity").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_delete_identity() {
    make_test("delete-identity").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_update_identity() {
    make_test("update-identity").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_connection_id() {
    make_test("insert-connection-id").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_caller_connection_id() {
    make_test("insert-caller-connection-id").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_delete_connection_id() {
    make_test("delete-connection-id").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_update_connection_id() {
    make_test("update-connection-id").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_timestamp() {
    make_test("insert-timestamp").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_call_timestamp() {
    make_test("insert-call-timestamp").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_uuid() {
    make_test("insert-uuid").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_call_uuid_v4() {
    make_test("insert-call-uuid-v4").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_call_uuid_v7() {
    make_test("insert-call-uuid-v7").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_delete_uuid() {
    make_test("delete-uuid").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_update_uuid() {
    make_test("update-uuid").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_on_reducer() {
    make_test("on-reducer").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_vec() {
    make_test("insert-vec").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_option_some() {
    make_test("insert-option-some").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_option_none() {
    make_test("insert-option-none").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_struct() {
    make_test("insert-struct").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_simple_enum() {
    make_test("insert-simple-enum").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_enum_with_payload() {
    make_test("insert-enum-with-payload").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_fail_reducer() {
    make_test("fail-reducer").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_delete_large_table() {
    make_test("insert-delete-large-table").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_primitives_as_strings() {
    make_test("insert-primitives-as-strings").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_should_fail() {
    run_expected_client_failure(
        make_test("should-fail"),
        "intentional failure for harness should_panic coverage",
    );
}

#[test]
#[serial(CsharpSdk)]
fn csharp_reauth() {
    make_test("reauth").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_reconnect_different_connection_id() {
    make_test("reconnect-different-connection-id").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_caller_always_notified() {
    make_test("caller-always-notified").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_caller_alice_receives_reducer_callback_but_not_bob() {
    make_test("caller-alice-receives-reducer-callback-but-not-bob").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_row_deduplication() {
    make_test("row-deduplication").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_row_deduplication_join_r_and_s() {
    make_test("row-deduplication-join-r-and-s").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_row_deduplication_r_join_s_and_r_joint() {
    make_test("row-deduplication-r-join-s-and-r-joint").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_test_lhs_join_update() {
    make_test("test-lhs-join-update").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_test_lhs_join_update_disjoint_queries() {
    make_test("test-lhs-join-update-disjoint-queries").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_test_intra_query_bag_semantics_for_join() {
    make_test("test-intra-query-bag-semantics-for-join").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_two_different_compression_algos() {
    make_test("two-different-compression-algos").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_test_parameterized_subscription() {
    make_test("test-parameterized-subscription").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_test_rls_subscription() {
    make_test("test-rls-subscription").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_pk_simple_enum() {
    make_test("pk-simple-enum").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_indexed_simple_enum() {
    make_test("indexed-simple-enum").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_overlapping_subscriptions() {
    make_test("overlapping-subscriptions").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_sorted_uuids_insert() {
    make_test("sorted-uuids-insert").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_procedure_return_values() {
    make_procedure_test("procedure-return-values").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_procedure_observe_panic() {
    make_procedure_test("procedure-observe-panic").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_with_tx_commit() {
    make_procedure_test("insert-with-tx-commit").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_insert_with_tx_rollback() {
    make_procedure_test("insert-with-tx-rollback").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_procedure_http_ok() {
    make_procedure_test("procedure-http-ok").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_procedure_http_err() {
    make_procedure_test("procedure-http-err").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_schedule_procedure() {
    make_procedure_test("schedule-procedure").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_view_pk_on_update() {
    make_view_pk_test("view-pk-on-update").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_view_pk_join_query_builder() {
    make_view_pk_test("view-pk-join-query-builder").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_view_pk_semijoin_two_sender_views_query_builder() {
    make_view_pk_test("view-pk-semijoin-two-sender-views-query-builder").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_sender_scoped_procedural_pk_view() {
    make_procedural_view_pk_test("sender-scoped-pk-view").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_procedural_view_pk_left_semijoin() {
    make_procedural_view_pk_test("view-pk-left-semijoin").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_procedural_view_pk_right_semijoin() {
    make_procedural_view_pk_test("view-pk-right-semijoin").run();
}

#[test]
#[serial(CsharpSdk)]
fn csharp_connect_disconnect_callbacks() {
    Test::builder()
        .with_name("csharp-client-connect-disconnect-callbacks")
        .with_module("sdk-test-connect-disconnect-cs")
        .with_client(CONNECT_DISCONNECT_CLIENT)
        .with_language("csharp")
        .with_bindings_dir("module_bindings")
        .with_compile_command("bash ../build-client.sh")
        .with_run_command("dotnet ./bin~/Debug/net8.0/connect-disconnect-client.dll")
        .build()
        .run();
}
