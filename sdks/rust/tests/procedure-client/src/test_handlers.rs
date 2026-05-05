use crate::module_bindings::*;
use anyhow::Context;
use core::time::Duration;
use spacetimedb_lib::db::raw_def::v9::{RawMiscModuleExportV9, RawModuleDefV9};
use spacetimedb_sdk::{DbConnectionBuilder, DbContext, Table};
use std::sync::{Arc, Mutex};
use test_counter::TestCounter;

const LOCALHOST: &str = "http://localhost:3000";

pub async fn dispatch(test: &str, db_name: &str) {
    match test {
        "procedure-return-values" => exec_procedure_return_values(db_name).await,
        "procedure-observe-panic" => exec_procedure_panic(db_name).await,
        "procedure-http-ok" => exec_procedure_http_ok(db_name).await,
        "procedure-http-err" => exec_procedure_http_err(db_name).await,
        "insert-with-tx-commit" => exec_insert_with_tx_commit(db_name).await,
        "insert-with-tx-rollback" => exec_insert_with_tx_rollback(db_name).await,
        "procedure-reducer-interleaving" => exec_procedure_reducer_interleaving(db_name).await,
        "procedure-reducer-same-client-not-interleaved" => {
            exec_procedure_reducer_same_client_not_interleaved(db_name).await
        }
        "schedule-procedure" => exec_schedule_procedure(db_name).await,
        "sorted-uuids-insert" => exec_sorted_uuids_insert(db_name).await,
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

fn assert_table_empty<T: Table>(tbl: T) -> anyhow::Result<()> {
    let count = tbl.count();
    if count != 0 {
        anyhow::bail!(
            "Expected table {} to be empty, but found {} rows resident",
            std::any::type_name::<T::Row>(),
            count,
        )
    }
    Ok(())
}

/// Each subscribing test runs against a fresh DB,
/// so all tables should be empty until we call an insert reducer.
///
/// We'll call this function within our initial `on_subscription_applied` callback to verify that.
fn assert_all_tables_empty(ctx: &impl RemoteDbContext) -> anyhow::Result<()> {
    assert_table_empty(ctx.db().my_table())?;
    assert_table_empty(ctx.db().procedure_concurrency_row())?;
    Ok(())
}

async fn connect_with_then(
    db_name: &str,
    test_counter: &std::sync::Arc<TestCounter>,
    on_connect_suffix: &str,
    with_builder: impl FnOnce(DbConnectionBuilder<RemoteModule>) -> DbConnectionBuilder<RemoteModule>,
    callback: impl FnOnce(&DbConnection) + Send + 'static,
) -> DbConnection {
    let connected_result = test_counter.add_test(format!("on_connect_{on_connect_suffix}"));
    let name = db_name.to_owned();
    let builder = DbConnection::builder()
        .with_database_name(name)
        .with_uri(LOCALHOST)
        .on_connect(|ctx, _, _| {
            callback(ctx);
            connected_result(Ok(()));
        })
        .on_connect_error(|_ctx, error| panic!("Connect errored: {error:?}"));
    build_and_run(with_builder(builder)).await
}

async fn connect_then(
    db_name: &str,
    test_counter: &std::sync::Arc<TestCounter>,
    callback: impl FnOnce(&DbConnection) + Send + 'static,
) -> DbConnection {
    connect_with_then(db_name, test_counter, "", |x| x, callback).await
}

/// A query that subscribes to all rows from all tables.
const SUBSCRIBE_ALL: &[&str] = &[
    "SELECT * FROM my_table;",
    "SELECT * FROM procedure_concurrency_row;",
    "SELECT * FROM proc_inserts_into;",
    "SELECT * FROM pk_uuid;",
];

fn subscribe_all_then(ctx: &impl RemoteDbContext, callback: impl FnOnce(&SubscriptionEventContext) + Send + 'static) {
    subscribe_these_then(ctx, SUBSCRIBE_ALL, callback)
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

async fn exec_procedure_return_values(db_name: &str) {
    let test_counter = TestCounter::new();

    connect_then(db_name, &test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            let return_primitive_result = test_counter.add_test("return_primitive");
            let return_struct_result = test_counter.add_test("return_struct");
            let return_enum_a_result = test_counter.add_test("return_enum_a");
            let return_enum_b_result = test_counter.add_test("return_enum_b");

            ctx.procedures.return_primitive_then(1, 2, move |_, res| {
                return_primitive_result(res.context("return_primtive failed unexpectedly").and_then(|sum| {
                    if sum == 3 {
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!(
                            "Expected return value from return_primitive of 3 but got {sum}"
                        ))
                    }
                }));
            });
            ctx.procedures
                .return_struct_then(1234, "foo".to_string(), move |_, res| {
                    return_struct_result(res.context("return_struct failed unexpectedly").and_then(|strukt| {
                        anyhow::ensure!(strukt.a == 1234);
                        anyhow::ensure!(&*strukt.b == "foo");
                        Ok(())
                    }));
                });
            ctx.procedures.return_enum_a_then(1234, move |_, res| {
                return_enum_a_result(res.context("return_enum_a failed unexpectedly").and_then(|enum_a| {
                    anyhow::ensure!(matches!(enum_a, ReturnEnum::A(1234)));
                    Ok(())
                }));
            });
            ctx.procedures.return_enum_b_then("foo".to_string(), move |_, res| {
                return_enum_b_result(res.context("return_enum_b failed unexpectedly").and_then(|enum_b| {
                    let ReturnEnum::B(string) = enum_b else {
                        anyhow::bail!("Unexpected variant for returned enum {enum_b:?}");
                    };
                    anyhow::ensure!(&*string == "foo");
                    Ok(())
                }));
            });
        }
    })
    .await;

    test_counter.wait_for_all().await;
}

