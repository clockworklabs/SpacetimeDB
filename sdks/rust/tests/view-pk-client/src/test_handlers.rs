use crate::module_bindings::*;
use spacetimedb_sdk::TableWithPrimaryKey;
use spacetimedb_sdk::{error::InternalError, DbConnectionBuilder, DbContext};
use test_counter::TestCounter;

const LOCALHOST: &str = "http://localhost:3000";

type ResultRecorder = Box<dyn Send + FnOnce(Result<(), anyhow::Error>)>;

#[cfg(not(target_arch = "wasm32"))]
async fn build_and_run(builder: DbConnectionBuilder<RemoteModule>) -> DbConnection {
    let conn = builder.build().unwrap();
    conn.run_threaded();
    conn
}

#[cfg(target_arch = "wasm32")]
async fn build_and_run(builder: DbConnectionBuilder<RemoteModule>) -> DbConnection {
    // Web builds use async connection setup, so awaiting here avoids blocking the event loop
    // before websocket callbacks and subscription completions have a chance to run.
    let conn = builder.build().await.unwrap();
    conn.run_background_task();
    conn
}

fn put_result(result: &mut Option<ResultRecorder>, res: Result<(), anyhow::Error>) {
    (result.take().unwrap())(res);
}

fn reducer_callback_assert_committed<T>(
    reducer_name: &'static str,
) -> impl FnOnce(&ReducerEventContext, Result<Result<T, String>, InternalError>) + Send + 'static {
    move |_ctx, outcome| match outcome {
        Ok(Ok(_)) => (),
        Ok(Err(msg)) => panic!("`{reducer_name}` reducer returned error: {msg}"),
        Err(internal_error) => panic!("`{reducer_name}` reducer panicked: {internal_error:?}"),
    }
}

async fn connect_then(
    db_name: &str,
    test_counter: &std::sync::Arc<TestCounter>,
    callback: impl FnOnce(&DbConnection) + Send + 'static,
) -> DbConnection {
    let connected_result = test_counter.add_test("on_connect");
    let name = db_name.to_owned();
    let conn = DbConnection::builder()
        .with_database_name(name)
        .with_uri(LOCALHOST)
        .on_connect(|ctx, _, _| {
            callback(ctx);
            connected_result(Ok(()));
        })
        .on_connect_error(|_ctx, error| panic!("Connect errored: {error:?}"));
    build_and_run(conn).await
}

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

/// Subscribe to a query builder view whose underlying table has a primary key.
/// Ensures the rust sdk emits an `on_update` callback and that the client receives the correct old and new rows.
///
/// Test:
/// 1. Subscribe to: SELECT * FROM all_view_pk_players
/// 2. Insert row:  (id=1, name="before")
/// 3. Update row:  (id=1, name="after")
///
/// Expect:
/// - `on_update` is called for PK=1
/// - `old_row` should be the "before" value
/// - `new_row` should be the "after" value
async fn exec_view_pk_on_update(db_name: &str) {
    let test_counter = TestCounter::new();
    let mut on_update = Some(test_counter.add_test("on_update"));

    connect_then(db_name, &test_counter, move |ctx| {
        subscribe_these_then(ctx, &["SELECT * FROM all_view_pk_players"], move |ctx| {
            ctx.db.all_view_pk_players().on_update(move |_, old_row, new_row| {
                assert_eq!(old_row.id, 1);
                assert_eq!(old_row.name, "before");
                assert_eq!(new_row.id, 1);
                assert_eq!(new_row.name, "after");
                put_result(&mut on_update, Ok(()));
            });

            // Seed the row that the view will expose.
            ctx.reducers()
                .insert_view_pk_player_then(
                    1,
                    "before".to_string(),
                    reducer_callback_assert_committed("insert_view_pk_player"),
                )
                .unwrap();

            // Mutate same PK so subscription emits an update(old,new) pair.
            ctx.reducers()
                .update_view_pk_player_then(
                    1,
                    "after".to_string(),
                    reducer_callback_assert_committed("update_view_pk_player"),
                )
                .unwrap();
        });
    })
    .await;

    test_counter.wait_for_all().await;
}

