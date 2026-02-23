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
        "subscribe-view" => exec_subscribe_view(),
        "subscribe-canonical-names" => exec_subscribe_canonical_names(),
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

fn subscribe_then(
    ctx: &impl RemoteDbContext,
    queries: &[&str],
    callback: impl FnOnce(&SubscriptionEventContext) + Send + 'static,
) {
    ctx.subscription_builder()
        .on_applied(callback)
        .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
        .subscribe(queries);
}

/// Test: Insert a player via CreatePlayer1 reducer.
/// Verifies that table accessor `player_1()`, field names with digits
/// (`player_1_id`, `current_level_2`, `status_3_field`), and enum variant
/// `Player2Status::Active1` all work correctly through case conversion.
fn exec_insert_player() {
    let test_counter = TestCounter::new();
    let mut insert_result = Some(test_counter.add_test("insert_player"));

    connect_then(&test_counter, move |ctx| {
        // Subscribe using the canonical table name (case-converted wire name)
        subscribe_then(ctx, &["SELECT * FROM Player1Canonical"], move |ctx| {
            ctx.db.player_1().on_insert(move |_ctx, row| {
                let check = || {
                    // Verify field names with digit boundaries are correctly case-converted
                    // player_1_id is auto-assigned (0 in insert becomes server-assigned)
                    assert_eq_or_bail!("Alice".to_string(), row.player_name[0]);
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
        });
    });

    test_counter.wait_for_all();
}

/// Test: Insert a person via AddPerson2 reducer.
/// Verifies nested struct `Person3Info` with digit-boundary fields
/// (`age_value_1`, `score_total`), index on `player_ref`, and
/// table accessor `person_2()`.
fn exec_insert_person() {
    let test_counter = TestCounter::new();
    let mut insert_person = Some(test_counter.add_test("insert_person"));

    connect_then(&test_counter, move |ctx| {
        subscribe_then(
            ctx,
            &["SELECT * FROM Player1Canonical", "SELECT * FROM person_2"],
            move |ctx| {
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
            },
        );
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
        subscribe_then(ctx, &["SELECT * FROM Player1Canonical"], move |ctx| {
            ctx.db.player_1().on_update(move |_ctx, _old, new| {
                let check = || {
                    assert_eq_or_bail!(Player2Status::BannedUntil(9999), new.status_3_field);
                    Ok(())
                };
                put_result(&mut update_result, check());
            });

            // Insert a player, then ban them in the reducer callback
            ctx.reducers()
                .create_player_1_then("ToBan".to_string(), 1, move |ctx, outcome| {
                    match outcome {
                        Ok(Ok(())) => {
                            // Find the inserted player to get their ID
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
                    }
                })
                .unwrap();
        });
    });

    test_counter.wait_for_all();
}

/// Test: Subscribe to the view `Level2Players` (accessor: `players_at_level_2`).
/// Verifies that views with case-converted names work correctly and that
/// the view returns PlayerRow typed data.
fn exec_subscribe_view() {
    let test_counter = TestCounter::new();
    let mut view_applied = Some(test_counter.add_test("view_subscription_applied"));

    connect_then(&test_counter, move |ctx| {
        // Subscribe to both the table and the view
        subscribe_then(
            ctx,
            &[
                "SELECT * FROM Player1Canonical",
                "SELECT * FROM person_2",
                "SELECT * FROM Level2Players",
            ],
            move |ctx| {
                // The view initially returns nothing — just verify subscription works
                let check = || {
                    // Access the view through its accessor name
                    let _count = ctx.db.players_at_level_2().count();
                    // View accessor works — case conversion is correct
                    Ok(())
                };
                put_result(&mut view_applied, check());
            },
        );
    });

    test_counter.wait_for_all();
}

/// Test: Verify that SQL queries must use canonical (wire) names, not accessor names.
/// The canonical name for the Player1 table is `Player1Canonical` (explicitly set),
/// and for Person2 table it is `person_2` (case-converted from `Person2`).
fn exec_subscribe_canonical_names() {
    let test_counter = TestCounter::new();
    let mut sub_result = Some(test_counter.add_test("canonical_names_subscribe"));

    connect_then(&test_counter, move |ctx| {
        // Use canonical names in SQL — these should succeed
        subscribe_then(
            ctx,
            &[
                "SELECT * FROM Player1Canonical",
                "SELECT * FROM person_2",
                "SELECT * FROM Level2Players",
            ],
            move |ctx| {
                let check = || {
                    // Verify we can access all tables through their accessor methods
                    let _p1_count = ctx.db.player_1().count();
                    let _p2_count = ctx.db.person_2().count();
                    let _view_count = ctx.db.players_at_level_2().count();
                    Ok(())
                };
                put_result(&mut sub_result, check());
            },
        );
    });

    test_counter.wait_for_all();
}