async fn exec_procedure_panic(db_name: &str) {
    let test_counter = TestCounter::new();

    connect_then(db_name, &test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            let will_panic_result = test_counter.add_test("will_panic");

            ctx.procedures.will_panic_then(move |_, res| {
                will_panic_result(if res.is_err() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Expected failure but got Ok... huh? {res:?}"))
                });
            });
        }
    })
    .await;

    test_counter.wait_for_all().await;
}

async fn exec_insert_with_tx_commit(db_name: &str) {
    fn expected() -> ReturnStruct {
        ReturnStruct {
            a: 42,
            b: "magic".into(),
        }
    }

    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");
    let inspect_result = test_counter.add_test("insert_with_tx_commit_values");
    let mut callback_result = Some(test_counter.add_test("insert_with_tx_commit_callback"));

    connect_then(db_name, &test_counter, {
        move |ctx| {
            ctx.db().my_table().on_insert(move |_, row| {
                assert_eq!(row.field, expected());
                (callback_result.take().unwrap())(Ok(()));
            });

            subscribe_all_then(ctx, move |ctx| {
                sub_applied_nothing_result(assert_all_tables_empty(ctx));

                ctx.procedures.insert_with_tx_commit_then(move |ctx, res| {
                    assert!(res.is_ok(), "Expected Ok result but got {res:?}");
                    let row = ctx.db().my_table().iter().next().unwrap();
                    assert_eq!(row.field, expected());
                    inspect_result(Ok(()));
                });
            });
        }
    })
    .await;

    test_counter.wait_for_all().await;
}

async fn exec_insert_with_tx_rollback(db_name: &str) {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");
    let inspect_result = test_counter.add_test("insert_with_tx_rollback_values");

    connect_then(db_name, &test_counter, {
        move |ctx| {
            ctx.db()
                .my_table()
                .on_insert(|_, _| unreachable!("should not have inserted a row"));

            subscribe_all_then(ctx, move |ctx| {
                sub_applied_nothing_result(assert_all_tables_empty(ctx));

                ctx.procedures.insert_with_tx_rollback_then(move |ctx, res| {
                    assert!(res.is_ok(), "Expected Ok result but got {res:?}");
                    assert_eq!(ctx.db().my_table().iter().next(), None);
                    inspect_result(Ok(()));
                });
            });
        }
    })
    .await;

    test_counter.wait_for_all().await;
}

#[derive(Default)]
struct ConnectionRowObservation {
    procedure_before: Option<u32>,
    reducer: Option<u32>,
    procedure_after: Option<u32>,
    ordering_checked: bool,
}

