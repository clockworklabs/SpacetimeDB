mod module_bindings;

use module_bindings::*;
use spacetimedb_lib::Identity;
use spacetimedb_sdk::{DbConnectionBuilder, DbContext, Table};
use test_counter::TestCounter;

const LOCALHOST: &str = "http://localhost:3000";

/// Register a panic hook which will exit the process whenever any thread panics.
///
/// This allows us to fail tests by panicking in callbacks.
fn exit_on_panic() {
    // The default panic hook is responsible for printing the panic message and backtrace to stderr.
    // Grab a handle on it, and invoke it in our custom hook before exiting.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Print panic information
        default_hook(panic_info);

        // Exit the process with a non-zero code to denote failure.
        std::process::exit(1);
    }));
}

fn db_name_or_panic() -> String {
    std::env::var("SPACETIME_SDK_TEST_DB_NAME").expect("Failed to read db name from env")
}

fn main() {
    env_logger::init();
    exit_on_panic();

    let test = std::env::args()
        .nth(1)
        .expect("Pass a test name as a command-line argument to the test client");

    match &*test {
        "view-anonymous-subscribe" => exec_anonymous_subscribe(),
        "view-anonymous-subscribe-with-query-builder" => exec_anonymous_subscribe_with_query_builder(),
        "view-non-anonymous-subscribe" => exec_non_anonymous_subscribe(),

        "view-non-table-return" => exec_non_table_return(),
        "view-non-table-query-builder-return" => exec_non_table_query_builder_return(),
        "view-subscription-update" => exec_subscription_update(),
        _ => panic!("Unknown test: {test}"),
    }
}

fn connect_with_then(
    test_counter: &std::sync::Arc<TestCounter>,
    on_connect_suffix: &str,
    with_builder: impl FnOnce(DbConnectionBuilder<RemoteModule>) -> DbConnectionBuilder<RemoteModule>,
    callback: impl FnOnce(&DbConnection) + Send + 'static,
) -> DbConnection {
    let connected_result = test_counter.add_test(format!("on_connect_{on_connect_suffix}"));
    let name = db_name_or_panic();
    let builder = DbConnection::builder()
        .with_module_name(name)
        .with_uri(LOCALHOST)
        .on_connect(|ctx, _, _| {
            callback(ctx);
            connected_result(Ok(()));
        })
        .on_connect_error(|_ctx, error| panic!("Connect errored: {error:?}"));
    let conn = with_builder(builder).build().unwrap();
    conn.run_threaded();
    conn
}

fn connect_then(
    test_counter: &std::sync::Arc<TestCounter>,
    callback: impl FnOnce(&DbConnection) + Send + 'static,
) -> DbConnection {
    connect_with_then(test_counter, "", |x| x, callback)
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

fn put_result(result: &mut Option<ResultRecorder>, res: Result<(), anyhow::Error>) {
    (result.take().unwrap())(res);
}

fn exec_anonymous_subscribe() {
    let test_counter = TestCounter::new();
    let mut insert_0 = Some(test_counter.add_test("insert_0"));
    let mut insert_1 = Some(test_counter.add_test("insert_1"));
    let mut delete_1 = Some(test_counter.add_test("delete_1"));
    connect_then(&test_counter, move |ctx| {
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
                .insert_player(Identity::from_byte_array([1; 32]), 1)
                .unwrap();
            ctx.reducers()
                .insert_player(Identity::from_byte_array([2; 32]), 0)
                .unwrap();
            ctx.reducers()
                .insert_player(Identity::from_byte_array([3; 32]), 1)
                .unwrap();
            ctx.reducers()
                .insert_player(Identity::from_byte_array([4; 32]), 0)
                .unwrap();
            ctx.reducers()
                .delete_player(Identity::from_byte_array([4; 32]))
                .unwrap();
        });
    });
    test_counter.wait_for_all();
}

