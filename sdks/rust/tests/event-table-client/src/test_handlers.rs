use crate::module_bindings::*;

use spacetimedb_sdk::{DbConnectionBuilder, DbContext, Event, TableLike, WithInsert};
use std::sync::atomic::{AtomicU32, Ordering};
use test_counter::TestCounter;

const LOCALHOST: &str = "http://localhost:3000";

macro_rules! assert_eq_or_bail {
    ($expected:expr, $found:expr) => {{
        let expected = &$expected;
        let found = &$found;
        if expected != found {
            anyhow::bail!(
                "Expected {} => {:?} but found {} => {:?}",
                stringify!($expected),
                expected,
                stringify!($found),
                found
            );
        }
    }};
}

pub async fn dispatch(test: &str, db_name: &str) {
    match test {
        "event-table" => exec_event_table(db_name).await,
        "multiple-events" => exec_multiple_events(db_name).await,
        "events-dont-persist" => exec_events_dont_persist(db_name).await,
        "v1-rejects-event-table" => exec_v1_rejects_event_table(db_name).await,
        _ => panic!("Unknown test: {test}"),
    }
}

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

async fn exec_event_table(db_name: &str) {
    let test_counter = TestCounter::new();
    let sub_applied_result = test_counter.add_test("subscription_applied");
    let on_insert_result = test_counter.add_test("event-table-on-insert");
    let on_insert_result = std::sync::Mutex::new(Some(on_insert_result));

    connect_then(db_name, &test_counter, {
        move |ctx| {
            subscribe_these_then(ctx, &["SELECT * FROM test_event;"], move |ctx| {
                // Event table should be empty on subscription applied
                assert_eq!(0usize, ctx.db.test_event().iter().count());
                sub_applied_result(Ok(()));

                ctx.db.test_event().on_insert(move |ctx, row| {
                    if let Some(set_result) = on_insert_result.lock().unwrap().take() {
                        let run_checks = || {
                            assert_eq_or_bail!("hello", row.name);
                            assert_eq_or_bail!(42u64, row.value);

                            let Event::Reducer(reducer_event) = &ctx.event else {
                                anyhow::bail!("Expected a reducer event");
                            };
                            anyhow::ensure!(
                                matches!(reducer_event.reducer, Reducer::EmitTestEvent { .. }),
                                "Unexpected Reducer variant {:?}",
                                reducer_event.reducer,
                            );

                            // Event table rows are not cached
                            assert_eq_or_bail!(0u64, ctx.db.test_event().count());
                            assert_eq_or_bail!(0usize, ctx.db.test_event().iter().count());

                            Ok(())
                        };
                        set_result(run_checks());
                    }
                });

                ctx.reducers.emit_test_event("hello".to_string(), 42).unwrap();
            });
        }
    })
    .await;

    test_counter.wait_for_all().await;
}

/// Test that multiple events emitted in a single reducer call all arrive as inserts.
async fn exec_multiple_events(db_name: &str) {
    let test_counter = TestCounter::new();
    let sub_applied_result = test_counter.add_test("subscription_applied");
    let result = test_counter.add_test("multiple-events");
    let result = std::sync::Mutex::new(Some(result));

    connect_then(db_name, &test_counter, {
        move |ctx| {
            subscribe_these_then(ctx, &["SELECT * FROM test_event;"], move |ctx| {
                assert_eq!(0usize, ctx.db.test_event().iter().count());
                sub_applied_result(Ok(()));

                let received = std::sync::Arc::new(AtomicU32::new(0));

                ctx.db.test_event().on_insert({
                    let received = received.clone();
                    move |_ctx, _row| {
                        let count = received.fetch_add(1, Ordering::SeqCst) + 1;
                        if count == 3 {
                            let set_result = result.lock().unwrap().take().unwrap();
                            set_result(Ok(()));
                        }
                    }
                });

                ctx.reducers.emit_multiple_test_events().unwrap();
            });
        }
    })
    .await;

    test_counter.wait_for_all().await;
}

/// Test that event table rows don't persist across transactions.
/// Emit events, then call a no-op reducer. After the no-op completes,
/// verify we didn't receive any additional event inserts.
async fn exec_events_dont_persist(db_name: &str) {
    let test_counter = TestCounter::new();
    let sub_applied_result = test_counter.add_test("subscription_applied");
    let noop_result = test_counter.add_test("events-dont-persist");
    let noop_result = std::sync::Mutex::new(Some(noop_result));

    connect_then(db_name, &test_counter, {
        move |ctx| {
            subscribe_these_then(ctx, &["SELECT * FROM test_event;"], move |ctx| {
                assert_eq!(0usize, ctx.db.test_event().iter().count());
                sub_applied_result(Ok(()));

                let insert_count = std::sync::Arc::new(AtomicU32::new(0));

                ctx.db.test_event().on_insert({
                    let insert_count = insert_count.clone();
                    move |_ctx, _row| {
                        insert_count.fetch_add(1, Ordering::SeqCst);
                    }
                });

                ctx.reducers.emit_test_event("hello".to_string(), 42).unwrap();

                // After the noop reducer completes, the insert count should
                // still be 1 from the emit_test_event call — no stale events.
                ctx.reducers
                    .noop_then({
                        let insert_count = insert_count.clone();
                        move |_ctx, _result| {
                            let set_result = noop_result.lock().unwrap().take().unwrap();
                            let count = insert_count.load(Ordering::SeqCst);
                            if count == 1 {
                                set_result(Ok(()));
                            } else {
                                set_result(Err(anyhow::anyhow!("Expected 1 event insert, but got {count}")));
                            }
                        }
                    })
                    .unwrap();
            });
        }
    })
    .await;

    test_counter.wait_for_all().await;
}

/// Test that v1 WebSocket clients are rejected when subscribing to event tables.
/// The server should return a subscription error directing the developer to upgrade.
async fn exec_v1_rejects_event_table(db_name: &str) {
    let test_counter = TestCounter::new();

    connect_then(db_name, &test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            let error_result = test_counter.add_test("v1-rejects-event-table");

            ctx.subscription_builder()
                .on_applied(move |_ctx: &SubscriptionEventContext| {
                    panic!("Subscription to event table should not succeed over v1");
                })
                .on_error(move |_ctx, error| {
                    let msg = format!("{error:?}");
                    if msg.contains("v2") || msg.contains("upgrade") || msg.contains("Upgrade") {
                        error_result(Ok(()));
                    } else {
                        error_result(Err(anyhow::anyhow!("Expected error about v2/upgrade, got: {msg}")));
                    }
                })
                .subscribe(["SELECT * FROM test_event;"]);
        }
    })
    .await;

    test_counter.wait_for_all().await;
}