#[derive(Default)]
struct ProcedureReducerInterleavingState {
    procedure_conn: ConnectionRowObservation,
    reducer_conn: ConnectionRowObservation,
    reducer_invoked: bool,
}

/// Test that a procedure and a reducer can execute concurrently,
/// provided that the procedure cooperatively yields with `ctx.sleep_until`.
///
/// This test creates two separate connections, `procedure_conn` and `reducer_conn`.
/// Two connections are necessary because, as of writing (pgoldman 2026-05-05),
/// WS messages from a single client are processed strictly in-order,
/// so a long-running procedure invoked by a client causes all subsequent messages from that client
/// to wait until the procedure finishes.
/// https://github.com/clockworklabs/SpacetimeDB/issues/4954 tracks this behavior.
///
/// In the test, `procedure_conn` invokes a procedure, `procedure_sleep_between_inserts`,
/// which inserts a row, sleeps for 10 seconds, then inserts another row.
/// `reducer_conn` listens for the first row, and when it sees it,
/// invokes a reducer `insert_reducer_row` which inserts a row.
/// Both connections then collect all 3 rows and assert
/// that their `auto_inc` IDs are ordered s.t. the reducer row was inserted before the procedure post-sleep row.
async fn exec_procedure_reducer_interleaving(db_name: &str) {
    let init_counter = TestCounter::new();
    let procedure_sub_applied_result = init_counter.add_test("procedure_on_subscription_applied_nothing");
    let reducer_sub_applied_result = init_counter.add_test("reducer_on_subscription_applied_nothing");

    let test_counter = TestCounter::new();
    let procedure_callback_result = test_counter.add_test("procedure_sleep_between_inserts_callback");
    let mut reducer_callback_result = Some(test_counter.add_test("insert_reducer_row_callback"));
    let mut procedure_ordering_result = Some(test_counter.add_test("procedure_connection_order"));
    let mut reducer_ordering_result = Some(test_counter.add_test("reducer_connection_order"));
    let state = Arc::new(Mutex::new(ProcedureReducerInterleavingState::default()));

    let procedure_conn = connect_with_then(
        db_name,
        &init_counter,
        "procedure",
        |x| x,
        {
            let state = Arc::clone(&state);
            move |ctx| {
                ctx.db().procedure_concurrency_row().on_insert({
                    let state = Arc::clone(&state);
                    move |_ctx, row| {
                        let maybe_ordering = {
                            let mut state = state.lock().expect("ProcedureReducerInterleavingState mutex is poisoned");
                            let observation = &mut state.procedure_conn;
                            match row.insertion_context.as_str() {
                                "procedure_before" => {
                                    assert!(observation.procedure_before.replace(row.insertion_order).is_none());
                                }
                                "reducer" => {
                                    assert!(observation.reducer.replace(row.insertion_order).is_none());
                                }
                                "procedure_after" => {
                                    assert!(observation.procedure_after.replace(row.insertion_order).is_none());
                                }
                                unexpected => panic!("Unexpected insertion context: {unexpected}"),
                            }
                            match (
                                observation.procedure_before,
                                observation.reducer,
                                observation.procedure_after,
                            ) {
                                (Some(before), Some(reducer), Some(after)) if !observation.ordering_checked => {
                                    observation.ordering_checked = true;
                                    Some((before, reducer, after))
                                }
                                _ => None,
                            }
                        };

                        if let Some((before, reducer, after)) = maybe_ordering {
                            (procedure_ordering_result
                                .take()
                                .expect("Procedure ordering result should only be reported once"))(
                                #[allow(clippy::redundant_closure_call)]
                                (|| {
                                    anyhow::ensure!(
                                        before < reducer && reducer < after,
                                        "Procedure connection observed wrong insertion order: {before} < {reducer} < {after}"
                                    );
                                    Ok(())
                                })(),
                            );
                        }
                    }
                });

                subscribe_all_then(ctx, move |ctx| {
                    procedure_sub_applied_result(assert_all_tables_empty(ctx));
                });
            }
        },
    )
    .await;

    let reducer_conn = connect_with_then(db_name, &init_counter, "reducer", |x| x, {
        let state = Arc::clone(&state);
        move |ctx| {
            ctx.db().procedure_concurrency_row().on_insert({
                let state = Arc::clone(&state);
                move |ctx, row| {
                    let should_call_reducer = {
                        let mut state = state
                            .lock()
                            .expect("ProcedureReducerInterleavingState mutex is poisoned");
                        match row.insertion_context.as_str() {
                            "procedure_before" => {
                                assert!(state
                                    .reducer_conn
                                    .procedure_before
                                    .replace(row.insertion_order)
                                    .is_none());
                                if !state.reducer_invoked {
                                    state.reducer_invoked = true;
                                    true
                                } else {
                                    false
                                }
                            }
                            "reducer" => {
                                assert!(state.reducer_conn.reducer.replace(row.insertion_order).is_none());
                                false
                            }
                            "procedure_after" => {
                                assert!(state
                                    .reducer_conn
                                    .procedure_after
                                    .replace(row.insertion_order)
                                    .is_none());
                                false
                            }
                            unexpected => panic!("Unexpected insertion context: {unexpected}"),
                        }
                    };

                    if should_call_reducer {
                        let reducer_callback_result = reducer_callback_result
                            .take()
                            .expect("Reducer callback should only be registered once");
                        ctx.reducers
                            .insert_reducer_row_then(move |_ctx, outcome| {
                                reducer_callback_result(match outcome {
                                    Ok(Ok(())) => Ok(()),
                                    Ok(Err(msg)) => {
                                        Err(anyhow::anyhow!("`insert_reducer_row` reducer returned error: {msg}"))
                                    }
                                    Err(internal_error) => Err(anyhow::anyhow!(
                                        "`insert_reducer_row` reducer panicked: {internal_error:?}"
                                    )),
                                });
                            })
                            .unwrap();
                    }

                    let maybe_ordering = {
                        let mut state = state
                            .lock()
                            .expect("ProcedureReducerInterleavingState mutex is poisoned");
                        let observation = &mut state.reducer_conn;
                        match (
                            observation.procedure_before,
                            observation.reducer,
                            observation.procedure_after,
                        ) {
                            (Some(before), Some(reducer), Some(after)) if !observation.ordering_checked => {
                                observation.ordering_checked = true;
                                Some((before, reducer, after))
                            }
                            _ => None,
                        }
                    };

                    if let Some((before, reducer, after)) = maybe_ordering {
                        (reducer_ordering_result
                            .take()
                            .expect("Reducer ordering result should only be reported once"))(
                            #[allow(clippy::redundant_closure_call)]
                            (|| {
                                anyhow::ensure!(
                                    before < reducer && reducer < after,
                                    "Reducer connection observed wrong insertion order: {before} < {reducer} < {after}"
                                );
                                Ok(())
                            })(),
                        );
                    }
                }
            });

            subscribe_all_then(ctx, move |ctx| {
                reducer_sub_applied_result(assert_all_tables_empty(ctx));
            });
        }
    })
    .await;

    init_counter.wait_for_all().await;

    procedure_conn
        .procedures
        .procedure_sleep_between_inserts_then(move |_ctx, res| {
            procedure_callback_result(
                res.context("procedure_sleep_between_inserts failed unexpectedly")
                    .map(|()| ()),
            );
        });

    test_counter.wait_for_all().await;

    let _keep_connections_alive = (procedure_conn, reducer_conn);
}