fn exec_anonymous_subscribe_with_query_builder() {
    let test_counter = TestCounter::new();
    let mut insert_0 = Some(test_counter.add_test("insert_0"));
    let mut insert_1 = Some(test_counter.add_test("insert_1"));
    let mut delete_1 = Some(test_counter.add_test("delete_1"));
    connect_then(&test_counter, move |ctx| {
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
                    .insert_player(Identity::from_byte_array([1; 32]), 1)
                    .unwrap();
                ctx.reducers()
                    .insert_player(Identity::from_byte_array([2; 32]), 0)
                    .unwrap();
                ctx.reducers()
                    .insert_player(Identity::from_byte_array([3; 32]), 1)
                    .unwrap();
                ctx.reducers()
                    .insert_player(Identity::from_byte_array([4; 32]), 0)
                    .unwrap();
                ctx.reducers()
                    .delete_player(Identity::from_byte_array([4; 32]))
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
    });
    test_counter.wait_for_all();
}

fn exec_non_anonymous_subscribe() {
    let test_counter = TestCounter::new();
    let mut insert = Some(test_counter.add_test("insert"));
    let mut delete = Some(test_counter.add_test("delete"));
    connect_then(&test_counter, move |ctx| {
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
                .insert_player(Identity::from_byte_array([1; 32]), 0)
                .unwrap();
            ctx.reducers().insert_player(my_identity, 0).unwrap();
            ctx.reducers()
                .delete_player(Identity::from_byte_array([1; 32]))
                .unwrap();
            ctx.reducers().delete_player(my_identity).unwrap();
        });
    });
    test_counter.wait_for_all();
}

fn exec_non_table_return() {
    let test_counter = TestCounter::new();
    let mut insert = Some(test_counter.add_test("insert"));
    let mut delete = Some(test_counter.add_test("delete"));
    connect_then(&test_counter, move |ctx| {
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
                .insert_player(Identity::from_byte_array([1; 32]), 0)
                .unwrap();
            ctx.reducers().insert_player(my_identity, 1).unwrap();
            ctx.reducers()
                .delete_player(Identity::from_byte_array([1; 32]))
                .unwrap();
            ctx.reducers().delete_player(my_identity).unwrap();
        });
    });
    test_counter.wait_for_all();
}

fn exec_non_table_query_builder_return() {
    let test_counter = TestCounter::new();
    let mut insert = Some(test_counter.add_test("insert"));
    let mut delete = Some(test_counter.add_test("delete"));
    connect_then(&test_counter, move |ctx| {
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
                    .insert_player(Identity::from_byte_array([1; 32]), 0)
                    .unwrap();
                ctx.reducers().insert_player(my_identity, 1).unwrap();

                ctx.reducers()
                    .delete_player(Identity::from_byte_array([1; 32]))
                    .unwrap();
                ctx.reducers().delete_player(my_identity).unwrap();
            })
            .add_query(|q_ctx| q_ctx.from.my_player_and_level().filter(|p| p.level.eq(1)).build())
            .subscribe();
    });
    test_counter.wait_for_all();
}

fn exec_subscription_update() {
    let test_counter = TestCounter::new();

    let mut insert_0 = Some(test_counter.add_test("insert_0"));
    let mut delete_0 = Some(test_counter.add_test("delete_0"));

    connect_with_then(
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
                ctx.reducers().move_player(0, 0).unwrap();
            });
        },
    );

    let mut insert_1 = Some(test_counter.add_test("insert_1"));
    let mut delete_1 = Some(test_counter.add_test("delete_1"));

    connect_with_then(
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
                    ctx.reducers().move_player(3, 3).unwrap();
                });
                ctx.db.nearby_players().on_delete(move |_, loc| {
                    assert_eq!(loc.x, 0);
                    assert_eq!(loc.y, 0);
                    put_result(&mut delete_1, Ok(()));
                });
                // Insert player 1 at coords (2, 2)
                ctx.reducers().move_player(2, 2).unwrap();
            });
        },
    );
    test_counter.wait_for_all();
}
