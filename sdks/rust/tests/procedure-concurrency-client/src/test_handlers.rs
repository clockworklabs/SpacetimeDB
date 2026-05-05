use crate::module_bindings::*;
use anyhow::Context;
use spacetimedb_sdk::{DbConnectionBuilder, DbContext, Table};
use std::sync::{Arc, Mutex};
use test_counter::TestCounter;

const LOCALHOST: &str = "http://localhost:3000";

pub async fn dispatch(test: &str, db_name: &str) {
    match test {
        "procedure-reducer-interleaving" => exec_procedure_reducer_interleaving(db_name).await,
        "procedure-reducer-same-client-not-interleaved" => {
            exec_procedure_reducer_same_client_not_interleaved(db_name).await
        }
        "procedure-concurrent-with-scheduled-reducer" => {
            exec_procedure_concurrent_with_scheduled_reducer(db_name).await
        }
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

fn assert_all_tables_empty(ctx: &impl RemoteDbContext) -> anyhow::Result<()> {
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

const SUBSCRIBE_ALL: &[&str] = &["SELECT * FROM procedure_concurrency_row;"];

fn subscribe_all_then(ctx: &impl RemoteDbContext, callback: impl FnOnce(&SubscriptionEventContext) + Send + 'static) {
    ctx.subscription_builder()
        .on_applied(callback)
        .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
        .subscribe(SUBSCRIBE_ALL);
}

#[derive(Default)]
struct ConnectionRowObservation {
    procedure_before: Option<u32>,
    reducer: Option<u32>,
    scheduled_reducer: Option<u32>,
    procedure_after: Option<u32>,
    ordering_checked: bool,
}

#[derive(Default)]
struct ProcedureReducerInterleavingState {
    procedure_conn: ConnectionRowObservation,
    reducer_conn: ConnectionRowObservation,
    reducer_invoked: bool,
}

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

    let procedure_conn = connect_with_then(db_name, &init_counter, "procedure", |x| x, {
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
    })
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
            procedure_callback_result(res.context("procedure_sleep_between_inserts failed unexpectedly"));
        });

    test_counter.wait_for_all().await;

    let _keep_connections_alive = (procedure_conn, reducer_conn);
}

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
                    procedure_callback_result(res.context("procedure_sleep_between_inserts failed unexpectedly"));
                });
            });
        }
    })
    .await;

    test_counter.wait_for_all().await;
}

async fn exec_procedure_concurrent_with_scheduled_reducer(db_name: &str) {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");
    let procedure_callback_result = test_counter.add_test("procedure_schedule_reducer_between_inserts_callback");
    let mut ordering_result = Some(test_counter.add_test("procedure_scheduled_reducer_order"));
    let state = Arc::new(Mutex::new(ConnectionRowObservation::default()));

    connect_then(db_name, &test_counter, {
        let state = Arc::clone(&state);
        move |ctx| {
            ctx.db().procedure_concurrency_row().on_insert({
                let state = Arc::clone(&state);
                move |_ctx, row| {
                    let maybe_ordering = {
                        let mut observation = state.lock().expect("ConnectionRowObservation mutex is poisoned");
                        match row.insertion_context.as_str() {
                            "procedure_before" => {
                                assert!(observation.procedure_before.replace(row.insertion_order).is_none());
                            }
                            "scheduled_reducer" => {
                                assert!(observation
                                    .scheduled_reducer
                                    .replace(row.insertion_order)
                                    .is_none());
                            }
                            "procedure_after" => {
                                assert!(observation.procedure_after.replace(row.insertion_order).is_none());
                            }
                            unexpected => panic!("Unexpected insertion context: {unexpected}"),
                        }
                        match (
                            observation.procedure_before,
                            observation.scheduled_reducer,
                            observation.procedure_after,
                        ) {
                            (Some(before), Some(scheduled_reducer), Some(after))
                                if !observation.ordering_checked =>
                            {
                                observation.ordering_checked = true;
                                Some((before, scheduled_reducer, after))
                            }
                            _ => None,
                        }
                    };

                    if let Some((before, scheduled_reducer, after)) = maybe_ordering {
                        (ordering_result.take().expect("Ordering result should only be reported once"))(
                            #[allow(clippy::redundant_closure_call)]
                            (|| {
                                anyhow::ensure!(
                                    before < scheduled_reducer && scheduled_reducer < after,
                                    "Expected scheduled reducer insertion order procedure_before < scheduled_reducer < procedure_after, got {before} < {scheduled_reducer} < {after}"
                                );
                                Ok(())
                            })(),
                        );
                    }
                }
            });

            subscribe_all_then(ctx, move |ctx| {
                sub_applied_nothing_result(assert_all_tables_empty(ctx));
                ctx.procedures
                    .procedure_schedule_reducer_between_inserts_then(move |_ctx, res| {
                        procedure_callback_result(
                            res.context("procedure_schedule_reducer_between_inserts failed unexpectedly"),
                        );
                    });
            });
        }
    })
    .await;

    test_counter.wait_for_all().await;
}