/// Test that messages to and from a single connection, including `CallProcedure` and `ProcedureResult`, are strictly ordered.
///
/// This test is the dual of [`exec_procedure_reducer_interleaving`]:
/// it asserts that the reducer does not run until after the procedure has completed,
/// because the host won't reorder or concurrently process messages from the same client.
///
/// We're not attached to this behavior, and in fact https://github.com/clockworklabs/SpacetimeDB/issues/4954
/// tracks a plan to change it.
/// When that ticket is fixed, we should merge the two tests into a single one,
/// which asserts the interleaved order expected by [`exec_procedure_reducer_interleaving`],
/// but uses only a single connection like this test.
async fn exec_procedure_reducer_same_client_not_interleaved(db_name: &str) {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");
    let procedure_callback_result = test_counter.add_test("procedure_sleep_between_inserts_callback");
    let mut reducer_callback_result = Some(test_counter.add_test("insert_reducer_row_callback"));
    let mut ordering_result = Some(test_counter.add_test("procedure_reducer_same_client_order"));
    let state = Arc::new(Mutex::new(ProcedureReducerInterleavingState::default()));

    connect_then(db_name, &test_counter, {
        let state = Arc::clone(&state);
        move |ctx| {
            ctx.db().procedure_concurrency_row().on_insert({
                let state = Arc::clone(&state);
                move |ctx, row| {
                    let should_call_reducer = {
                        let mut state = state.lock().expect("ProcedureReducerInterleavingState mutex is poisoned");
                        let observation = &mut state.procedure_conn;
                        match row.insertion_context.as_str() {
                            "procedure_before" => {
                                assert!(observation.procedure_before.replace(row.insertion_order).is_none());
                                if !state.reducer_invoked {
                                    state.reducer_invoked = true;
                                    true
                                } else {
                                    false
                                }
                            }
                            "reducer" => {
                                assert!(observation.reducer.replace(row.insertion_order).is_none());
                                false
                            }
                            "procedure_after" => {
                                assert!(observation.procedure_after.replace(row.insertion_order).is_none());
                                false
                            }
                            unexpected => panic!("Unexpected insertion context: {unexpected}"),
                        }
                    };

                    if should_call_reducer {
                        let reducer_callback_result = reducer_callback_result
                            .take()
                            .expect("Reducer callback should only be registered once");
                        ctx.reducers
                            .insert_reducer_row_then(move |_ctx, outcome| {
                                reducer_callback_result(match outcome {
                                    Ok(Ok(())) => Ok(()),
                                    Ok(Err(msg)) => Err(anyhow::anyhow!(
                                        "`insert_reducer_row` reducer returned error: {msg}"
                                    )),
                                    Err(internal_error) => Err(anyhow::anyhow!(
                                        "`insert_reducer_row` reducer panicked: {internal_error:?}"
                                    )),
                                });
                            })
                            .unwrap();
                    }

                    let maybe_ordering = {
                        let mut state = state.lock().expect("ProcedureReducerInterleavingState mutex is poisoned");
                        let observation = &mut state.procedure_conn;
                        match (
                            observation.procedure_before,
                            observation.reducer,
                            observation.procedure_after,
                        ) {
                            (Some(before), Some(reducer), Some(after)) if !observation.ordering_checked => {
                                observation.ordering_checked = true;
                                Some((before, reducer, after))
                            }
                            _ => None,
                        }
                    };

                    if let Some((before, reducer, after)) = maybe_ordering {
                        (ordering_result.take().expect("Ordering result should only be reported once"))(
                            #[allow(clippy::redundant_closure_call)]
                            (|| {
                                anyhow::ensure!(
                                    before < after && after < reducer,
                                    "Expected same-client insertion order procedure_before < procedure_after < reducer, got {before} < {after} < {reducer}"
                                );
                                Ok(())
                            })(),
                        );
                    }
                }
            });

            subscribe_all_then(ctx, move |ctx| {
                sub_applied_nothing_result(assert_all_tables_empty(ctx));
                ctx.procedures.procedure_sleep_between_inserts_then(move |_ctx, res| {
                    procedure_callback_result(
                        res.context("procedure_sleep_between_inserts failed unexpectedly")
                            .map(|()| ()),
                    );
                });
            });
        }
    })
    .await;

    test_counter.wait_for_all().await;
}

