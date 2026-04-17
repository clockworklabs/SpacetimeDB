use crate::module_bindings::*;
use spacetimedb_lib::Identity;
use spacetimedb_sdk::{error::InternalError, DbConnectionBuilder, DbContext, WithDelete, WithInsert};
use test_counter::TestCounter;

const LOCALHOST: &str = "http://localhost:3000";

pub async fn dispatch(test: &str, db_name: &str) {
    match test {
        "view-anonymous-subscribe" => exec_anonymous_subscribe(db_name).await,
        "view-anonymous-subscribe-with-query-builder" => exec_anonymous_subscribe_with_query_builder(db_name).await,
        "view-non-anonymous-subscribe" => exec_non_anonymous_subscribe(db_name).await,

        "view-non-table-return" => exec_non_table_return(db_name).await,
        "view-non-table-query-builder-return" => exec_non_table_query_builder_return(db_name).await,
        "view-subscription-update" => exec_subscription_update(db_name).await,
        "view-disconnect-does-not-break-sender-updates" => exec_disconnect_sender_view_updates(db_name).await,
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

async fn build_connection(
    db_name: &str,
    with_builder: impl FnOnce(DbConnectionBuilder<RemoteModule>) -> DbConnectionBuilder<RemoteModule>,
    callback: impl FnOnce(&DbConnection) + Send + 'static,
) -> DbConnection {
    let name = db_name.to_owned();
    let builder = DbConnection::builder()
        .with_database_name(name)
        .with_uri(LOCALHOST)
        .on_connect(|ctx, _, _| {
            callback(ctx);
        })
        .on_connect_error(|_ctx, error| panic!("Connect errored: {error:?}"));
    build_and_run(with_builder(builder)).await
}

async fn connect_with_then(
    db_name: &str,
    test_counter: &std::sync::Arc<TestCounter>,
    on_connect_suffix: &str,
    with_builder: impl FnOnce(DbConnectionBuilder<RemoteModule>) -> DbConnectionBuilder<RemoteModule>,
    callback: impl FnOnce(&DbConnection) + Send + 'static,
) -> DbConnection {
    let connected_result = test_counter.add_test(format!("on_connect_{on_connect_suffix}"));
    build_connection(db_name, with_builder, move |ctx| {
        callback(ctx);
        connected_result(Ok(()));
    })
    .await
}

async fn connect_then(
    db_name: &str,
    test_counter: &std::sync::Arc<TestCounter>,
    callback: impl FnOnce(&DbConnection) + Send + 'static,
) -> DbConnection {
    connect_with_then(db_name, test_counter, "", |x| x, callback).await
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

type ResultRecorder = Box<dyn Send + FnOnce(Result<(), anyhow::Error>)>;

async fn connect_my_player_client(
    db_name: &str,
    mut inserted_result: Option<ResultRecorder>,
    mut deleted_result: Option<ResultRecorder>,
    disconnected_result: Option<ResultRecorder>,
) -> DbConnection {
    // Subscribe to an identity-filtered view and immediately create one matching row for this client.
    build_connection(
        db_name,
        |builder| {
            builder.on_disconnect(move |ctx, error| {
                assert!(
                    !ctx.is_active(),
                    "on_disconnect callback, but `ctx.is_active()` is true"
                );
                if let Some(disconnected_result) = disconnected_result {
                    match error {
                        Some(error) => disconnected_result(Err(anyhow::anyhow!("{error:?}"))),
                        None => disconnected_result(Ok(())),
                    }
                } else if let Some(error) = error {
                    panic!("Disconnect errored: {error:?}");
                }
            })
        },
        move |ctx| {
            subscribe_these_then(ctx, &["SELECT * FROM my_player"], {
                move |ctx| {
                    let my_identity = ctx.identity();

                    ctx.db.my_player().on_insert(move |_, player| {
                        if player.identity == my_identity && inserted_result.is_some() {
                            put_result(&mut inserted_result, Ok(()));
                        }
                    });

                    ctx.db.my_player().on_delete(move |_, player| {
                        if player.identity == my_identity && deleted_result.is_some() {
                            put_result(&mut deleted_result, Ok(()));
                        }
                    });

                    ctx.reducers()
                        .insert_player_then(my_identity, 0, reducer_callback_assert_committed("insert_player"))
                        .unwrap();
                }
            });
        },
    )
    .await
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

async fn exec_anonymous_subscribe(db_name: &str) {
    let test_counter = TestCounter::new();
    let mut insert_0 = Some(test_counter.add_test("insert_0"));
    let mut insert_1 = Some(test_counter.add_test("insert_1"));
    let mut delete_1 = Some(test_counter.add_test("delete_1"));
    connect_then(db_name, &test_counter, move |ctx| {
        subscribe_these_then(ctx, &["SELECT * FROM players_at_level_0"], move |ctx| {
            ctx.db.players_at_level_0().on_insert(move |_, player| {
                if player.identity == Identity::from_byte_array([2; 32]) {
                    return put_result(&mut insert_0, Ok(()));
                }
                if player.identity == Identity::from_byte_array([4; 32]) {
                    return put_result(&mut insert_1, Ok(()));
                }
                unreachable!("Unexpected identity on insert: `{}`", player.identity)
            });
            ctx.db.players_at_level_0().on_delete(move |_, player| {
                if player.identity == Identity::from_byte_array([4; 32]) {
                    return put_result(&mut delete_1, Ok(()));
                }
                unreachable!("Unexpected identity on delete: `{}`", player.identity)
            });
            ctx.reducers()
                .insert_player_then(
                    Identity::from_byte_array([1; 32]),
                    1,
                    reducer_callback_assert_committed("insert_player"),
                )
                .unwrap();
            ctx.reducers()
                .insert_player_then(
                    Identity::from_byte_array([2; 32]),
                    0,
                    reducer_callback_assert_committed("insert_player"),
                )
                .unwrap();
            ctx.reducers()
                .insert_player_then(
                    Identity::from_byte_array([3; 32]),
                    1,
                    reducer_callback_assert_committed("insert_player"),
                )
                .unwrap();
            ctx.reducers()
                .insert_player_then(
                    Identity::from_byte_array([4; 32]),
                    0,
                    reducer_callback_assert_committed("insert_player"),
                )
                .unwrap();
            ctx.reducers()
                .delete_player_then(
                    Identity::from_byte_array([4; 32]),
                    reducer_callback_assert_committed("insert_player"),
                )
                .unwrap();
        });
    })
    .await;
    test_counter.wait_for_all().await;
}

async fn exec_anonymous_subscribe_with_query_builder(db_name: &str) {
    let test_counter = TestCounter::new();
    let mut insert_0 = Some(test_counter.add_test("insert_0"));
    let mut insert_1 = Some(test_counter.add_test("insert_1"));
    let mut delete_1 = Some(test_counter.add_test("delete_1"));
    connect_then(db_name, &test_counter, move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.player().on_insert(move |_, player| {
                    if player.identity == Identity::from_byte_array([2; 32]) {
                        return put_result(&mut insert_0, Ok(()));
                    }
                    if player.identity == Identity::from_byte_array([4; 32]) {
                        return put_result(&mut insert_1, Ok(()));
                    }
                    unreachable!("Unexpected identity on insert: `{}`", player.identity)
                });
                ctx.db.player().on_delete(move |_, player| {
                    if player.identity == Identity::from_byte_array([4; 32]) {
                        return put_result(&mut delete_1, Ok(()));
                    }
                    unreachable!("Unexpected identity on delete: `{}`", player.identity)
                });
                ctx.reducers()
                    .insert_player_then(
                        Identity::from_byte_array([1; 32]),
                        1,
                        reducer_callback_assert_committed("insert_player"),
                    )
                    .unwrap();
                ctx.reducers()
                    .insert_player_then(
                        Identity::from_byte_array([2; 32]),
                        0,
                        reducer_callback_assert_committed("insert_player"),
                    )
                    .unwrap();
                ctx.reducers()
                    .insert_player_then(
                        Identity::from_byte_array([3; 32]),
                        1,
                        reducer_callback_assert_committed("insert_player"),
                    )
                    .unwrap();
                ctx.reducers()
                    .insert_player_then(
                        Identity::from_byte_array([4; 32]),
                        0,
                        reducer_callback_assert_committed("insert_player"),
                    )
                    .unwrap();
                ctx.reducers()
                    .delete_player_then(
                        Identity::from_byte_array([4; 32]),
                        reducer_callback_assert_committed("delete_player"),
                    )
                    .unwrap();
            })
            .add_query(|ctx| {
                ctx.from
                    .player_level()
                    .filter(|pl| pl.level.eq(0))
                    .right_semijoin(ctx.from.player(), |lvl, pl| lvl.entity_id.eq(pl.entity_id))
                    .build()
            })
            .subscribe();
    })
    .await;
    test_counter.wait_for_all().await;
}

async fn exec_non_anonymous_subscribe(db_name: &str) {
    let test_counter = TestCounter::new();
    let mut insert = Some(test_counter.add_test("insert"));
    let mut delete = Some(test_counter.add_test("delete"));
    connect_then(db_name, &test_counter, move |ctx| {
        subscribe_these_then(ctx, &["SELECT * FROM my_player"], move |ctx| {
            let my_identity = ctx.identity();
            ctx.db.my_player().on_insert(move |_, player| {
                assert_eq!(player.identity, my_identity);
                put_result(&mut insert, Ok(()));
            });
            ctx.db.my_player().on_delete(move |_, player| {
                assert_eq!(player.identity, my_identity);
                put_result(&mut delete, Ok(()));
            });
            ctx.reducers()
                .insert_player_then(
                    Identity::from_byte_array([1; 32]),
                    0,
                    reducer_callback_assert_committed("insert_player"),
                )
                .unwrap();
            ctx.reducers()
                .insert_player_then(my_identity, 0, reducer_callback_assert_committed("insert_player"))
                .unwrap();
            ctx.reducers()
                .delete_player_then(
                    Identity::from_byte_array([1; 32]),
                    reducer_callback_assert_committed("insert_player"),
                )
                .unwrap();
            ctx.reducers()
                .delete_player_then(my_identity, reducer_callback_assert_committed("delete_player"))
                .unwrap();
        });
    })
    .await;
    test_counter.wait_for_all().await;
}

async fn exec_non_table_return(db_name: &str) {
    let test_counter = TestCounter::new();
    let mut insert = Some(test_counter.add_test("insert"));
    let mut delete = Some(test_counter.add_test("delete"));
    connect_then(db_name, &test_counter, move |ctx| {
        subscribe_these_then(ctx, &["SELECT * FROM my_player_and_level"], move |ctx| {
            let my_identity = ctx.identity();
            ctx.db.my_player_and_level().on_insert(move |_, player| {
                assert_eq!(player.identity, my_identity);
                assert_eq!(player.level, 1);
                put_result(&mut insert, Ok(()));
            });
            ctx.db.my_player_and_level().on_delete(move |_, player| {
                assert_eq!(player.identity, my_identity);
                assert_eq!(player.level, 1);
                put_result(&mut delete, Ok(()));
            });
            ctx.reducers()
                .insert_player_then(
                    Identity::from_byte_array([1; 32]),
                    0,
                    reducer_callback_assert_committed("insert_player"),
                )
                .unwrap();
            ctx.reducers()
                .insert_player_then(my_identity, 1, reducer_callback_assert_committed("insert_player"))
                .unwrap();
            ctx.reducers()
                .delete_player_then(
                    Identity::from_byte_array([1; 32]),
                    reducer_callback_assert_committed("delete_player"),
                )
                .unwrap();
            ctx.reducers()
                .delete_player_then(my_identity, reducer_callback_assert_committed("delete_player"))
                .unwrap();
        });
    })
    .await;
    test_counter.wait_for_all().await;
}

async fn exec_non_table_query_builder_return(db_name: &str) {
    let test_counter = TestCounter::new();
    let mut insert = Some(test_counter.add_test("insert"));
    let mut delete = Some(test_counter.add_test("delete"));
    connect_then(db_name, &test_counter, move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                let my_identity = ctx.identity();
                ctx.db.my_player_and_level().on_insert(move |_, player| {
                    assert_eq!(player.identity, my_identity);
                    assert_eq!(player.level, 1);
                    put_result(&mut insert, Ok(()));
                });
                ctx.db.my_player_and_level().on_delete(move |_, player| {
                    assert_eq!(player.identity, my_identity);
                    assert_eq!(player.level, 1);
                    put_result(&mut delete, Ok(()));
                });
                ctx.reducers()
                    .insert_player_then(
                        Identity::from_byte_array([1; 32]),
                        0,
                        reducer_callback_assert_committed("insert_player"),
                    )
                    .unwrap();
                ctx.reducers()
                    .insert_player_then(my_identity, 1, reducer_callback_assert_committed("insert_player"))
                    .unwrap();

                ctx.reducers()
                    .delete_player_then(
                        Identity::from_byte_array([1; 32]),
                        reducer_callback_assert_committed("delete_player"),
                    )
                    .unwrap();
                ctx.reducers()
                    .delete_player_then(my_identity, reducer_callback_assert_committed("delete_player"))
                    .unwrap();
            })
            .add_query(|q_ctx| q_ctx.from.my_player_and_level().filter(|p| p.level.eq(1)).build())
            .subscribe();
    })
    .await;
    test_counter.wait_for_all().await;
}

