use serial_test::serial;
use spacetimedb_testing::sdk::Test;

const TEST_CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/sdk-test-client");
const CONNECT_DISCONNECT_CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/connect-disconnect-client");

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

#[test]
#[serial(CsharpSdk)]
fn insert_primitive() {
    make_test("insert-primitive").run();
}

#[test]
#[serial(CsharpSdk)]
fn subscribe_and_cancel() {
    make_test("subscribe-and-cancel").run();
}

#[test]
#[serial(CsharpSdk)]
fn subscribe_and_unsubscribe() {
    make_test("subscribe-and-unsubscribe").run();
}

#[test]
#[serial(CsharpSdk)]
fn subscription_error_smoke_test() {
    make_test("subscription-error-smoke-test").run();
}

#[test]
#[serial(CsharpSdk)]
fn subscribe_all_select_star() {
    make_test("subscribe-all-select-star").run();
}

#[test]
#[serial(CsharpSdk)]
fn delete_primitive() {
    make_test("delete-primitive").run();
}

#[test]
#[serial(CsharpSdk)]
fn update_primitive() {
    make_test("update-primitive").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_identity() {
    make_test("insert-identity").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_caller_identity() {
    make_test("insert-caller-identity").run();
}

#[test]
#[serial(CsharpSdk)]
fn delete_identity() {
    make_test("delete-identity").run();
}

#[test]
#[serial(CsharpSdk)]
fn update_identity() {
    make_test("update-identity").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_connection_id() {
    make_test("insert-connection-id").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_caller_connection_id() {
    make_test("insert-caller-connection-id").run();
}

#[test]
#[serial(CsharpSdk)]
fn delete_connection_id() {
    make_test("delete-connection-id").run();
}

#[test]
#[serial(CsharpSdk)]
fn update_connection_id() {
    make_test("update-connection-id").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_timestamp() {
    make_test("insert-timestamp").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_call_timestamp() {
    make_test("insert-call-timestamp").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_uuid() {
    make_test("insert-uuid").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_call_uuid_v4() {
    make_test("insert-call-uuid-v4").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_call_uuid_v7() {
    make_test("insert-call-uuid-v7").run();
}

#[test]
#[serial(CsharpSdk)]
fn delete_uuid() {
    make_test("delete-uuid").run();
}

#[test]
#[serial(CsharpSdk)]
fn update_uuid() {
    make_test("update-uuid").run();
}

#[test]
#[serial(CsharpSdk)]
fn on_reducer() {
    make_test("on-reducer").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_vec() {
    make_test("insert-vec").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_option_some() {
    make_test("insert-option-some").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_option_none() {
    make_test("insert-option-none").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_struct() {
    make_test("insert-struct").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_simple_enum() {
    make_test("insert-simple-enum").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_enum_with_payload() {
    make_test("insert-enum-with-payload").run();
}

#[test]
#[serial(CsharpSdk)]
fn fail_reducer() {
    make_test("fail-reducer").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_delete_large_table() {
    make_test("insert-delete-large-table").run();
}

#[test]
#[serial(CsharpSdk)]
fn insert_primitives_as_strings() {
    make_test("insert-primitives-as-strings").run();
}

#[test]
#[serial(CsharpSdk)]
#[should_panic]
fn should_fail() {
    make_test("should-fail").run();
}

#[test]
#[serial(CsharpSdk)]
fn reauth() {
    make_test("reauth").run();
}

#[test]
#[serial(CsharpSdk)]
fn reconnect_different_connection_id() {
    make_test("reconnect-different-connection-id").run();
}

#[test]
#[serial(CsharpSdk)]
fn caller_always_notified() {
    make_test("caller-always-notified").run();
}

#[test]
#[serial(CsharpSdk)]
fn caller_alice_receives_reducer_callback_but_not_bob() {
    make_test("caller-alice-receives-reducer-callback-but-not-bob").run();
}

#[test]
#[serial(CsharpSdk)]
fn row_deduplication() {
    make_test("row-deduplication").run();
}

#[test]
#[serial(CsharpSdk)]
fn row_deduplication_join_r_and_s() {
    make_test("row-deduplication-join-r-and-s").run();
}

#[test]
#[serial(CsharpSdk)]
fn row_deduplication_r_join_s_and_r_joint() {
    make_test("row-deduplication-r-join-s-and-r-joint").run();
}

#[test]
#[serial(CsharpSdk)]
fn test_lhs_join_update() {
    make_test("test-lhs-join-update").run();
}

#[test]
#[serial(CsharpSdk)]
fn test_lhs_join_update_disjoint_queries() {
    make_test("test-lhs-join-update-disjoint-queries").run();
}

#[test]
#[serial(CsharpSdk)]
fn test_intra_query_bag_semantics_for_join() {
    make_test("test-intra-query-bag-semantics-for-join").run();
}

#[test]
#[serial(CsharpSdk)]
fn two_different_compression_algos() {
    make_test("two-different-compression-algos").run();
}

#[test]
#[serial(CsharpSdk)]
fn test_parameterized_subscription() {
    make_test("test-parameterized-subscription").run();
}

#[test]
#[serial(CsharpSdk)]
fn test_rls_subscription() {
    make_test("test-rls-subscription").run();
}

#[test]
#[serial(CsharpSdk)]
fn pk_simple_enum() {
    make_test("pk-simple-enum").run();
}

#[test]
#[serial(CsharpSdk)]
fn indexed_simple_enum() {
    make_test("indexed-simple-enum").run();
}

#[test]
#[serial(CsharpSdk)]
fn overlapping_subscriptions() {
    make_test("overlapping-subscriptions").run();
}

#[test]
#[serial(CsharpSdk)]
fn sorted_uuids_insert() {
    make_test("sorted-uuids-insert").run();
}

#[test]
#[serial(CsharpSdk)]
fn connect_disconnect_callbacks() {
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
