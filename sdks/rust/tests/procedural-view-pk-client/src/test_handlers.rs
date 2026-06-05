use crate::module_bindings::*;
use spacetimedb_sdk::{error::InternalError, DbConnectionBuilder, DbContext, Table, TableWithPrimaryKey};
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
    let conn = builder.build().await.unwrap();
    conn.run_background_task();
    conn
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

async fn connect_then(
    db_name: &str,
    test_counter: &std::sync::Arc<TestCounter>,
    callback: impl FnOnce(&DbConnection) + Send + 'static,
) -> DbConnection {
    connect_then_named(db_name, test_counter, "on_connect", callback).await
}

async fn connect_then_named(
    db_name: &str,
    test_counter: &std::sync::Arc<TestCounter>,
    connect_test_name: &'static str,
    callback: impl FnOnce(&DbConnection) + Send + 'static,
) -> DbConnection {
    let connected_result = test_counter.add_test(connect_test_name);
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

async fn exec_sender_scoped_pk_view(db_name: &str) {
    let test_counter = TestCounter::new();
    let mut sender_a_update = Some(test_counter.add_test("sender_a_update"));
    let mut sender_b_update = Some(test_counter.add_test("sender_b_update"));

    // Keep both clients connected so the test exercises the sender predicate in
    // the procedural view. Each client inserts and updates a different primary key.
    // Receiving the other client's update would fail the assertions in the callback.
    let _sender_a = connect_then(db_name, &test_counter, move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.sender_left_view().on_update(move |_, old_row, new_row| {
                    assert_eq!(old_row.id, 1);
                    assert_eq!(old_row.filter, 10);
                    assert_eq!(new_row.id, 1);
                    assert_eq!(new_row.filter, 11);
                    put_result(&mut sender_a_update, Ok(()));
                });

                ctx.reducers()
                    .insert_left_then(1, 10, reducer_callback_assert_committed("insert_left"))
                    .unwrap();
                ctx.reducers()
                    .update_left_then(1, 11, reducer_callback_assert_committed("update_left"))
                    .unwrap();
            })
            .add_query(|q| q.from.sender_left_view().build())
            .subscribe();
    })
    .await;

    let _sender_b = connect_then_named(db_name, &test_counter, "sender_b_on_connect", move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.sender_left_view().on_update(move |_, old_row, new_row| {
                    assert_eq!(old_row.id, 2);
                    assert_eq!(old_row.filter, 20);
                    assert_eq!(new_row.id, 2);
                    assert_eq!(new_row.filter, 21);
                    put_result(&mut sender_b_update, Ok(()));
                });

                ctx.reducers()
                    .insert_left_then(2, 20, reducer_callback_assert_committed("insert_left"))
                    .unwrap();
                ctx.reducers()
                    .update_left_then(2, 21, reducer_callback_assert_committed("update_left"))
                    .unwrap();
            })
            .add_query(|q| q.from.sender_left_view().build())
            .subscribe();
    })
    .await;

    test_counter.wait_for_all().await;
}

// Shared harness for the semijoin tests. The tests intentionally keep their
// query-builder expressions inline because those expressions are the behavior
// under test. This helper only owns the common connection and completion wiring.
async fn exec_semijoin(
    db_name: &str,
    result_name: &'static str,
    subscribe: impl FnOnce(&DbConnection, ResultRecorder) + Send + 'static,
) {
    let test_counter = TestCounter::new();
    let joined_insert = test_counter.add_test(result_name);

    connect_then(db_name, &test_counter, move |ctx| {
        subscribe(ctx, joined_insert);
    })
    .await;

    test_counter.wait_for_all().await;
}

fn insert_semijoin_source_rows(ctx: &SubscriptionEventContext) {
    // Both views contain rows with ids 10 and 20, so the primary-key semijoin
    // alone would keep both rows. The side filters then reduce the result to
    // just id 10: left.filter == 100 and right.filter == 300.
    ctx.reducers()
        .insert_left_then(10, 100, reducer_callback_assert_committed("insert_left"))
        .unwrap();
    ctx.reducers()
        .insert_left_then(20, 200, reducer_callback_assert_committed("insert_left"))
        .unwrap();
    ctx.reducers()
        .insert_right_then(10, 300, reducer_callback_assert_committed("insert_right"))
        .unwrap();
    ctx.reducers()
        .insert_right_then(20, 400, reducer_callback_assert_committed("insert_right"))
        .unwrap();
}

async fn exec_view_pk_left_semijoin(db_name: &str) {
    exec_semijoin(db_name, "left_semijoin_insert", move |ctx, joined_insert| {
        let mut joined_insert = Some(joined_insert);
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.sender_left_view().on_insert(move |ctx, row| {
                    assert_eq!(ctx.db.sender_left_view().count(), 1);
                    assert_eq!(row.id, 10);
                    assert_eq!(row.filter, 100);
                    put_result(&mut joined_insert, Ok(()));
                });

                insert_semijoin_source_rows(ctx);
            })
            .add_query(|q| {
                q.from
                    .sender_right_view()
                    .filter(|right| right.filter.eq(300u64))
                    .right_semijoin(q.from.sender_left_view(), |right, left| right.id.eq(left.id))
                    .filter(|left| left.filter.eq(100u64))
                    .build()
            })
            .subscribe();
    })
    .await;
}

async fn exec_view_pk_right_semijoin(db_name: &str) {
    exec_semijoin(db_name, "right_semijoin_insert", move |ctx, joined_insert| {
        let mut joined_insert = Some(joined_insert);
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.sender_right_view().on_insert(move |ctx, row| {
                    assert_eq!(ctx.db.sender_right_view().count(), 1);
                    assert_eq!(row.id, 10);
                    assert_eq!(row.filter, 300);
                    put_result(&mut joined_insert, Ok(()));
                });

                insert_semijoin_source_rows(ctx);
            })
            .add_query(|q| {
                q.from
                    .sender_left_view()
                    .filter(|left| left.filter.eq(100u64))
                    .right_semijoin(q.from.sender_right_view(), |left, right| left.id.eq(right.id))
                    .filter(|right| right.filter.eq(300u64))
                    .build()
            })
            .subscribe();
    })
    .await;
}

pub async fn dispatch(test: &str, db_name: &str) {
    match test {
        "sender-scoped-pk-view" => exec_sender_scoped_pk_view(db_name).await,
        "view-pk-left-semijoin" => exec_view_pk_left_semijoin(db_name).await,
        "view-pk-right-semijoin" => exec_view_pk_right_semijoin(db_name).await,
        _ => panic!("Unknown test: {test}"),
    }
}
