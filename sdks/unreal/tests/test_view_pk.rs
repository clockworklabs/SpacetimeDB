mod sdk_unreal_harness;
use sdk_unreal_harness::{make_test_with_suite, TestSuite};

use serial_test::serial;
use std::env;

const SDK_TEST_SUITE: TestSuite = TestSuite {
    module: "sdk-test-view-pk",
    client_root: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/TestViewPkClient"),
    unreal_module: "TestViewPkClient",
    uproject_file: "TestViewPkClient.uproject",
};

fn make_test(test_name: &str) -> spacetimedb_testing::sdk::Test {
    make_test_with_suite(&SDK_TEST_SUITE, test_name)
}

#[test]
#[serial(ViewPkGroup)]
fn unreal_view_pk_query_builder_direct_sources() {
    make_test("ViewPkQueryBuilderDirectSourcesTest").run();
}

#[test]
#[serial(ViewPkGroup)]
fn unreal_view_pk_query_builder_semijoin() {
    make_test("ViewPkQueryBuilderSemijoinTest").run();
}

#[test]
#[serial(ViewPkGroup)]
fn unreal_view_pk_subscribe_all_tables() {
    make_test("ViewPkSubscribeAllTablesTest").run();
}

#[test]
#[serial(ViewPkGroup)]
fn unreal_view_pk_runtime_update_pairing() {
    make_test("ViewPkRuntimeUpdatePairingTest").run();
}

#[test]
#[serial(ViewPkGroup)]
fn unreal_view_pk_blueprint_query_builder_flow() {
    make_test("ViewPkBlueprintQueryBuilderFlowTest").run();
}