/// Subscribe to a right semijoin whose rhs is a view with primary key.
///
/// Ensures:
/// 1. A semijoin subscription involving a view is valid
/// 2. The rust sdk emits an `on_update` callback and that the client receives the correct old and new rows
///
/// Query:
///   SELECT player.*
///   FROM view_pk_membership membership
///   JOIN all_view_pk_players player ON membership.player_id = player.id
///
/// Test:
/// 1. Insert player row (id=1, "before").
/// 2. Insert membership row referencing player_id=1, allowing the semijoin match.
/// 3. Update player row to (id=1, "after").
///
/// Expect:
/// - `on_update` is called for player PK=1
/// - `old_row` should be the "before" value
/// - `new_row` should be the "after" value
async fn exec_view_pk_join_query_builder(db_name: &str) {
    let test_counter = TestCounter::new();
    let mut joined_update = Some(test_counter.add_test("join_update"));

    connect_then(db_name, &test_counter, move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.all_view_pk_players().on_update(move |_, old_row, new_row| {
                    assert_eq!(old_row.id, 1);
                    assert_eq!(old_row.name, "before");
                    assert_eq!(new_row.id, 1);
                    assert_eq!(new_row.name, "after");
                    put_result(&mut joined_update, Ok(()));
                });

                // Base player row.
                ctx.reducers()
                    .insert_view_pk_player_then(
                        1,
                        "before".to_string(),
                        reducer_callback_assert_committed("insert_view_pk_player"),
                    )
                    .unwrap();

                // Membership row that causes semijoin inclusion.
                ctx.reducers()
                    .insert_view_pk_membership_then(
                        1,
                        1,
                        reducer_callback_assert_committed("insert_view_pk_membership"),
                    )
                    .unwrap();

                // Update same PK to force joined-stream update event.
                ctx.reducers()
                    .update_view_pk_player_then(
                        1,
                        "after".to_string(),
                        reducer_callback_assert_committed("update_view_pk_player"),
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
    })
    .await;

    test_counter.wait_for_all().await;
}

/// Subscribe to a semijoin between two views with primary keys.
///
/// Ensures:
/// 1. A semijoin subscription involving a view is valid
/// 2. The rust sdk emits an `on_update` callback and that the client receives the correct old and new rows
///
/// Query:
///   SELECT b.*
///   FROM sender_view_pk_players_a a
///   JOIN sender_view_pk_players_b b ON a.id = b.id
///
/// Test:
/// 1. Insert player row (id=1, "before").
/// 2. Insert membership for sender view A.
/// 3. Insert membership for sender view B.
/// 4. Update player row to (id=1, "after").
///
/// Expect:
/// - `on_update` is called for player PK=1
/// - `old_row` should be the "before" value
/// - `new_row` should be the "after" value
async fn exec_view_pk_semijoin_two_sender_views_query_builder(db_name: &str) {
    let test_counter = TestCounter::new();
    let mut joined_update = Some(test_counter.add_test("join_update"));

    connect_then(db_name, &test_counter, move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.sender_view_pk_players_b().on_update(move |_, old_row, new_row| {
                    assert_eq!(old_row.id, 1);
                    assert_eq!(old_row.name, "before");
                    assert_eq!(new_row.id, 1);
                    assert_eq!(new_row.name, "after");
                    put_result(&mut joined_update, Ok(()));
                });

                // Base player row used by both sender-scoped views.
                ctx.reducers()
                    .insert_view_pk_player_then(
                        1,
                        "before".to_string(),
                        reducer_callback_assert_committed("insert_view_pk_player"),
                    )
                    .unwrap();

                // Membership edge that enables sender_view_pk_players_a.
                ctx.reducers()
                    .insert_view_pk_membership_then(
                        1,
                        1,
                        reducer_callback_assert_committed("insert_view_pk_membership"),
                    )
                    .unwrap();

                // Membership edge that enables sender_view_pk_players_b.
                ctx.reducers()
                    .insert_view_pk_membership_secondary_then(
                        1,
                        1,
                        reducer_callback_assert_committed("insert_view_pk_membership_secondary"),
                    )
                    .unwrap();

                // Update same PK to verify sender-view join emits an on_update event.
                ctx.reducers()
                    .update_view_pk_player_then(
                        1,
                        "after".to_string(),
                        reducer_callback_assert_committed("update_view_pk_player"),
                    )
                    .unwrap();
            })
            .add_query(|q| {
                q.from
                    .sender_view_pk_players_a()
                    .right_semijoin(q.from.sender_view_pk_players_b(), |lhs_view, rhs_view| {
                        lhs_view.id.eq(rhs_view.id)
                    })
                    .build()
            })
            .subscribe();
    })
    .await;

    test_counter.wait_for_all().await;
}

pub async fn dispatch(test: &str, db_name: &str) {
    match test {
        "view-pk-on-update" => exec_view_pk_on_update(db_name).await,
        "view-pk-join-query-builder" => exec_view_pk_join_query_builder(db_name).await,
        "view-pk-semijoin-two-sender-views-query-builder" => {
            exec_view_pk_semijoin_two_sender_views_query_builder(db_name).await
        }
        _ => panic!("Unknown test: {test}"),
    }
}
