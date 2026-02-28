#![allow(clippy::disallowed_macros)]

#[allow(clippy::too_many_arguments)]
#[allow(clippy::large_enum_variant)]
mod module_bindings;

use module_bindings::*;
use spacetimedb_sdk::error::InternalError;
use spacetimedb_sdk::{DbContext, Table, TableWithPrimaryKey};
use std::sync::Arc;
use test_counter::TestCounter;

const LOCALHOST: &str = "http://localhost:3000";

fn db_name_or_panic() -> String {
    std::env::var("SPACETIME_SDK_TEST_DB_NAME").expect("Failed to read db name from env")
}

fn exit_on_panic() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        default_hook(panic_info);
        std::process::exit(1);
    }));
}

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

fn main() {
    env_logger::init();
    exit_on_panic();

    let test = std::env::args()
        .nth(1)
        .expect("Pass a test name as a command-line argument to the test client");

    match &*test {
        "insert-player" => exec_insert_player(),
        "insert-person" => exec_insert_person(),
        "ban-player" => exec_ban_player(),
        "query-builder-filter" => exec_query_builder_filter(),
        "query-builder-join" => exec_query_builder_join(),
        "view" => exec_view(),
        _ => panic!("Unknown test: {test}"),
    }
}

type ResultRecorder = Box<dyn Send + FnOnce(anyhow::Result<()>)>;

fn put_result(result: &mut Option<ResultRecorder>, res: anyhow::Result<()>) {
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
    test_counter: &Arc<TestCounter>,
    callback: impl FnOnce(&DbConnection) + Send + 'static,
) -> DbConnection {
    let connected_result = test_counter.add_test("on_connect");
    let name = db_name_or_panic();
    let conn = DbConnection::builder()
        .with_database_name(name)
        .with_uri(LOCALHOST)
        .on_connect(move |ctx, _, _| {
            callback(ctx);
            connected_result(Ok(()));
        })
        .on_connect_error(|_ctx, error| panic!("Connect errored: {error:?}"))
        .build()
        .unwrap();
    conn.run_threaded();
    conn
}

/// Test: Insert a player via CreatePlayer1 reducer using query builder subscription.
/// Verifies that table accessor `player_1()`, field names with digits
/// (`player_1_id`, `current_level_2`, `status_3_field`), and enum variant
/// `Player2Status::Active1` all work correctly through case conversion.
fn exec_insert_player() {
    let test_counter = TestCounter::new();
    let mut insert_result = Some(test_counter.add_test("insert_player"));

    connect_then(&test_counter, move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.player_1().on_insert(move |_ctx, row| {
                    let check = || {
                        // Verify field names with digit boundaries are correctly case-converted
                        assert_eq_or_bail!("Alice".to_string(), row.player_name);
                        assert_eq_or_bail!(5u32, row.current_level_2);
                        assert_eq_or_bail!(Player2Status::Active1, row.status_3_field);
                        Ok(())
                    };
                    put_result(&mut insert_result, check());
                });

                ctx.reducers()
                    .create_player_1_then(
                        "Alice".to_string(),
                        5,
                        reducer_callback_assert_committed("create_player_1"),
                    )
                    .unwrap();
            })
            // Query builder: subscribe to player_1 table (canonical: Player1Canonical)
            .add_query(|q| q.from.player_1().build())
            .subscribe();
    });

    test_counter.wait_for_all();
}

