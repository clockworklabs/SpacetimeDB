mod module_bindings;

use module_bindings::*;
use spacetimedb_sdk::{
    disconnect,
    identity::{credentials, identity, once_on_connect},
    once_on_disconnect, once_on_subscription_applied, subscribe,
    table::TableType,
};

use test_counter::TestCounter;

const LOCALHOST: &str = "http://localhost:3000";

fn db_name_or_panic() -> String {
    std::env::var("SPACETIME_SDK_TEST_DB_NAME").expect("Failed to read db name from env")
}

fn main() {
    let test_counter = TestCounter::new();
    let subscribe_result = test_counter.add_test("subscribe");
    let sub_applied_one_row_result = test_counter.add_test("connected_row");
    let connect_result = test_counter.add_test("connect");

    once_on_subscription_applied(move || {
        let check = || {
            anyhow::ensure!(Connected::count() == 1);
            if let Some(row) = Connected::iter().next() {
                anyhow::ensure!(row.identity == identity().unwrap());
            } else {
                anyhow::bail!("Expected one row but Connected::iter().next() returned None");
            }
            Ok(())
        };
        sub_applied_one_row_result(check());
    });
    once_on_connect(move |_, _| {
        subscribe_result(subscribe(&["SELECT * FROM Connected;"]));
    });

    connect_result(connect(LOCALHOST, &db_name_or_panic(), None));

    test_counter.wait_for_all();

    let test_counter = TestCounter::new();
    let disconnect_result = test_counter.add_test("disconnect");
    once_on_disconnect(move || {
        disconnect_result(Ok(()));
    });
    disconnect();
    test_counter.wait_for_all();

    let test_counter = TestCounter::new();
    let subscribe_result = test_counter.add_test("subscribe");
    let sub_applied_one_row_result = test_counter.add_test("disconnected_row");
    let connect_result = test_counter.add_test("connect");

    once_on_subscription_applied(move || {
        let check = || {
            anyhow::ensure!(Disconnected::count() == 1);
            if let Some(row) = Disconnected::iter().next() {
                anyhow::ensure!(row.identity == identity().unwrap());
            } else {
                anyhow::bail!("Expected one row but Disconnected::iter().next() returned None");
            }
            Ok(())
        };
        sub_applied_one_row_result(check());
    });
    once_on_connect(move |_, _| {
        subscribe_result(subscribe(&["SELECT * FROM Disconnected;"]));
    });

    connect_result(connect(LOCALHOST, &db_name_or_panic(), Some(credentials().unwrap())));

    test_counter.wait_for_all();
}