/// Test that a procedure can perform an HTTP request and return a string derived from the response.
///
/// Invoke the procedure `read_my_schema`,
/// which does an HTTP request to the `/database/schema` route and returns a JSON-ified [`RawModuleDefV9`],
/// then (in the client) deserialize the response and assert that it contains a description of that procedure.
async fn exec_procedure_http_ok(db_name: &str) {
    let test_counter = TestCounter::new();
    connect_then(db_name, &test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            let result = test_counter.add_test("invoke_http");
            ctx.procedures.read_my_schema_then(move |_ctx, res| {
                result(
                    // It's a try block!
                    #[allow(clippy::redundant_closure_call)]
                    (|| {
                        anyhow::ensure!(res.is_ok(), "Expected Ok result but got {res:?}");
                        let module_def: RawModuleDefV9 = spacetimedb_lib::de::serde::deserialize_from(
                            &mut serde_json::Deserializer::from_str(&res.unwrap()),
                        )?;
                        anyhow::ensure!(module_def.misc_exports.iter().any(|misc_export| {
                            if let RawMiscModuleExportV9::Procedure(procedure_def) = misc_export {
                                &*procedure_def.name == "read_my_schema"
                            } else {
                                false
                            }
                        }));
                        Ok(())
                    })(),
                )
            })
        }
    })
    .await;
    test_counter.wait_for_all().await;
}

