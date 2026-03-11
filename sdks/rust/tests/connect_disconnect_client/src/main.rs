pub(crate) mod module_bindings;

use module_bindings::*;

use spacetimedb_sdk::{DbConnectionBuilder, DbContext, Table};

use test_counter::TestCounter;

const LOCALHOST: &str = "http://localhost:3000";

fn db_name_or_panic() -> String {
    std::env::var("SPACETIME_SDK_TEST_DB_NAME").expect("Failed to read db name from env")
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    dispatch();
}

pub(crate) fn dispatch() {
    let disconnect_test_counter = TestCounter::new();
    let disconnect_result = disconnect_test_counter.add_test("disconnect");

    let connect_test_counter = TestCounter::new();
    let connected_result = connect_test_counter.add_test("on_connect");
    let sub_applied_one_row_result = connect_test_counter.add_test("connected_row");

    let connection = DbConnection::builder()
        .with_database_name(db_name_or_panic())
        .with_uri(LOCALHOST)
        .on_connect_error(|_ctx, error| panic!("on_connect_error: {error:?}"))
        .on_connect(move |ctx, _, _| {
            connected_result(Ok(()));
            ctx.subscription_builder()
                .on_error(|_ctx, error| {
                    panic!("Subscription failed: {error:?}");
                })
                .on_applied(move |ctx| {
                    let check = || {
                        anyhow::ensure!(ctx.db.connected().count() == 1);
                        match ctx.db.connected().iter().next() {
                            Some(_row) => {
                                // TODO: anyhow::ensure!(row.identity == ctx.identity().unwrap());
                            }
                            _ => {
                                anyhow::bail!("Expected one row but Connected::iter().next() returned None");
                            }
                        }
                        Ok(())
                    };
                    sub_applied_one_row_result(check());
                })
                .subscribe("SELECT * FROM connected");
        })
        .on_disconnect(move |ctx, error| {
            assert!(
                !ctx.is_active(),
                "on_disconnect callback, but `ctx.is_active()` is true"
            );
            match error {
                Some(err) => disconnect_result(Err(anyhow::anyhow!("{err:?}"))),
                None => disconnect_result(Ok(())),
            }
        });
    let connection = build_connection(connection);

    #[cfg(not(target_arch = "wasm32"))]
    let join_handle = connection.run_threaded();
    #[cfg(target_arch = "wasm32")]
    connection.run_background_task();

    connect_test_counter.wait_for_all();

    connection.disconnect().unwrap();
    #[cfg(not(target_arch = "wasm32"))]
    join_handle.join().unwrap();

    disconnect_test_counter.wait_for_all();

    let reconnect_test_counter = TestCounter::new();
    let reconnected_result = reconnect_test_counter.add_test("on_reconnect");
    let sub_applied_one_row_result = reconnect_test_counter.add_test("disconnected_row");

    let new_connection = DbConnection::builder()
        .on_connect_error(|_ctx, error| panic!("on_connect_error: {error:?}"))
        .on_connect(move |_ctx, _, _| {
            reconnected_result(Ok(()));
        })
        .with_database_name(db_name_or_panic())
        .with_uri(LOCALHOST);
    let new_connection = build_connection(new_connection);

    new_connection
        .subscription_builder()
        .on_applied(move |ctx| {
            let check = || {
                anyhow::ensure!(ctx.db.disconnected().count() == 1);
                match ctx.db.disconnected().iter().next() {
                    Some(_row) => {
                        // TODO: anyhow::ensure!(row.identity == ctx.identity().unwrap());
                    }
                    _ => {
                        anyhow::bail!("Expected one row but Disconnected::iter().next() returned None");
                    }
                }
                Ok(())
            };
            sub_applied_one_row_result(check());
        })
        .on_error(|_ctx, error| panic!("subscription on_error: {error:?}"))
        .subscribe("SELECT * FROM disconnected");

    #[cfg(not(target_arch = "wasm32"))]
    new_connection.run_threaded();
    #[cfg(target_arch = "wasm32")]
    new_connection.run_background_task();

    reconnect_test_counter.wait_for_all();
}

#[cfg(not(target_arch = "wasm32"))]
fn build_connection(builder: DbConnectionBuilder<RemoteModule>) -> DbConnection {
    builder.build().unwrap()
}

#[cfg(target_arch = "wasm32")]
fn build_connection(builder: DbConnectionBuilder<RemoteModule>) -> DbConnection {
    futures::executor::block_on(builder.build()).unwrap()
}