/// Test: Insert a person via AddPerson2 reducer using query builder subscription.
/// Verifies nested struct `Person3Info` with digit-boundary fields
/// (`age_value_1`, `score_total`), index on `player_ref`, and
/// table accessor `person_2()`.
fn exec_insert_person() {
    let test_counter = TestCounter::new();
    let mut insert_person = Some(test_counter.add_test("insert_person"));

    connect_then(&test_counter, move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.person_2().on_insert(move |_ctx, person| {
                    let check = || {
                        assert_eq_or_bail!("Bob".to_string(), person.first_name);
                        // Verify nested struct field names with digit boundaries
                        assert_eq_or_bail!(25u8, person.person_info.age_value_1);
                        assert_eq_or_bail!(1000u32, person.person_info.score_total);
                        Ok(())
                    };
                    put_result(&mut insert_person, check());
                });

                // Insert a player first, then add a person referencing them
                ctx.reducers()
                    .create_player_1_then("PlayerForPerson".to_string(), 1, move |ctx, outcome| match outcome {
                        Ok(Ok(())) => {
                            let player = ctx.db.player_1().iter().next().expect("Player should exist");
                            ctx.reducers()
                                .add_person_2_then(
                                    "Bob".to_string(),
                                    player.player_1_id,
                                    25,
                                    1000,
                                    reducer_callback_assert_committed("add_person_2"),
                                )
                                .unwrap();
                        }
                        Ok(Err(msg)) => panic!("create_player_1 returned error: {msg}"),
                        Err(e) => panic!("create_player_1 panicked: {e:?}"),
                    })
                    .unwrap();
            })
            // Query builder: subscribe to both tables
            .add_query(|q| q.from.player_1().build())
            .add_query(|q| q.from.person_2().build())
            .subscribe();
    });

    test_counter.wait_for_all();
}

/// Test: Ban a player via BanPlayer1 reducer (which has explicit name `banPlayer1`).
/// Verifies that reducers with explicit names work, and that updating a player's
/// status from `Active1` to `BannedUntil(timestamp)` is reflected correctly.
fn exec_ban_player() {
    let test_counter = TestCounter::new();
    let mut update_result = Some(test_counter.add_test("ban_player_update"));

    connect_then(&test_counter, move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.player_1().on_update(move |_ctx, _old, new| {
                    let check = || {
                        assert_eq_or_bail!(Player2Status::BannedUntil(9999), new.status_3_field);
                        Ok(())
                    };
                    put_result(&mut update_result, check());
                });

                // Insert a player, then ban them in the reducer callback
                ctx.reducers()
                    .create_player_1_then("ToBan".to_string(), 1, move |ctx, outcome| match outcome {
                        Ok(Ok(())) => {
                            let player = ctx.db.player_1().iter().next().expect("Player should exist");
                            ctx.reducers()
                                .ban_player_1_then(
                                    player.player_1_id,
                                    9999,
                                    reducer_callback_assert_committed("ban_player_1"),
                                )
                                .unwrap();
                        }
                        Ok(Err(msg)) => panic!("create_player_1 returned error: {msg}"),
                        Err(e) => panic!("create_player_1 panicked: {e:?}"),
                    })
                    .unwrap();
            })
            // Query builder: subscribe to player_1 table
            .add_query(|q| q.from.player_1().build())
            .subscribe();
    });

    test_counter.wait_for_all();
}

/// Test: Query builder with a filter on a digit-boundary column.
/// Subscribes to player_1 rows WHERE current_level_2 == 5, verifying that
/// the case-converted column name works correctly in query builder filters.
fn exec_query_builder_filter() {
    let test_counter = TestCounter::new();
    let mut insert_match = Some(test_counter.add_test("insert_matching_filter"));

    connect_then(&test_counter, move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.player_1().on_insert(move |_ctx, row| {
                    let check = || {
                        // Only level-5 players should come through the filter
                        assert_eq_or_bail!(5u32, row.current_level_2);
                        assert_eq_or_bail!("FilterMatch".to_string(), row.player_name);
                        Ok(())
                    };
                    put_result(&mut insert_match, check());
                });

                // Insert a player at level 3 (should NOT match filter)
                ctx.reducers()
                    .create_player_1_then(
                        "NoMatch".to_string(),
                        3,
                        reducer_callback_assert_committed("create_player_1"),
                    )
                    .unwrap();

                // Insert a player at level 5 (should match filter)
                ctx.reducers()
                    .create_player_1_then(
                        "FilterMatch".to_string(),
                        5,
                        reducer_callback_assert_committed("create_player_1"),
                    )
                    .unwrap();
            })
            // Query builder: filter on digit-boundary column current_level_2
            .add_query(|q| q.from.player_1().filter(|p| p.current_level_2.eq(5)).build())
            .subscribe();
    });

    test_counter.wait_for_all();
}