/// Test that a procedure can perform an HTTP request, handle its failure and return a string derived from the error.
///
/// Invoke the procedure `invalid_request`,
/// which does an HTTP request to a reserved invalid URL and returns a string-ified error,
/// then (in the client) assert that the error message looks sane.
async fn exec_procedure_http_err(db_name: &str) {
    let test_counter = TestCounter::new();
    connect_then(db_name, &test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            let result = test_counter.add_test("invoke_http");
            ctx.procedures.invalid_request_then(move |_ctx, res| {
                result(
                    // It's a try block!
                    #[allow(clippy::redundant_closure_call)]
                    (|| {
                        anyhow::ensure!(res.is_ok(), "Expected Ok result but got {res:?}");
                        let error = res.unwrap();
                        anyhow::ensure!(error.contains("error"));
                        anyhow::ensure!(error.contains("http://foo.invalid/"));
                        Ok(())
                    })(),
                )
            })
        }
    })
    .await;

    test_counter.wait_for_all().await;
}

async fn exec_schedule_procedure(db_name: &str) {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let mut callback_result = Some(test_counter.add_test("insert_with_tx_commit_callback"));

    connect_then(db_name, &test_counter, {
        move |ctx| {
            ctx.db().proc_inserts_into().on_insert(move |_, row| {
                assert_eq!(row.x, 42);
                assert_eq!(row.y, 24);

                // Ensure that the elapsed time
                // between the reducer and procedure
                // is at least 1 second
                // but no more than 2 seconds.
                let elapsed = row
                    .procedure_ts
                    .duration_since(row.reducer_ts)
                    .expect("procedure ts > reducer ts");
                const MS_1000: Duration = Duration::from_secs(1);
                const MS_2000: Duration = Duration::from_secs(2);
                assert!(elapsed >= MS_1000);
                assert!(elapsed <= MS_2000);

                (callback_result.take().unwrap())(Ok(()));
            });

            subscribe_all_then(ctx, move |ctx| {
                sub_applied_nothing_result(assert_all_tables_empty(ctx));
                ctx.reducers
                    .schedule_proc_then(|_ctx, outcome| match outcome {
                        Ok(Ok(())) => (),
                        Ok(Err(msg)) => panic!("`schedule_proc` reducer returned error: {msg}"),
                        Err(internal_error) => panic!("`schedule_proc` reducer panicked: {internal_error:?}"),
                    })
                    .unwrap();
            });
        }
    })
    .await;

    test_counter.wait_for_all().await;
}

/// Test that a procedure can generate sorted UUIDs and insert them into a table
///
/// Invoke the procedure `sorted_uuids_insert`,
/// which generates 1000 sorted UUIDv7 values and inserts them into the `pk_uuid` table,
/// then (in the client) verify that the UUIDs in the table are sorted
async fn exec_sorted_uuids_insert(db_name: &str) {
    let test_counter = TestCounter::new();
    let sorted_uuids_insert_result = test_counter.add_test("sorted_uuids_insert");

    connect_then(db_name, &test_counter, {
        move |ctx| {
            ctx.procedures.sorted_uuids_insert_then(move |ctx, res| {
                sorted_uuids_insert_result(
                    // It's a try block!
                    #[allow(clippy::redundant_closure_call)]
                    (|| {
                        anyhow::ensure!(res.is_ok(), "Expected Ok result but got {res:?}");

                        let mut last_uuid = None;
                        for row in ctx.db().pk_uuid().iter() {
                            if let Some(last) = last_uuid {
                                anyhow::ensure!(
                                    last < row.u,
                                    "UUIDs are not sorted: last UUID {last} >= current UUID {}",
                                    row.u
                                );
                            }
                            last_uuid = Some(row.u);
                        }

                        Ok(())
                    })(),
                )
            });
        }
    })
    .await;

    test_counter.wait_for_all().await;
}
