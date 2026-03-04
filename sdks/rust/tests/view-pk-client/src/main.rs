mod module_bindings;

use module_bindings::*;
use spacetimedb_sdk::{error::InternalError, DbContext, Table};
#[cfg(feature = "expect_view_pk_on_update")]
use spacetimedb_sdk::TableWithPrimaryKey;
use test_counter::TestCounter;

const LOCALHOST: &str = "http://localhost:3000";

type ResultRecorder = Box<dyn Send + FnOnce(Result<(), anyhow::Error>)>;

fn exit_on_panic() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        default_hook(panic_info);
        std::process::exit(1);
    }));
}

fn db_name_or_panic() -> String {
    std::env::var("SPACETIME_SDK_TEST_DB_NAME").expect("Failed to read db name from env")
}

fn put_result(result: &mut Option<ResultRecorder>, res: Result<(), anyhow::Error>) {
    (result.take().unwrap())(res);
}

fn reducer_callback_assert_committed(
    reducer_name: &'static str,
) -> impl FnOnce(&ReducerEventContext, Result<Result<(), String>, InternalError>) + Send + 'static {
    move |_ctx, outcome| match outcome {
        Ok(Ok(())) => (),
        Ok(Err(msg)) => panic!("`{reducer_name}` reducer returned error: {msg}"),
        Err(internal_error) => panic!("`{reducer_name}` reducer panicked: {internal_error:?}"),
    }
}

fn connect_then(
    test_counter: &std::sync::Arc<TestCounter>,
    callback: impl FnOnce(&DbConnection) + Send + 'static,
) -> DbConnection {
    let connected_result = test_counter.add_test("on_connect");
    let name = db_name_or_panic();
    let conn = DbConnection::builder()
        .with_database_name(name)
        .with_uri(LOCALHOST)
        .on_connect(|ctx, _, _| {
            callback(ctx);
            connected_result(Ok(()));
        })
        .on_connect_error(|_ctx, error| panic!("Connect errored: {error:?}"))
        .build()
        .unwrap();
    conn.run_threaded();
    conn
}

#[cfg(feature = "expect_view_pk_on_update")]
fn subscribe_these_then(
    ctx: &impl RemoteDbContext,
    queries: &[&str],
    callback: impl FnOnce(&SubscriptionEventContext) + Send + 'static,
) {
    ctx.subscription_builder()
        .on_applied(callback)
        .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
        .subscribe(queries);
}

#[cfg(feature = "expect_view_pk_on_update")]
fn exec_view_pk_on_update() {
    let test_counter = TestCounter::new();
    let mut on_update = Some(test_counter.add_test("on_update"));

    connect_then(&test_counter, move |ctx| {
        subscribe_these_then(ctx, &["SELECT * FROM all_view_pk_players"], move |ctx| {
            ctx.db.all_view_pk_players().on_update(move |_, old_row, new_row| {
                assert_eq!(old_row.id, 1);
                assert_eq!(old_row.name, "before");
                assert_eq!(new_row.id, 1);
                assert_eq!(new_row.name, "after");
                put_result(&mut on_update, Ok(()));
            });

            ctx.reducers()
                .insert_view_pk_player_then(
                    1,
                    "before".to_string(),
                    reducer_callback_assert_committed("insert_view_pk_player"),
                )
                .unwrap();

            ctx.reducers()
                .update_view_pk_player_then(
                    1,
                    "after".to_string(),
                    reducer_callback_assert_committed("update_view_pk_player"),
                )
                .unwrap();
        });
    });

    test_counter.wait_for_all();
}

#[cfg(not(feature = "expect_view_pk_on_update"))]
fn exec_view_pk_on_update() {
    panic!("This test must be run with --features expect_view_pk_on_update");
}

fn exec_view_pk_join_query_builder() {
    let test_counter = TestCounter::new();
    let mut joined_insert = Some(test_counter.add_test("join_insert"));

    connect_then(&test_counter, move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.all_view_pk_players().on_insert(move |_, row| {
                    assert_eq!(row.id, 1);
                    assert_eq!(row.name, "joined");
                    put_result(&mut joined_insert, Ok(()));
                });

                ctx.reducers()
                    .insert_view_pk_player_then(
                        1,
                        "joined".to_string(),
                        reducer_callback_assert_committed("insert_view_pk_player"),
                    )
                    .unwrap();

                ctx.reducers()
                    .insert_view_pk_membership_then(
                        1,
                        1,
                        reducer_callback_assert_committed("insert_view_pk_membership"),
                    )
                    .unwrap();
            })
            .add_query(|q| {
                q.from
                    .view_pk_membership()
                    .right_semijoin(q.from.all_view_pk_players(), |membership, player| {
                        membership.player_id.eq(player.id)
                    })
                    .build()
            })
            .subscribe();
    });

    test_counter.wait_for_all();
}

fn main() {
    env_logger::init();
    exit_on_panic();

    let test = std::env::args()
        .nth(1)
        .expect("Pass a test name as a command-line argument to the test client");

    match &*test {
        "view-pk-on-update" => exec_view_pk_on_update(),
        "view-pk-join-query-builder" => exec_view_pk_join_query_builder(),
        _ => panic!("Unknown test: {test}"),
    }
}