/// Test: Query builder with a JOIN between player_1 and person_2.
/// Uses a right semijoin: person_2 results from player_1 JOIN person_2.
/// This tests that:
/// - Digit-boundary column names work in join predicates
/// - The query builder correctly resolves canonical table names for both tables
/// - Joined results are received correctly through case-converted accessors
/// - The view accessor `person_at_level_2()` returns Person2 rows
fn exec_query_builder_join() {
    let test_counter = TestCounter::new();
    let mut join_result = Some(test_counter.add_test("join_insert"));

    connect_then(&test_counter, move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                // Listen for person_2 inserts that come through the join.
                // The join is: player_1 RIGHT SEMIJOIN person_2 ON player_1.player_1_id = person_2.player_ref
                // This means we see person_2 rows that have a matching player_1 row.
                ctx.db.person_2().on_insert(move |_ctx, row| {
                    // Only care about inserts from our join subscription
                    if row.first_name == "JoinPerson" {
                        let check = || {
                            assert_eq_or_bail!("JoinPerson".to_string(), row.first_name);
                            assert_eq_or_bail!(30u8, row.person_info.age_value_1);
                            assert_eq_or_bail!(500u32, row.person_info.score_total);
                            Ok(())
                        };
                        put_result(&mut join_result, check());
                    }
                });

                // Insert a player first
                ctx.reducers()
                    .create_player_1_then("JoinedPlayer".to_string(), 7, move |ctx, outcome| {
                        match outcome {
                            Ok(Ok(())) => {
                                let player = ctx
                                    .db
                                    .player_1()
                                    .iter()
                                    .find(|p| p.player_name == "JoinedPlayer")
                                    .expect("JoinedPlayer should exist");

                                // Insert a person referencing this player — triggers the join
                                ctx.reducers()
                                    .add_person_2_then(
                                        "JoinPerson".to_string(),
                                        player.player_1_id,
                                        30,
                                        500,
                                        reducer_callback_assert_committed("add_person_2"),
                                    )
                                    .unwrap();
                            }
                            Ok(Err(msg)) => panic!("create_player_1 returned error: {msg}"),
                            Err(e) => panic!("create_player_1 panicked: {e:?}"),
                        }
                    })
                    .unwrap();
            })
            // Query builder: JOIN player_1 with person_2 on player_1_id = player_ref
            // player_1 RIGHT SEMIJOIN person_2 means: show person_2 rows that have a matching player_1
            .add_query(|q| {
                q.from
                    .player_1()
                    .right_semijoin(q.from.person_2(), |player, person| {
                        player.player_1_id.eq(person.player_ref)
                    })
                    .build()
            })
            // Also subscribe to player_1 so reducer callbacks can see inserted players
            .add_query(|q| q.from.player_1().build())
            .subscribe();
    });

    test_counter.wait_for_all();
}

/// Query view named Level2Person
fn exec_view() {
    let test_counter = TestCounter::new();
    let mut view_result = Some(test_counter.add_test("view_query"));

    connect_then(&test_counter, move |ctx| {
        ctx.subscription_builder()
            .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
            .on_applied(move |ctx| {
                ctx.db.person_at_level_2().on_insert(move |_ctx, row| {
                    let check = || {
                        assert_eq_or_bail!("ViewPerson".to_string(), row.first_name);
                        assert_eq_or_bail!(20u8, row.person_info.age_value_1);
                        assert_eq_or_bail!(200u32, row.person_info.score_total);
                        Ok(())
                    };
                    put_result(&mut view_result, check());
                });

                // Insert a player at level 2
                ctx.reducers()
                    .create_player_1_then(
                        "Level2Player".to_string(),
                        2,
                        reducer_callback_assert_committed("create_player_1"),
                    )
                    .unwrap();

                // Insert a person referencing that player — should appear in the view
                ctx.reducers()
                    .add_person_2_then(
                        "ViewPerson".to_string(),
                        1, // player_ref of 1 matches the first inserted player
                        20,
                        200,
                        reducer_callback_assert_committed("add_person_2"),
                    )
                    .unwrap();
            })
            // Subscribe to the view which selects people at level 2
            .add_query(|q| q.from.person_at_level_2().build())
            .subscribe();
    });

    test_counter.wait_for_all();
}
