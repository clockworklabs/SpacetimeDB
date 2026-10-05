mod sdk_unreal_harness;
use sdk_unreal_harness::{make_test_with_suite, TestSuite};

use serial_test::serial;
use std::env;

const SDK_TEST_SUITE: TestSuite = TestSuite {
    module: "sdk-test-view",
    client_root: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/TestViewClient"),
    unreal_module: "TestViewClient",
    uproject_file: "TestViewClient.uproject",
};

fn make_test(test_name: &str) -> spacetimedb_testing::sdk::Test {
    make_test_with_suite(&SDK_TEST_SUITE, test_name)
}

#[test]
#[serial(ViewGroup)]
fn unreal_view_query_builder_direct_sources() {
    make_test("ViewQueryBuilderDirectSourcesTest").run();
}

#[test]
#[serial(ViewGroup)]
fn unreal_view_subscribe_all_tables() {
    make_test("ViewSubscribeAllTablesTest").run();
}

#[test]
#[serial(ViewGroup)]
fn unreal_view_blueprint_query_builder_flow() {
    make_test("ViewBlueprintQueryBuilderFlowTest").run();
}

#[test]
#[serial(ViewGroup)]
fn unreal_view_blueprint_query_builder_runtime() {
    make_test("ViewBlueprintQueryBuilderRuntimeTest").run();
}