async fn exec_subscription_update(db_name: &str) {
    let test_counter = TestCounter::new();

    let mut insert_0 = Some(test_counter.add_test("insert_0"));
    let mut delete_0 = Some(test_counter.add_test("delete_0"));

    connect_with_then(
        db_name,
        &test_counter,
        "0",
        |builder| builder,
        move |ctx| {
            subscribe_these_then(ctx, &["SELECT * FROM nearby_players"], move |ctx| {
                ctx.db.nearby_players().on_insert(move |_, loc| {
                    assert_eq!(loc.x, 2);
                    assert_eq!(loc.y, 2);
                    put_result(&mut insert_0, Ok(()));
                });
                ctx.db.nearby_players().on_delete(move |_, loc| {
                    assert_eq!(loc.x, 2);
                    assert_eq!(loc.y, 2);
                    put_result(&mut delete_0, Ok(()));
                });
                // Insert player 0 at coords (0, 0)
                ctx.reducers()
                    .move_player_then(0, 0, reducer_callback_assert_committed("move_player"))
                    .unwrap();
            });
        },
    )
    .await;

    let mut insert_1 = Some(test_counter.add_test("insert_1"));
    let mut delete_1 = Some(test_counter.add_test("delete_1"));

    connect_with_then(
        db_name,
        &test_counter,
        "1",
        |builder| builder,
        move |ctx| {
            subscribe_these_then(ctx, &["SELECT * FROM nearby_players"], move |ctx| {
                ctx.db.nearby_players().on_insert(move |ctx, loc| {
                    assert_eq!(loc.x, 0);
                    assert_eq!(loc.y, 0);
                    put_result(&mut insert_1, Ok(()));
                    // Move player 1 outside of visible region
                    ctx.reducers()
                        .move_player_then(3, 3, reducer_callback_assert_committed("move_player"))
                        .unwrap();
                });
                ctx.db.nearby_players().on_delete(move |_, loc| {
                    assert_eq!(loc.x, 0);
                    assert_eq!(loc.y, 0);
                    put_result(&mut delete_1, Ok(()));
                });
                // Insert player 1 at coords (2, 2)
                ctx.reducers()
                    .move_player_then(2, 2, reducer_callback_assert_committed("move_player"))
                    .unwrap();
            });
        },
    )
    .await;
    test_counter.wait_for_all().await;
}

