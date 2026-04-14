mod sdk_unreal_harness;
use sdk_unreal_harness::{make_test_with_suite, TestSuite};

use serial_test::serial;
use std::env;

const SDK_TEST_SUITE: TestSuite = TestSuite {
    module: "sdk-test",
    client_root: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/TestClient"),
    unreal_module: "TestClient",
    uproject_file: "TestClient.uproject",
};

fn make_test(test_name: &str) -> spacetimedb_testing::sdk::Test {
    make_test_with_suite(&SDK_TEST_SUITE, test_name)
}

// Below shows examples on how to use the serial and parallel attributes for tests

//#[test]
//#[serial]
//fn test_serial_one() {
//  // Do things
//}

//#[test]
//#[serial(something)]
//fn test_serial_one() {
//  // Do things
//}

//#[test]
//#[parallel]
//fn test_parallel_another() {
//  // Do parallel things
//}

//48 tests → 6 groups → 8 per group (where possible)
//Gouping added for performace
// ---------------- GROUP 1 ----------------

#[test]
#[serial(Group1)]
fn unreal_insert_primitive() {
    make_test("InsertPrimitiveTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_subscribe_and_cancel() {
    make_test("SubscribeAndCancelTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_subscribe_and_unsubscribe() {
    make_test("SubscribeAndUnsubscribeTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_subscription_error_smoke_test() {
    make_test("SubscriptionErrorSmokeTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_delete_primitive() {
    make_test("DeletePrimitiveTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_update_primitive() {
    make_test("UpdatePrimitiveTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_insert_identity() {
    make_test("InsertIdentityTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_insert_caller_identity() {
    make_test("InsertCallerIdentityTest").run();
}

// ---------------- GROUP 2 ----------------
#[test]
#[serial(Group2)]
fn unreal_delete_identity() {
    make_test("DeleteIdentityTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_update_identity() {
    make_test("UpdateIdentityTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_insert_connection_id() {
    make_test("InsertConnectionIdTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_insert_caller_connection_id() {
    make_test("InsertCallerConnectionIdTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_delete_connection_id() {
    make_test("DeleteConnectionIdTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_update_connection_id() {
    make_test("UpdateConnectionIdTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_insert_timestamp() {
    make_test("InsertTimestampTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_insert_call_timestamp() {
    make_test("InsertCallTimestampTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_insert_call_uuid_v4() {
    make_test("InsertCallUuidV4Test").run();
}

#[test]
#[serial(Group2)]
fn unreal_insert_call_uuid_v7() {
    make_test("InsertCallUuidV7Test").run();
}

// ---------------- GROUP 3 ----------------
#[test]
#[serial(Group3)]
fn unreal_on_reducer() {
    make_test("OnReducerTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_fail_reducer() {
    make_test("FailReducerTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_insert_vec() {
    make_test("InsertVecTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_insert_option_some() {
    make_test("InsertOptionSomeTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_insert_option_none() {
    make_test("InsertOptionNoneTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_insert_struct() {
    make_test("InsertStructTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_insert_simple_enum() {
    make_test("InsertSimpleEnumTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_insert_enum_with_payload() {
    make_test("InsertEnumWithPayloadTest").run();
}

// ---------------- GROUP 4 ----------------
#[test]
#[serial(Group4)]
fn unreal_insert_delete_large_table() {
    make_test("InsertDeleteLargeTableTest").run();
}

#[test]
#[serial(Group4)]
fn unreal_insert_primitives_as_strings() {
    make_test("InsertPrimitivesAsStringsTest").run();
}

#[test]
#[serial(Group4)]
fn unreal_reauth() {
    make_test("ReauthPart1Test").run();
    make_test("ReauthPart2Test").run();
}

#[test]
#[should_panic]
#[serial(Group4)]
fn unreal_should_fail() {
    make_test("ShouldFailTest").run();
}

#[test]
#[serial(Group4)]
fn unreal_caller_always_notified() {
    make_test("CallerAlwaysNotifiedTest").run();
}

#[test]
#[serial(Group4)]
fn unreal_subscribe_all_select_star() {
    make_test("SubscribeAllSelectStarTest").run();
}

// ---------------- GROUP 5 ----------------
#[test]
#[serial(Group5)]
fn unreal_row_deduplication() {
    make_test("RowDeduplicationTest").run();
}

#[test]
#[serial(Group5)]
fn unreal_row_deduplication_join_r_and_s() {
    make_test("RowDeduplicationJoinRAndSTest").run();
}

#[test]
#[serial(Group5)]
fn unreal_row_deduplication_r_join_s_and_r_joint() {
    make_test("RowDeduplicationRJoinSAndRJoinTTest").run();
}

#[test]
#[serial(Group5)]
fn unreal_test_lhs_join_update() {
    make_test("LhsJoinUpdateTest").run();
}

#[test]
#[serial(Group5)]
fn unreal_test_lhs_join_update_disjoint_queries() {
    make_test("LhsJoinUpdateDisjointQueriesTest").run();
}

#[test]
#[serial(Group5)]
fn unreal_test_intra_query_bag_semantics_for_join() {
    make_test("IntraQueryBagSemanticsForJoinTest").run();
}

// ---------------- GROUP 6 ----------------
#[test]
#[serial(Group6)]
fn unreal_test_parameterized_subscription() {
    make_test("ParameterizedSubscriptionTest").run();
}

#[test]
#[serial(Group6)]
fn unreal_test_rls_subscription() {
    make_test("RlsSubscriptionTest").run();
}

#[test]
#[serial(Group6)]
fn unreal_pk_simple_enum() {
    make_test("PkSimpleEnumTest").run();
}

#[test]
#[serial(Group6)]
fn unreal_indexed_simple_enum() {
    make_test("IndexedSimpleEnumTest").run();
}

#[test]
#[serial(Group6)]
fn unreal_overlapping_subscriptions() {
    make_test("OverlappingSubscriptionsTest").run();
}

#[test]
#[serial(Group6)]
fn unreal_insert_result_okay() {
    make_test("InsertResultOkTest").run();
}
