mod sdk_unreal_harness;
use sdk_unreal_harness::{make_test_with_suite, TestSuite};

use serial_test::serial;
use std::env;

const SDK_TEST_SUITE: TestSuite = TestSuite {
    module: "sdk-test-procedure",
    client_root: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/TestProcClient"),
    unreal_module: "TestProcClient",
    uproject_file: "TestProcClient.uproject",
};

fn make_test(test_name: &str) -> spacetimedb_testing::sdk::Test {
    make_test_with_suite(&SDK_TEST_SUITE, test_name)
}

#[test]
#[serial(Group7)]
fn unreal_procedure_basic_test() {
    make_test("ProcedureBasicTest").run();
}

#[test]
#[serial(Group7)]
//exec_insert_with_tx_commit
fn unreal_procedure_insert_w_tx_commit() {
    make_test("ProcedureInsertTransactionCommitTest").run();
}

#[test]
#[serial(Group7)]
fn unreal_procedure_insert_w_tx_rollback() {
    make_test("ProcedureInsertTransactionRollbackTest").run();
}