async fn exec_disconnect_sender_view_updates(db_name: &str) {
    let conn1_insert_counter = TestCounter::new();
    let conn1_delete_counter = TestCounter::new();
    let conn1 = connect_my_player_client(
        db_name,
        Some(conn1_insert_counter.add_test("conn1 initial insert")),
        Some(conn1_delete_counter.add_test("conn1 delete after disconnect")),
        None,
    )
    .await;
    conn1_insert_counter.wait_for_all().await;

    let conn2_insert_counter = TestCounter::new();
    let conn2_disconnect_counter = TestCounter::new();
    let conn2 = connect_my_player_client(
        db_name,
        Some(conn2_insert_counter.add_test("conn2 initial insert")),
        None,
        Some(conn2_disconnect_counter.add_test("conn2 disconnect")),
    )
    .await;
    conn2_insert_counter.wait_for_all().await;

    // After client 2 disconnects, client 1 should still receive deletes from the sender-scoped view.
    conn2.disconnect().unwrap();
    conn2_disconnect_counter.wait_for_all().await;

    conn1
        .reducers()
        .delete_player_then(conn1.identity(), reducer_callback_assert_committed("delete_player"))
        .unwrap();
    conn1_delete_counter.wait_for_all().await;

    conn1.disconnect().unwrap();
}
