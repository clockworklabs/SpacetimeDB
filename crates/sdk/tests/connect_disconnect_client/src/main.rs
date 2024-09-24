mod module_bindings;

use module_bindings::*;

use spacetimedb_sdk::{DbContext, Table};

use test_counter::TestCounter;

const LOCALHOST: &str = "http://localhost:3000";

fn db_name_or_panic() -> String {
    std::env::var("SPACETIME_SDK_TEST_DB_NAME").expect("Failed to read db name from env")
}

fn main() {
    let disconnect_test_counter = TestCounter::new();
    let disconnect_result = disconnect_test_counter.add_test("disconnect");

    let connect_test_counter = TestCounter::new();
    let connected_result = connect_test_counter.add_test("on_connect");
    let sub_applied_one_row_result = connect_test_counter.add_test("connected_row");

    let connection = DbConnection::builder()
        .with_module_name(db_name_or_panic())
        .with_uri(LOCALHOST)
        .on_connect_error(|e| panic!("on_connect_error: {e:?}"))
        .on_connect(move |ctx, _, _| {
            connected_result(Ok(()));
            ctx.subscription_builder()
                .on_error(|ctx| panic!("Subscription failed: {:?}", ctx.event))
                .on_applied(move |ctx| {
                    let check = || {
                        anyhow::ensure!(ctx.db.connected().count() == 1);
                        if let Some(_row) = ctx.db.connected().iter().next() {
                            // TODO: anyhow::ensure!(row.identity == ctx.identity().unwrap());
                        } else {
                            anyhow::bail!("Expected one row but Connected::iter().next() returned None");
                        }
                        Ok(())
                    };
                    sub_applied_one_row_result(check());
                })
                .subscribe(vec!["SELECT * FROM Connected;".to_string()]);
        })
        .on_disconnect(move |ctx, err| {
            assert!(
                !ctx.is_active(),
                "on_disconnect callback, but `ctx.is_active()` is true"
            );
            if let Some(err) = err {
                disconnect_result(Err(anyhow::anyhow!("{err:?}")));
            } else {
                disconnect_result(Ok(()));
            }
        })
        .build()
        .unwrap();

    let join_handle = connection.run_threaded();

    connect_test_counter.wait_for_all();

    connection.disconnect().unwrap();
    join_handle.join().unwrap();

    disconnect_test_counter.wait_for_all();

    let reconnect_test_counter = TestCounter::new();
    let reconnected_result = reconnect_test_counter.add_test("on_reconnect");
    let sub_applied_one_row_result = reconnect_test_counter.add_test("disconnected_row");

    let new_connection = DbConnection::builder()
        .on_connect_error(|e| panic!("on_connect_error: {e:?}"))
        .on_connect(move |_ctx, _, _| {
            reconnected_result(Ok(()));
        })
        .with_module_name(db_name_or_panic())
        .with_uri(LOCALHOST)
        .build()
        .unwrap();

    new_connection
        .subscription_builder()
        .on_applied(move |ctx| {
            let check = || {
                anyhow::ensure!(ctx.db.disconnected().count() == 1);
                if let Some(_row) = ctx.db.disconnected().iter().next() {
                    // TODO: anyhow::ensure!(row.identity == ctx.identity().unwrap());
                } else {
                    anyhow::bail!("Expected one row but Disconnected::iter().next() returned None");
                }
                Ok(())
            };
            sub_applied_one_row_result(check());
        })
        .on_error(|ctx| panic!("subscription on_error: {:?}", ctx.event))
        .subscribe(vec!["SELECT * FROM Disconnected;".to_string()]);

    new_connection.run_threaded();

    reconnect_test_counter.wait_for_all();
}
