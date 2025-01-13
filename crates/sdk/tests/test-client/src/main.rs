#[allow(clippy::too_many_arguments)]
#[allow(clippy::large_enum_variant)]
mod module_bindings;

use core::fmt::Display;
use std::sync::{atomic::AtomicUsize, Arc, Mutex};

use module_bindings::*;

use spacetimedb_sdk::{
    credentials, i256, u256, unstable::CallReducerFlags, Address, DbConnectionBuilder, DbContext, Event, Identity,
    ReducerEvent, Status, SubscriptionHandle, Table,
};
use test_counter::TestCounter;

mod simple_test_table;
use simple_test_table::{insert_one, on_insert_one};

mod pk_test_table;
use pk_test_table::insert_update_delete_one;

mod unique_test_table;
use unique_test_table::insert_then_delete_one;

const LOCALHOST: &str = "http://localhost:3000";

fn db_name_or_panic() -> String {
    std::env::var("SPACETIME_SDK_TEST_DB_NAME").expect("Failed to read db name from env")
}

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

fn main() {
    env_logger::init();
    exit_on_panic();

    let test = std::env::args()
        .nth(1)
        .expect("Pass a test name as a command-line argument to the test client");

    match &*test {
        "insert_primitive" => exec_insert_primitive(),
        "subscribe_and_cancel" => exec_subscribe_and_cancel(),
        "subscribe_and_unsubscribe" => exec_subscribe_and_unsubscribe(),
        "subscription_error_smoke_test" => exec_subscription_error_smoke_test(),
        "delete_primitive" => exec_delete_primitive(),
        "update_primitive" => exec_update_primitive(),

        "insert_identity" => exec_insert_identity(),
        "insert_caller_identity" => exec_insert_caller_identity(),
        "delete_identity" => exec_delete_identity(),
        "update_identity" => exec_update_identity(),

        "insert_address" => exec_insert_address(),
        "insert_caller_address" => exec_insert_caller_address(),
        "delete_address" => exec_delete_address(),
        "update_address" => exec_update_address(),

        "on_reducer" => exec_on_reducer(),
        "fail_reducer" => exec_fail_reducer(),

        "insert_vec" => exec_insert_vec(),
        "insert_option_some" => exec_insert_option_some(),
        "insert_option_none" => exec_insert_option_none(),
        "insert_struct" => exec_insert_struct(),
        "insert_simple_enum" => exec_insert_simple_enum(),
        "insert_enum_with_payload" => exec_insert_enum_with_payload(),

        "insert_long_table" => exec_insert_long_table(),

        "insert_primitives_as_strings" => exec_insert_primitives_as_strings(),

        // "resubscribe" => exec_resubscribe(),
        //
        "reauth_part_1" => exec_reauth_part_1(),
        "reauth_part_2" => exec_reauth_part_2(),

        "should_fail" => exec_should_fail(),

        "reconnect_same_address" => exec_reconnect_same_address(),
        "caller_always_notified" => exec_caller_always_notified(),

        "subscribe_all_select_star" => exec_subscribe_all_select_star(),
        "caller_alice_receives_reducer_callback_but_not_bob" => {
            exec_caller_alice_receives_reducer_callback_but_not_bob()
        }
        _ => panic!("Unknown test: {}", test),
    }
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

/// Each test runs against a fresh DB, so all tables should be empty until we call an insert reducer.
///
/// We'll call this function within our initial `on_subscription_applied` callback to verify that.
fn assert_all_tables_empty(ctx: &impl RemoteDbContext) -> anyhow::Result<()> {
    assert_table_empty(ctx.db().one_u_8())?;
    assert_table_empty(ctx.db().one_u_16())?;
    assert_table_empty(ctx.db().one_u_32())?;
    assert_table_empty(ctx.db().one_u_64())?;
    assert_table_empty(ctx.db().one_u_128())?;
    assert_table_empty(ctx.db().one_u_256())?;

    assert_table_empty(ctx.db().one_i_8())?;
    assert_table_empty(ctx.db().one_i_16())?;
    assert_table_empty(ctx.db().one_i_32())?;
    assert_table_empty(ctx.db().one_i_64())?;
    assert_table_empty(ctx.db().one_i_128())?;
    assert_table_empty(ctx.db().one_i_256())?;

    assert_table_empty(ctx.db().one_bool())?;

    assert_table_empty(ctx.db().one_f_32())?;
    assert_table_empty(ctx.db().one_f_64())?;

    assert_table_empty(ctx.db().one_string())?;
    assert_table_empty(ctx.db().one_identity())?;
    assert_table_empty(ctx.db().one_address())?;

    assert_table_empty(ctx.db().one_simple_enum())?;
    assert_table_empty(ctx.db().one_enum_with_payload())?;

    assert_table_empty(ctx.db().one_unit_struct())?;
    assert_table_empty(ctx.db().one_byte_struct())?;
    assert_table_empty(ctx.db().one_every_primitive_struct())?;
    assert_table_empty(ctx.db().one_every_vec_struct())?;

    assert_table_empty(ctx.db().vec_u_8())?;
    assert_table_empty(ctx.db().vec_u_16())?;
    assert_table_empty(ctx.db().vec_u_32())?;
    assert_table_empty(ctx.db().vec_u_64())?;
    assert_table_empty(ctx.db().vec_u_128())?;
    assert_table_empty(ctx.db().vec_u_256())?;

    assert_table_empty(ctx.db().vec_i_8())?;
    assert_table_empty(ctx.db().vec_i_16())?;
    assert_table_empty(ctx.db().vec_i_32())?;
    assert_table_empty(ctx.db().vec_i_64())?;
    assert_table_empty(ctx.db().vec_i_128())?;
    assert_table_empty(ctx.db().vec_i_256())?;

    assert_table_empty(ctx.db().vec_bool())?;

    assert_table_empty(ctx.db().vec_f_32())?;
    assert_table_empty(ctx.db().vec_f_64())?;

    assert_table_empty(ctx.db().vec_string())?;
    assert_table_empty(ctx.db().vec_identity())?;
    assert_table_empty(ctx.db().vec_address())?;

    assert_table_empty(ctx.db().vec_simple_enum())?;
    assert_table_empty(ctx.db().vec_enum_with_payload())?;

    assert_table_empty(ctx.db().vec_unit_struct())?;
    assert_table_empty(ctx.db().vec_byte_struct())?;
    assert_table_empty(ctx.db().vec_every_primitive_struct())?;
    assert_table_empty(ctx.db().vec_every_vec_struct())?;

    assert_table_empty(ctx.db().option_i_32())?;
    assert_table_empty(ctx.db().option_string())?;
    assert_table_empty(ctx.db().option_identity())?;
    assert_table_empty(ctx.db().option_simple_enum())?;
    assert_table_empty(ctx.db().option_every_primitive_struct())?;
    assert_table_empty(ctx.db().option_vec_option_i_32())?;

    assert_table_empty(ctx.db().unique_u_8())?;
    assert_table_empty(ctx.db().unique_u_16())?;
    assert_table_empty(ctx.db().unique_u_32())?;
    assert_table_empty(ctx.db().unique_u_64())?;
    assert_table_empty(ctx.db().unique_u_128())?;
    assert_table_empty(ctx.db().unique_u_256())?;

    assert_table_empty(ctx.db().unique_i_8())?;
    assert_table_empty(ctx.db().unique_i_16())?;
    assert_table_empty(ctx.db().unique_i_32())?;
    assert_table_empty(ctx.db().unique_i_64())?;
    assert_table_empty(ctx.db().unique_i_128())?;
    assert_table_empty(ctx.db().unique_i_256())?;

    assert_table_empty(ctx.db().unique_bool())?;

    assert_table_empty(ctx.db().unique_string())?;
    assert_table_empty(ctx.db().unique_identity())?;
    assert_table_empty(ctx.db().unique_address())?;

    assert_table_empty(ctx.db().pk_u_8())?;
    assert_table_empty(ctx.db().pk_u_16())?;
    assert_table_empty(ctx.db().pk_u_32())?;
    assert_table_empty(ctx.db().pk_u_64())?;
    assert_table_empty(ctx.db().pk_u_128())?;
    assert_table_empty(ctx.db().pk_u_256())?;

    assert_table_empty(ctx.db().pk_i_8())?;
    assert_table_empty(ctx.db().pk_i_16())?;
    assert_table_empty(ctx.db().pk_i_32())?;
    assert_table_empty(ctx.db().pk_i_64())?;
    assert_table_empty(ctx.db().pk_i_128())?;
    assert_table_empty(ctx.db().pk_i_256())?;

    assert_table_empty(ctx.db().pk_bool())?;

    assert_table_empty(ctx.db().pk_string())?;
    assert_table_empty(ctx.db().pk_identity())?;
    assert_table_empty(ctx.db().pk_address())?;

    assert_table_empty(ctx.db().large_table())?;

    assert_table_empty(ctx.db().table_holds_table())?;

    Ok(())
}

/// A great big honking query that subscribes to all rows from all tables.
const SUBSCRIBE_ALL: &[&str] = &[
    "SELECT * FROM one_u8;",
    "SELECT * FROM one_u16;",
    "SELECT * FROM one_u32;",
    "SELECT * FROM one_u64;",
    "SELECT * FROM one_u128;",
    "SELECT * FROM one_u256;",
    "SELECT * FROM one_i8;",
    "SELECT * FROM one_i16;",
    "SELECT * FROM one_i32;",
    "SELECT * FROM one_i64;",
    "SELECT * FROM one_i128;",
    "SELECT * FROM one_i256;",
    "SELECT * FROM one_bool;",
    "SELECT * FROM one_f32;",
    "SELECT * FROM one_f64;",
    "SELECT * FROM one_string;",
    "SELECT * FROM one_identity;",
    "SELECT * FROM one_address;",
    "SELECT * FROM one_simple_enum;",
    "SELECT * FROM one_enum_with_payload;",
    "SELECT * FROM one_unit_struct;",
    "SELECT * FROM one_byte_struct;",
    "SELECT * FROM one_every_primitive_struct;",
    "SELECT * FROM one_every_vec_struct;",
    "SELECT * FROM vec_u8;",
    "SELECT * FROM vec_u16;",
    "SELECT * FROM vec_u32;",
    "SELECT * FROM vec_u64;",
    "SELECT * FROM vec_u128;",
    "SELECT * FROM vec_u256;",
    "SELECT * FROM vec_i8;",
    "SELECT * FROM vec_i16;",
    "SELECT * FROM vec_i32;",
    "SELECT * FROM vec_i64;",
    "SELECT * FROM vec_i128;",
    "SELECT * FROM vec_i256;",
    "SELECT * FROM vec_bool;",
    "SELECT * FROM vec_f32;",
    "SELECT * FROM vec_f64;",
    "SELECT * FROM vec_string;",
    "SELECT * FROM vec_identity;",
    "SELECT * FROM vec_address;",
    "SELECT * FROM vec_simple_enum;",
    "SELECT * FROM vec_enum_with_payload;",
    "SELECT * FROM vec_unit_struct;",
    "SELECT * FROM vec_byte_struct;",
    "SELECT * FROM vec_every_primitive_struct;",
    "SELECT * FROM vec_every_vec_struct;",
    "SELECT * FROM option_i32;",
    "SELECT * FROM option_string;",
    "SELECT * FROM option_identity;",
    "SELECT * FROM option_simple_enum;",
    "SELECT * FROM option_every_primitive_struct;",
    "SELECT * FROM option_vec_option_i32;",
    "SELECT * FROM unique_u8;",
    "SELECT * FROM unique_u16;",
    "SELECT * FROM unique_u32;",
    "SELECT * FROM unique_u64;",
    "SELECT * FROM unique_u128;",
    "SELECT * FROM unique_u256;",
    "SELECT * FROM unique_i8;",
    "SELECT * FROM unique_i16;",
    "SELECT * FROM unique_i32;",
    "SELECT * FROM unique_i64;",
    "SELECT * FROM unique_i128;",
    "SELECT * FROM unique_i256;",
    "SELECT * FROM unique_bool;",
    "SELECT * FROM unique_string;",
    "SELECT * FROM unique_identity;",
    "SELECT * FROM unique_address;",
    "SELECT * FROM pk_u8;",
    "SELECT * FROM pk_u16;",
    "SELECT * FROM pk_u32;",
    "SELECT * FROM pk_u64;",
    "SELECT * FROM pk_u128;",
    "SELECT * FROM pk_u256;",
    "SELECT * FROM pk_i8;",
    "SELECT * FROM pk_i16;",
    "SELECT * FROM pk_i32;",
    "SELECT * FROM pk_i64;",
    "SELECT * FROM pk_i128;",
    "SELECT * FROM pk_i256;",
    "SELECT * FROM pk_bool;",
    "SELECT * FROM pk_string;",
    "SELECT * FROM pk_identity;",
    "SELECT * FROM pk_address;",
    "SELECT * FROM large_table;",
    "SELECT * FROM table_holds_table;",
];

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
        .on_connect_error(|e| panic!("Connect errored: {e:?}"));
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

fn connect(test_counter: &std::sync::Arc<TestCounter>) -> DbConnection {
    connect_then(test_counter, |_| {})
}

fn subscribe_all_then(ctx: &impl RemoteDbContext, callback: impl FnOnce(&EventContext) + Send + 'static) {
    let remaining_queries = Arc::new(AtomicUsize::new(SUBSCRIBE_ALL.len()));
    let callback = Arc::new(Mutex::new(Some(callback)));
    for query in SUBSCRIBE_ALL {
        let atomic = remaining_queries.clone();
        let callback = callback.clone();

        let on_applied = move |ctx: &EventContext| {
            let count = atomic.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            if count == 1 {
                // Only execute callback when the last subscription completes
                if let Some(cb) = callback.lock().unwrap().take() {
                    cb(ctx);
                }
            }
        };

        ctx.subscription_builder()
            .on_applied(on_applied)
            .on_error(|ctx| panic!("Subscription errored: {:?}", ctx.event))
            .subscribe(query);
    }
}

fn exec_subscribe_and_cancel() {
    let test_counter = TestCounter::new();
    let cb = test_counter.add_test("unsubscribe_then_called");
    connect_then(&test_counter, {
        move |ctx| {
            let handle = ctx
                .subscription_builder()
                .on_applied(move |_ctx: &EventContext| {
                    panic!("Subscription should never be applied");
                })
                .on_error(|ctx| panic!("Subscription errored: {:?}", ctx.event))
                .subscribe("SELECT * FROM one_u8;");
            assert!(!handle.is_active());
            assert!(!handle.is_ended());
            let handle_clone = handle.clone();
            handle
                .unsubscribe_then(Box::new(move |_| {
                    assert!(!handle_clone.is_active());
                    assert!(handle_clone.is_ended());
                    cb(Ok(()));
                }))
                .unwrap();
        }
    });
    test_counter.wait_for_all();
}

fn exec_subscribe_and_unsubscribe() {
    let test_counter = TestCounter::new();
    let cb = test_counter.add_test("unsubscribe_then_called");
    connect_then(&test_counter, {
        move |ctx| {
            let handle_cell: Arc<Mutex<Option<module_bindings::SubscriptionHandle>>> = Arc::new(Mutex::new(None));
            let hc_clone = handle_cell.clone();
            let handle = ctx
                .subscription_builder()
                .on_applied(move |_ctx: &EventContext| {
                    let handle = { hc_clone.lock().unwrap().as_ref().unwrap().clone() };
                    assert!(handle.is_active());
                    assert!(!handle.is_ended());
                    let handle_clone = handle.clone();
                    handle
                        .unsubscribe_then(Box::new(move |_| {
                            assert!(!handle_clone.is_active());
                            assert!(handle_clone.is_ended());
                            cb(Ok(()));
                        }))
                        .unwrap();
                })
                .on_error(|ctx| panic!("Subscription errored: {:?}", ctx.event))
                .subscribe("SELECT * FROM one_u8;");
            handle_cell.lock().unwrap().replace(handle.clone());
            assert!(!handle.is_active());
            assert!(!handle.is_ended());
        }
    });
    test_counter.wait_for_all();
}

fn exec_subscription_error_smoke_test() {
    let test_counter = TestCounter::new();
    let cb = test_counter.add_test("error_callback_is_called");
    connect_then(&test_counter, {
        move |ctx| {
            let handle = ctx
                .subscription_builder()
                .on_applied(move |_ctx: &EventContext| {
                    panic!("Subscription should never be applied");
                })
                .on_error(|_| {
                    cb(Ok(()))
                })
                .subscribe("SELEcCT * FROM one_u8;"); // intentional typo
            assert!(!handle.is_active());
            assert!(!handle.is_ended());
        }
    });
    test_counter.wait_for_all();
}

/// This tests that we can:
/// - Pass primitive types to reducers.
/// - Deserialize primitive types in rows and in reducer arguments.
/// - Observe `on_insert` callbacks with appropriate reducer events.
fn exec_insert_primitive() {
    let test_counter = TestCounter::new();

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                insert_one::<OneU8>(ctx, &test_counter, 0);
                insert_one::<OneU16>(ctx, &test_counter, 0);
                insert_one::<OneU32>(ctx, &test_counter, 0);
                insert_one::<OneU64>(ctx, &test_counter, 0);
                insert_one::<OneU128>(ctx, &test_counter, 0);
                insert_one::<OneU256>(ctx, &test_counter, 0u8.into());

                insert_one::<OneI8>(ctx, &test_counter, 0);
                insert_one::<OneI16>(ctx, &test_counter, 0);
                insert_one::<OneI32>(ctx, &test_counter, 0);
                insert_one::<OneI64>(ctx, &test_counter, 0);
                insert_one::<OneI128>(ctx, &test_counter, 0);
                insert_one::<OneI256>(ctx, &test_counter, 0i8.into());

                insert_one::<OneBool>(ctx, &test_counter, false);

                insert_one::<OneF32>(ctx, &test_counter, 0.0);
                insert_one::<OneF64>(ctx, &test_counter, 0.0);

                insert_one::<OneString>(ctx, &test_counter, "".to_string());

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();
}

/// This tests that we can observe `on_delete` callbacks.
fn exec_delete_primitive() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                insert_then_delete_one::<UniqueU8>(ctx, &test_counter, 0, 0xbeef);
                insert_then_delete_one::<UniqueU16>(ctx, &test_counter, 0, 0xbeef);
                insert_then_delete_one::<UniqueU32>(ctx, &test_counter, 0, 0xbeef);
                insert_then_delete_one::<UniqueU64>(ctx, &test_counter, 0, 0xbeef);
                insert_then_delete_one::<UniqueU128>(ctx, &test_counter, 0, 0xbeef);
                insert_then_delete_one::<UniqueU256>(ctx, &test_counter, 0u8.into(), 0xbeef);

                insert_then_delete_one::<UniqueI8>(ctx, &test_counter, 0, 0xbeef);
                insert_then_delete_one::<UniqueI16>(ctx, &test_counter, 0, 0xbeef);
                insert_then_delete_one::<UniqueI32>(ctx, &test_counter, 0, 0xbeef);
                insert_then_delete_one::<UniqueI64>(ctx, &test_counter, 0, 0xbeef);
                insert_then_delete_one::<UniqueI128>(ctx, &test_counter, 0, 0xbeef);
                insert_then_delete_one::<UniqueI256>(ctx, &test_counter, 0i8.into(), 0xbeef);

                insert_then_delete_one::<UniqueBool>(ctx, &test_counter, false, 0xbeef);

                insert_then_delete_one::<UniqueString>(ctx, &test_counter, "".to_string(), 0xbeef);

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();

    assert_all_tables_empty(&connection).unwrap();
}

/// This tests that we can distinguish between `on_update` and `on_delete` callbacks for tables with primary keys.
fn exec_update_primitive() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                insert_update_delete_one::<PkU8>(ctx, &test_counter, 0, 0xbeef, 0xbabe);
                insert_update_delete_one::<PkU16>(ctx, &test_counter, 0, 0xbeef, 0xbabe);
                insert_update_delete_one::<PkU32>(ctx, &test_counter, 0, 0xbeef, 0xbabe);
                insert_update_delete_one::<PkU64>(ctx, &test_counter, 0, 0xbeef, 0xbabe);
                insert_update_delete_one::<PkU128>(ctx, &test_counter, 0, 0xbeef, 0xbabe);
                insert_update_delete_one::<PkU256>(ctx, &test_counter, 0u8.into(), 0xbeef, 0xbabe);

                insert_update_delete_one::<PkI8>(ctx, &test_counter, 0, 0xbeef, 0xbabe);
                insert_update_delete_one::<PkI16>(ctx, &test_counter, 0, 0xbeef, 0xbabe);
                insert_update_delete_one::<PkI32>(ctx, &test_counter, 0, 0xbeef, 0xbabe);
                insert_update_delete_one::<PkI64>(ctx, &test_counter, 0, 0xbeef, 0xbabe);
                insert_update_delete_one::<PkI128>(ctx, &test_counter, 0, 0xbeef, 0xbabe);
                insert_update_delete_one::<PkI256>(ctx, &test_counter, 0i8.into(), 0xbeef, 0xbabe);

                insert_update_delete_one::<PkBool>(ctx, &test_counter, false, 0xbeef, 0xbabe);

                insert_update_delete_one::<PkString>(ctx, &test_counter, "".to_string(), 0xbeef, 0xbabe);

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();

    assert_all_tables_empty(&connection).unwrap();
}

/// This tests that we can serialize and deserialize `Identity` in various contexts.
fn exec_insert_identity() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                insert_one::<OneIdentity>(
                    ctx,
                    &test_counter,
                    Identity::__dummy(), // connection.identity().unwrap()
                );

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            })
        }
    });

    test_counter.wait_for_all();
}

/// This tests that we can retrieve and use the caller's `Identity` from the reducer context.
fn exec_insert_caller_identity() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                on_insert_one::<OneIdentity>(ctx, &test_counter, ctx.identity(), |event| {
                    matches!(event, Reducer::InsertCallerOneIdentity)
                });
                ctx.reducers.insert_caller_one_identity().unwrap();

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();
}

/// This test doesn't add much alongside `exec_insert_identity` and `exec_delete_primitive`,
/// but it's here for symmetry.
fn exec_delete_identity() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                insert_then_delete_one::<UniqueIdentity>(ctx, &test_counter, ctx.identity(), 0xbeef);

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();

    assert_all_tables_empty(&connection).unwrap();
}

/// This tests that we can distinguish between `on_delete` and `on_update` events
/// for tables with `Identity` primary keys.
fn exec_update_identity() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                insert_update_delete_one::<PkIdentity>(ctx, &test_counter, ctx.identity(), 0xbeef, 0xbabe);

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();

    assert_all_tables_empty(&connection).unwrap();
}

/// This tests that we can serialize and deserialize `Address` in various contexts.
fn exec_insert_address() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                insert_one::<OneAddress>(ctx, &test_counter, ctx.address());

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();
}

/// This tests that we can serialize and deserialize `Address` in various contexts.
fn exec_insert_caller_address() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                on_insert_one::<OneAddress>(ctx, &test_counter, ctx.address(), |event| {
                    matches!(event, Reducer::InsertCallerOneAddress)
                });
                ctx.reducers.insert_caller_one_address().unwrap();
                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();
}

/// This test doesn't add much alongside `exec_insert_address` and `exec_delete_primitive`,
/// but it's here for symmetry.
fn exec_delete_address() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                insert_then_delete_one::<UniqueAddress>(ctx, &test_counter, ctx.address(), 0xbeef);

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();

    assert_all_tables_empty(&connection).unwrap();
}

/// This tests that we can distinguish between `on_delete` and `on_update` events
/// for tables with `Address` primary keys.
fn exec_update_address() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    subscribe_all_then(&connection, {
        let test_counter = test_counter.clone();
        move |ctx| {
            insert_update_delete_one::<PkAddress>(
                ctx,
                &test_counter,
                Address::default(), // connection.address().unwrap(),
                0xbeef,
                0xbabe,
            );

            sub_applied_nothing_result(assert_all_tables_empty(ctx));
        }
    });

    test_counter.wait_for_all();

    assert_all_tables_empty(&connection).unwrap();
}

/// This tests that we can observe reducer callbacks for successful reducer runs.
fn exec_on_reducer() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    let mut reducer_result = Some(test_counter.add_test("reducer-callback"));

    let value = 128;

    connection.reducers.on_insert_one_u_8(move |ctx, arg| {
        let run_checks = || {
            if *arg != value {
                anyhow::bail!("Unexpected reducer argument. Expected {} but found {}", value, *arg);
            }
            let Event::Reducer(reducer_event) = &ctx.event else {
                anyhow::bail!("Expected Reducer event but found {:?}", ctx.event);
            };
            if reducer_event.caller_identity != ctx.identity() {
                anyhow::bail!(
                    "Expected caller_identity to be my own identity {:?}, but found {:?}",
                    ctx.identity(),
                    reducer_event.caller_identity,
                );
            }
            if reducer_event.caller_address != Some(ctx.address()) {
                anyhow::bail!(
                    "Expected caller_address to be my own address {:?}, but found {:?}",
                    ctx.address(),
                    reducer_event.caller_address,
                )
            }
            if !matches!(reducer_event.status, Status::Committed) {
                anyhow::bail!(
                    "Unexpected status. Expected Committed but found {:?}",
                    reducer_event.status
                );
            }
            let expected_reducer = Reducer::InsertOneU8 { n: value };
            if reducer_event.reducer != expected_reducer {
                anyhow::bail!(
                    "Unexpected Reducer in ReducerEvent: expected {expected_reducer:?} but found {:?}",
                    reducer_event.reducer
                );
            }

            if ctx.db.one_u_8().count() != 1 {
                anyhow::bail!("Expected 1 row in table OneU8, but found {}", ctx.db.one_u_8().count());
            }
            let row = ctx.db.one_u_8().iter().next().unwrap();
            if row.n != value {
                anyhow::bail!("Unexpected row value. Expected {} but found {:?}", value, row);
            }
            Ok(())
        };

        (reducer_result.take().unwrap())(run_checks());
    });

    subscribe_all_then(&connection, move |ctx| {
        sub_applied_nothing_result(assert_all_tables_empty(ctx));
        ctx.reducers.insert_one_u_8(value).unwrap();
    });

    test_counter.wait_for_all();
}

/// This tests that we can observe reducer callbacks for failed reducers.
fn exec_fail_reducer() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");
    let mut reducer_success_result = Some(test_counter.add_test("reducer-callback-success"));
    let mut reducer_fail_result = Some(test_counter.add_test("reducer-callback-fail"));

    let connection = connect(&test_counter);

    let key = 128;
    let initial_data = 0xbeef;
    let fail_data = 0xbabe;

    // We'll call the reducer `insert_pk_u8` twice with the same key,
    // listening for a success the first time and a failure the second.
    // We'll set this to false after our first time through.
    let mut should_succeed = true;

    connection.reducers.on_insert_pk_u_8(move |ctx, arg_key, arg_val| {
        if should_succeed {
            let run_checks = || {
                if *arg_key != key {
                    anyhow::bail!("Unexpected reducer argument. Expected {} but found {}", key, *arg_key);
                }
                if *arg_val != initial_data {
                    anyhow::bail!(
                        "Unexpected reducer argument. Expected {} but found {}",
                        initial_data,
                        *arg_val,
                    );
                }
                let Event::Reducer(reducer_event) = &ctx.event else {
                    anyhow::bail!("Expected Reducer event but found {:?}", ctx.event);
                };
                if reducer_event.caller_identity != ctx.identity() {
                    anyhow::bail!(
                        "Expected caller_identity to be my own identity {:?}, but found {:?}",
                        ctx.identity(),
                        reducer_event.caller_identity,
                    );
                }
                if reducer_event.caller_address != Some(ctx.address()) {
                    anyhow::bail!(
                        "Expected caller_address to be my own address {:?}, but found {:?}",
                        ctx.address(),
                        reducer_event.caller_address,
                    )
                }
                if !matches!(reducer_event.status, Status::Committed) {
                    anyhow::bail!(
                        "Unexpected status. Expected Committed but found {:?}",
                        reducer_event.status
                    );
                }
                let expected_reducer = Reducer::InsertPkU8 {
                    n: key,
                    data: initial_data,
                };
                if reducer_event.reducer != expected_reducer {
                    anyhow::bail!(
                        "Unexpected Reducer in ReducerEvent: expected {expected_reducer:?} but found {:?}",
                        reducer_event.reducer
                    );
                }

                if ctx.db.pk_u_8().count() != 1 {
                    anyhow::bail!("Expected 1 row in table PkU8, but found {}", ctx.db.pk_u_8().count());
                }
                let row = ctx.db.pk_u_8().iter().next().unwrap();
                if row.n != key || row.data != initial_data {
                    anyhow::bail!(
                        "Unexpected row value. Expected ({}, {}) but found {:?}",
                        key,
                        initial_data,
                        row
                    );
                }
                Ok(())
            };

            (reducer_success_result.take().unwrap())(run_checks());

            should_succeed = false;

            ctx.reducers.insert_pk_u_8(key, fail_data).unwrap();
        } else {
            let run_checks = || {
                if *arg_key != key {
                    anyhow::bail!("Unexpected reducer argument. Expected {} but found {}", key, *arg_key);
                }
                if *arg_val != fail_data {
                    anyhow::bail!(
                        "Unexpected reducer argument. Expected {} but found {}",
                        initial_data,
                        *arg_val
                    );
                }
                let Event::Reducer(reducer_event) = &ctx.event else {
                    anyhow::bail!("Expected Reducer event but found {:?}", ctx.event);
                };
                if reducer_event.caller_identity != ctx.identity() {
                    anyhow::bail!(
                        "Expected caller_identity to be my own identity {:?}, but found {:?}",
                        ctx.identity(),
                        reducer_event.caller_identity,
                    );
                }
                if reducer_event.caller_address != Some(ctx.address()) {
                    anyhow::bail!(
                        "Expected caller_address to be my own address {:?}, but found {:?}",
                        ctx.address(),
                        reducer_event.caller_address,
                    )
                }
                if !matches!(reducer_event.status, Status::Failed(_)) {
                    anyhow::bail!(
                        "Unexpected status. Expected Committed but found {:?}",
                        reducer_event.status
                    );
                }
                let expected_reducer = Reducer::InsertPkU8 {
                    n: key,
                    data: fail_data,
                };
                if reducer_event.reducer != expected_reducer {
                    anyhow::bail!(
                        "Unexpected Reducer in ReducerEvent: expected {expected_reducer:?} but found {:?}",
                        reducer_event.reducer
                    );
                }

                if ctx.db.pk_u_8().count() != 1 {
                    anyhow::bail!("Expected 1 row in table PkU8, but found {}", ctx.db.pk_u_8().count());
                }
                let row = ctx.db.pk_u_8().iter().next().unwrap();
                if row.n != key || row.data != initial_data {
                    anyhow::bail!(
                        "Unexpected row value. Expected ({}, {}) but found {:?}",
                        key,
                        initial_data,
                        row
                    );
                }
                Ok(())
            };

            (reducer_fail_result.take().unwrap())(run_checks());
        }
    });

    subscribe_all_then(&connection, move |ctx| {
        ctx.reducers.insert_pk_u_8(key, initial_data).unwrap();

        sub_applied_nothing_result(assert_all_tables_empty(ctx));
    });

    test_counter.wait_for_all();
}

/// This tests that we can serialize and deserialize `Vec<?>` in various contexts.
fn exec_insert_vec() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    subscribe_all_then(&connection, {
        let test_counter = test_counter.clone();
        move |ctx| {
            insert_one::<VecU8>(ctx, &test_counter, vec![0, 1]);
            insert_one::<VecU16>(ctx, &test_counter, vec![0, 1]);
            insert_one::<VecU32>(ctx, &test_counter, vec![0, 1]);
            insert_one::<VecU64>(ctx, &test_counter, vec![0, 1]);
            insert_one::<VecU128>(ctx, &test_counter, vec![0, 1]);
            insert_one::<VecU256>(ctx, &test_counter, [0u8, 1].map(Into::into).into());

            insert_one::<VecI8>(ctx, &test_counter, vec![0, 1]);
            insert_one::<VecI16>(ctx, &test_counter, vec![0, 1]);
            insert_one::<VecI32>(ctx, &test_counter, vec![0, 1]);
            insert_one::<VecI64>(ctx, &test_counter, vec![0, 1]);
            insert_one::<VecI128>(ctx, &test_counter, vec![0, 1]);
            insert_one::<VecI256>(ctx, &test_counter, [0i8, 1].map(Into::into).into());

            insert_one::<VecBool>(ctx, &test_counter, vec![false, true]);

            insert_one::<VecF32>(ctx, &test_counter, vec![0.0, 1.0]);
            insert_one::<VecF64>(ctx, &test_counter, vec![0.0, 1.0]);

            insert_one::<VecString>(ctx, &test_counter, vec!["zero".to_string(), "one".to_string()]);

            insert_one::<VecIdentity>(ctx, &test_counter, vec![ctx.identity()]);
            insert_one::<VecAddress>(ctx, &test_counter, vec![ctx.address()]);

            sub_applied_nothing_result(assert_all_tables_empty(ctx));
        }
    });

    test_counter.wait_for_all();
}

fn every_primitive_struct() -> EveryPrimitiveStruct {
    // Note: the numbers are intentionally chosen to have asymmetrical binary
    // representations with all bytes being non-zero.
    // This allows to catch endianness issues in BSATN implementations.
    EveryPrimitiveStruct {
        a: 0x01_u8,
        b: 0x0102_u16,
        c: 0x0102_0304_u32,
        d: 0x0102_0304_0506_0708_u64,
        e: 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10_u128,
        f: u256::from_words(
            0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10_u128,
            0x1112_1314_1516_1718_191a_1b1c_1d1e_1f20_u128,
        ),
        g: -0x01_i8,
        h: -0x0102_i16,
        i: -0x0102_0304_i32,
        j: -0x0102_0304_0506_0708_i64,
        k: -0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10_i128,
        l: -i256::from_words(
            0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10_i128,
            0x1112_1314_1516_1718_191a_1b1c_1d1e_1f20_i128,
        ),
        m: false,
        n: 1.0,
        o: -1.0,
        p: "string".to_string(),
        q: Identity::__dummy(),
        r: Address::default(),
    }
}

fn every_vec_struct() -> EveryVecStruct {
    EveryVecStruct {
        a: vec![],
        b: vec![1],
        c: vec![2, 2],
        d: vec![3, 3, 3],
        e: vec![4, 4, 4, 4],
        f: [5u8, 5, 5, 5, 5].map(Into::into).into(),
        g: vec![-1],
        h: vec![-2, -2],
        i: vec![-3, -3, -3],
        j: vec![-4, -4, -4, -4],
        k: vec![-5, -5, -5, -5, -5],
        l: [-6i8, -6, -6, -6, -6, -6].map(Into::into).into(),
        m: vec![false, true, true, false],
        n: vec![0.0, -1.0, 1.0, -2.0, 2.0],
        o: vec![0.0, -0.5, 0.5, -1.5, 1.5],
        p: ["vec", "of", "strings"].into_iter().map(str::to_string).collect(),
        q: vec![Identity::__dummy()],
        r: vec![Address::default()],
    }
}

fn large_table() -> LargeTable {
    LargeTable {
        a: 0,
        b: 1,
        c: 2,
        d: 3,
        e: 4,
        f: 5u8.into(),
        g: 0,
        h: -1,
        i: -2,
        j: -3,
        k: -4,
        l: (-5i8).into(),
        m: false,
        n: 0.0,
        o: 1.0,
        p: "string".to_string(),
        q: SimpleEnum::Zero,
        r: EnumWithPayload::Bool(false),
        s: UnitStruct {},
        t: ByteStruct { b: 0b10101010 },
        u: every_primitive_struct(),
        v: every_vec_struct(),
    }
}

/// This tests that we can serialize and deserialize `Option`s of various payload types which are `Some`.
///
/// Note that this must be a separate test from [`exec_insert_option_none`],
/// as [`insert_one`] cannot handle running multiple tests for the same type in parallel.
fn exec_insert_option_some() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    subscribe_all_then(&connection, {
        let test_counter = test_counter.clone();
        move |ctx| {
            insert_one::<OptionI32>(ctx, &test_counter, Some(0));
            insert_one::<OptionString>(ctx, &test_counter, Some("string".to_string()));
            insert_one::<OptionIdentity>(ctx, &test_counter, Some(ctx.identity()));
            insert_one::<OptionSimpleEnum>(ctx, &test_counter, Some(SimpleEnum::Zero));
            insert_one::<OptionEveryPrimitiveStruct>(ctx, &test_counter, Some(every_primitive_struct()));
            insert_one::<OptionVecOptionI32>(ctx, &test_counter, Some(vec![Some(0), None]));

            sub_applied_nothing_result(assert_all_tables_empty(ctx));
        }
    });

    test_counter.wait_for_all();
}

/// This tests that we can serialize and deserialize `Option`s of various payload types which are `None`.
///
/// Note that this must be a separate test from [`exec_insert_option_some`],
/// as [`insert_one`] cannot handle running multiple tests for the same type in parallel.
fn exec_insert_option_none() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    subscribe_all_then(&connection, {
        let test_counter = test_counter.clone();
        move |ctx| {
            insert_one::<OptionI32>(ctx, &test_counter, None);
            insert_one::<OptionString>(ctx, &test_counter, None);
            insert_one::<OptionIdentity>(ctx, &test_counter, None);
            insert_one::<OptionSimpleEnum>(ctx, &test_counter, None);
            insert_one::<OptionEveryPrimitiveStruct>(ctx, &test_counter, None);
            insert_one::<OptionVecOptionI32>(ctx, &test_counter, None);

            sub_applied_nothing_result(assert_all_tables_empty(ctx));
        }
    });

    test_counter.wait_for_all();
}

/// This tests that we can serialize and deserialize structs in various contexts.
fn exec_insert_struct() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    subscribe_all_then(&connection, {
        let test_counter = test_counter.clone();
        move |ctx| {
            insert_one::<OneUnitStruct>(ctx, &test_counter, UnitStruct {});
            insert_one::<OneByteStruct>(ctx, &test_counter, ByteStruct { b: 0 });
            insert_one::<OneEveryPrimitiveStruct>(ctx, &test_counter, every_primitive_struct());
            insert_one::<OneEveryVecStruct>(ctx, &test_counter, every_vec_struct());

            insert_one::<VecUnitStruct>(ctx, &test_counter, vec![UnitStruct {}]);
            insert_one::<VecByteStruct>(ctx, &test_counter, vec![ByteStruct { b: 0 }]);
            insert_one::<VecEveryPrimitiveStruct>(ctx, &test_counter, vec![every_primitive_struct()]);
            insert_one::<VecEveryVecStruct>(ctx, &test_counter, vec![every_vec_struct()]);

            sub_applied_nothing_result(assert_all_tables_empty(ctx));
        }
    });

    test_counter.wait_for_all();
}

/// This tests that we can serialize and deserialize C-style enums in various contexts.
fn exec_insert_simple_enum() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    subscribe_all_then(&connection, {
        let test_counter = test_counter.clone();
        move |ctx| {
            insert_one::<OneSimpleEnum>(ctx, &test_counter, SimpleEnum::One);
            insert_one::<VecSimpleEnum>(
                ctx,
                &test_counter,
                vec![SimpleEnum::Zero, SimpleEnum::One, SimpleEnum::Two],
            );

            sub_applied_nothing_result(assert_all_tables_empty(ctx));
        }
    });

    test_counter.wait_for_all();
}

/// This tests that we can serialize and deserialize sum types in various contexts.
fn exec_insert_enum_with_payload() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    subscribe_all_then(&connection, {
        let test_counter = test_counter.clone();
        move |ctx| {
            insert_one::<OneEnumWithPayload>(ctx, &test_counter, EnumWithPayload::U8(0));
            insert_one::<VecEnumWithPayload>(
                ctx,
                &test_counter,
                vec![
                    EnumWithPayload::U8(0),
                    EnumWithPayload::U16(1),
                    EnumWithPayload::U32(2),
                    EnumWithPayload::U64(3),
                    EnumWithPayload::U128(4),
                    EnumWithPayload::U256(5u8.into()),
                    EnumWithPayload::I8(0),
                    EnumWithPayload::I16(-1),
                    EnumWithPayload::I32(-2),
                    EnumWithPayload::I64(-3),
                    EnumWithPayload::I128(-4),
                    EnumWithPayload::I128((-5i8).into()),
                    EnumWithPayload::Bool(true),
                    EnumWithPayload::F32(0.0),
                    EnumWithPayload::F64(100.0),
                    EnumWithPayload::Str("enum holds string".to_string()),
                    EnumWithPayload::Identity(ctx.identity()),
                    EnumWithPayload::Bytes(vec![0xde, 0xad, 0xbe, 0xef]),
                    EnumWithPayload::Strings(
                        ["enum", "of", "vec", "of", "strings"]
                            .into_iter()
                            .map(str::to_string)
                            .collect(),
                    ),
                    EnumWithPayload::SimpleEnums(vec![SimpleEnum::Zero, SimpleEnum::One, SimpleEnum::Two]),
                ],
            );

            sub_applied_nothing_result(assert_all_tables_empty(ctx));
        }
    });

    test_counter.wait_for_all();
}

/// This tests that the test machinery itself is functional and can detect failures.
fn exec_should_fail() {
    let test_counter = TestCounter::new();
    let fail = test_counter.add_test("should-fail");
    fail(Err(anyhow::anyhow!("This is an intentional failure")));
    test_counter.wait_for_all();
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

/// This test invokes a reducer with many arguments of many types,
/// and observes a callback for an inserted table with many columns of many types.
fn exec_insert_long_table() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    subscribe_all_then(&connection, {
        let test_counter = test_counter.clone();
        let mut large_table_result = Some(test_counter.add_test("insert-large-table"));
        move |ctx| {
            ctx.db.large_table().on_insert(move |ctx, row| {
                if large_table_result.is_some() {
                    let run_tests = || {
                        assert_eq_or_bail!(large_table(), *row);
                        if !matches!(
                            ctx.event,
                            Event::Reducer(ReducerEvent {
                                reducer: Reducer::InsertLargeTable { .. },
                                ..
                            })
                        ) {
                            anyhow::bail!("Unexpected event: expeced InsertLargeTable but found {:?}", ctx.event,);
                        }
                        Ok(())
                    };
                    (large_table_result.take().unwrap())(run_tests());
                }
            });
            let large_table = large_table();
            ctx.reducers
                .insert_large_table(
                    large_table.a,
                    large_table.b,
                    large_table.c,
                    large_table.d,
                    large_table.e,
                    large_table.f,
                    large_table.g,
                    large_table.h,
                    large_table.i,
                    large_table.j,
                    large_table.k,
                    large_table.l,
                    large_table.m,
                    large_table.n,
                    large_table.o,
                    large_table.p,
                    large_table.q,
                    large_table.r,
                    large_table.s,
                    large_table.t,
                    large_table.u,
                    large_table.v,
                )
                .unwrap();

            sub_applied_nothing_result(assert_all_tables_empty(ctx))
        }
    });

    test_counter.wait_for_all();
}

fn exec_insert_primitives_as_strings() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    subscribe_all_then(&connection, {
        let test_counter = test_counter.clone();
        let mut result = Some(test_counter.add_test("insert-primitives-as-strings"));
        move |ctx| {
            let s = every_primitive_struct();

            let strings = vec![
                s.a.to_string(),
                s.b.to_string(),
                s.c.to_string(),
                s.d.to_string(),
                s.e.to_string(),
                s.f.to_string(),
                s.g.to_string(),
                s.h.to_string(),
                s.i.to_string(),
                s.j.to_string(),
                s.k.to_string(),
                s.l.to_string(),
                s.m.to_string(),
                s.n.to_string(),
                s.o.to_string(),
                s.p.to_string(),
                s.q.to_string(),
                s.r.to_string(),
            ];

            ctx.db.vec_string().on_insert(move |ctx, row| {
                if result.is_some() {
                    let run_tests = || {
                        assert_eq_or_bail!(strings, row.s);
                        if !matches!(
                            ctx.event,
                            Event::Reducer(ReducerEvent {
                                status: Status::Committed,
                                reducer: Reducer::InsertPrimitivesAsStrings { .. },
                                ..
                            })
                        ) {
                            anyhow::bail!(
                                "Unexpected Event: expeced reducer InsertPrimitivesAsStrings but found {:?}",
                                ctx.event,
                            );
                        }
                        Ok(())
                    };
                    (result.take().unwrap())(run_tests());
                }
            });
            ctx.reducers.insert_primitives_as_strings(s).unwrap();

            sub_applied_nothing_result(assert_all_tables_empty(ctx))
        }
    });

    test_counter.wait_for_all();
}

// /// This tests the behavior of re-subscribing
// /// by observing `on_delete` callbacks of newly-unsubscribed rows
// /// and `on_insert` callbacks of newly-subscribed rows.
// fn exec_resubscribe() {
//     let test_counter = TestCounter::new();
//     let name = db_name_or_panic();

//     // Boring stuff first: connect and subscribe to everything.
//     let connect_result = test_counter.add_test("connect");
//     let subscribe_result = test_counter.add_test("initial-subscribe");
//     let sub_applied_result = test_counter.add_test("initial-subscription-nothing");

//     once_on_subscription_applied(move || {
//         sub_applied_result(assert_all_tables_empty());
//     });

//     once_on_connect(|_, _| {
//         subscribe_result(subscribe(SUBSCRIBE_ALL));
//     });

//     connect_result(connect(LOCALHOST, &name, None));

//     // Wait for all previous checks before continuing.
//     test_counter.wait_for_all();

//     // Insert 256 rows of `OneU8`.
//     // At this point, we should be subscribed to all of them.
//     let test_counter = TestCounter::new();
//     let mut insert_u8s = (0..=255)
//         .map(|n| Some(test_counter.add_test(format!("insert-{}", n))))
//         .collect::<Vec<_>>();
//     let on_insert_u8 = OneU8::on_insert(move |row, _| {
//         let n = row.n;
//         (insert_u8s[n as usize].take().unwrap())(Ok(()));
//     });
//     for n in 0..=255 {
//         insert_one_u_8(n as u8);
//     }
//     // Wait for all previous checks before continuing,
//     test_counter.wait_for_all();
//     // and remove the callback now that we're done with it.
//     OneU8::remove_on_insert(on_insert_u8);
//     // Re-subscribe with a query that excludes the lower half of the `OneU8` rows,
//     // and observe `on_delete` callbacks for those rows.
//     let test_counter = TestCounter::new();
//     let mut delete_u8s = (0..128)
//         .map(|n| Some(test_counter.add_test(format!("unsubscribe-{}-delete", n))))
//         .collect::<Vec<_>>();
//     let on_delete_verify = OneU8::on_delete(move |row, _| {
//         let n = row.n;
//         // This indexing will panic if n > 127.
//         (delete_u8s[n as usize].take().unwrap())(Ok(()));
//     });
//     // There should be no newly-subscribed rows, so we'll panic if we get an on-insert event.
//     let on_insert_panic = OneU8::on_insert(|row, _| {
//         panic!("Unexpected insert during re-subscribe for {:?}", row);
//     });
//     let subscribe_less_result = test_counter.add_test("resubscribe-fewer-matches");
//     once_on_subscription_applied(move || {
//         let run_checks = || {
//             assert_eq_or_bail!(128, OneU8::count());
//             if let Some(row) = OneU8::iter().find(|row| row.n < 128) {
//                 anyhow::bail!("After subscribing to OneU8 WHERE n > 127, found row with n < {}", row.n);
//             }
//             Ok(())
//         };
//         subscribe_less_result(run_checks());
//     });
//     let subscribe_result = test_counter.add_test("resubscribe");
//     subscribe_result(subscribe(&["SELECT * FROM OneU8 WHERE n > 127"]));
//     // Wait before continuing, and remove callbacks.
//     test_counter.wait_for_all();
//     OneU8::remove_on_delete(on_delete_verify);
//     OneU8::remove_on_insert(on_insert_panic);

//     // Re-subscribe with a query that includes all of the `OneU8` rows again,
//     // and observe `on_insert` callbacks for the lower half.
//     let test_counter = TestCounter::new();
//     let mut insert_u8s = (0..128)
//         .map(|n| Some(test_counter.add_test(format!("resubscribe-{}-insert", n))))
//         .collect::<Vec<_>>();
//     OneU8::on_insert(move |row, _| {
//         let n = row.n;
//         // This indexing will panic if n > 127.
//         (insert_u8s[n as usize].take().unwrap())(Ok(()));
//     });
//     // There should be no newly-unsubscribed rows, so we'll panic if we get an on-delete event.
//     OneU8::on_delete(|row, _| {
//         panic!("Unexpected delete during re-subscribe for {:?}", row);
//     });
//     let subscribe_more_result = test_counter.add_test("resubscribe-more-matches");
//     once_on_subscription_applied(move || {
//         let run_checks = || {
//             assert_eq_or_bail!(256, OneU8::count());
//             Ok(())
//         };
//         subscribe_more_result(run_checks());
//     });
//     let subscribe_result = test_counter.add_test("resubscribe-again");
//     subscribe_result(subscribe(&["SELECT * FROM OneU8"]));
//     test_counter.wait_for_all();
// }

fn creds_store() -> credentials::File {
    credentials::File::new("rust-sdk-test")
}

/// Part of the `reauth` test, this connects to Spacetime to get new credentials,
/// and saves them to a file.
fn exec_reauth_part_1() {
    let test_counter = TestCounter::new();

    let name = db_name_or_panic();

    let save_result = test_counter.add_test("save-credentials");

    DbConnection::builder()
        .on_connect(|_, identity, token| {
            save_result(creds_store().save(identity, token));
        })
        .on_connect_error(|e| panic!("Connect failed: {e:?}"))
        .with_module_name(name)
        .with_uri(LOCALHOST)
        .build()
        .unwrap()
        .run_threaded();

    test_counter.wait_for_all();
}

/// Part of the `reauth` test, this loads credentials from a file,
/// and passes them to `connect`.
///
/// Must run after `exec_reauth_part_1`.
fn exec_reauth_part_2() {
    let test_counter = TestCounter::new();

    let name = db_name_or_panic();

    let creds_match_result = test_counter.add_test("creds-match");

    let (identity, token) = creds_store().load().unwrap().unwrap();

    DbConnection::builder()
        .on_connect({
            let token = token.clone();
            move |_, recv_identity, recv_token| {
                let run_checks = || {
                    assert_eq_or_bail!(identity, recv_identity);
                    assert_eq_or_bail!(token, recv_token);
                    Ok(())
                };
                creds_match_result(run_checks());
            }
        })
        .on_connect_error(|e| panic!("Connect failed: {e:?}"))
        .with_module_name(name)
        .with_credentials(Some((identity, token)))
        .with_uri(LOCALHOST)
        .build()
        .unwrap()
        .run_threaded();

    test_counter.wait_for_all();
}

fn exec_reconnect_same_address() {
    let initial_test_counter = TestCounter::new();
    let initial_connect_result = initial_test_counter.add_test("connect");

    let disconnect_test_counter = TestCounter::new();
    let disconnect_result = disconnect_test_counter.add_test("disconnect");

    let initial_connection = DbConnection::builder()
        .with_module_name(db_name_or_panic())
        .with_uri(LOCALHOST)
        .on_connect_error(|e| panic!("on_connect_error: {e:?}"))
        .on_connect(move |_, _, _| {
            initial_connect_result(Ok(()));
        })
        .on_disconnect(|_, err| {
            if let Some(err) = err {
                disconnect_result(Err(anyhow::anyhow!("{err:?}")));
            } else {
                disconnect_result(Ok(()))
            }
        })
        .build()
        .unwrap();

    initial_connection.run_threaded();

    initial_test_counter.wait_for_all();

    let my_address = initial_connection.address();

    initial_connection.disconnect().unwrap();

    disconnect_test_counter.wait_for_all();

    let reconnect_test_counter = TestCounter::new();
    let reconnect_result = reconnect_test_counter.add_test("reconnect");
    let addr_after_reconnect_result = reconnect_test_counter.add_test("addr_after_reconnect");

    let re_connection = DbConnection::builder()
        .with_module_name(db_name_or_panic())
        .with_uri(LOCALHOST)
        .on_connect_error(|e| panic!("on_connect_error: {e:?}"))
        .on_connect(move |ctx, _, _| {
            reconnect_result(Ok(()));
            let run_checks = || {
                anyhow::ensure!(ctx.address() == my_address);
                Ok(())
            };
            addr_after_reconnect_result(run_checks());
        })
        .build()
        .unwrap();
    re_connection.run_threaded();

    reconnect_test_counter.wait_for_all();
}

fn exec_caller_always_notified() {
    let test_counter = TestCounter::new();

    let mut no_op_result = Some(test_counter.add_test("notified_of_no_op_reducer"));

    let connection = connect(&test_counter);

    connection.reducers.on_no_op_succeeds(move |ctx| {
        (no_op_result.take().unwrap())(match ctx.event {
            Event::Reducer(ReducerEvent {
                status: Status::Committed,
                reducer: Reducer::NoOpSucceeds,
                ..
            }) => Ok(()),
            _ => Err(anyhow::anyhow!(
                "Unexpected event from no_op_succeeds reducer: {:?}",
                ctx.event,
            )),
        });
    });

    connection.reducers.no_op_succeeds().unwrap();

    test_counter.wait_for_all();
}

/// Duplicates the test `insert_primitive`, but using the `SELECT * FROM *` sugar
/// rather than an explicit query set.
fn exec_subscribe_all_select_star() {
    let test_counter = TestCounter::new();

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    connection
        .subscription_builder()
        .on_applied({
            let test_counter = test_counter.clone();
            move |ctx| {
                insert_one::<OneU8>(ctx, &test_counter, 0);
                insert_one::<OneU16>(ctx, &test_counter, 0);
                insert_one::<OneU32>(ctx, &test_counter, 0);
                insert_one::<OneU64>(ctx, &test_counter, 0);
                insert_one::<OneU128>(ctx, &test_counter, 0);
                insert_one::<OneU256>(ctx, &test_counter, 0u8.into());

                insert_one::<OneI8>(ctx, &test_counter, 0);
                insert_one::<OneI16>(ctx, &test_counter, 0);
                insert_one::<OneI32>(ctx, &test_counter, 0);
                insert_one::<OneI64>(ctx, &test_counter, 0);
                insert_one::<OneI128>(ctx, &test_counter, 0);
                insert_one::<OneI256>(ctx, &test_counter, 0i8.into());

                insert_one::<OneBool>(ctx, &test_counter, false);

                insert_one::<OneF32>(ctx, &test_counter, 0.0);
                insert_one::<OneF64>(ctx, &test_counter, 0.0);

                insert_one::<OneString>(ctx, &test_counter, "".to_string());

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            }
        })
        .on_error(|_| panic!("Subscription error"))
        .subscribe_to_all_tables();

    test_counter.wait_for_all();
}

fn exec_caller_alice_receives_reducer_callback_but_not_bob() {
    fn check_val<T: Display + Eq>(val: T, eq: T) -> anyhow::Result<()> {
        (val == eq)
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("wrong value received: `{val}`, expected: `{eq}`"))
    }

    let counter = TestCounter::new();
    let pre_ins_counter = TestCounter::new();

    // Have two actors, Alice (0) and Bob (1), connect to the module.
    // For each actor, subscribe to the `OneU8` table.
    // The choice of table is a fairly random one: just one of the simpler tables.
    let conns = ["alice", "bob"].map(|who| {
        let conn = connect_with_then(&pre_ins_counter, who, |b| b.with_light_mode(true), |_| {});
        let sub_applied = pre_ins_counter.add_test(format!("sub_applied_{who}"));

        let counter2 = counter.clone();
        subscribe_all_then(&conn, move |ctx| {
            sub_applied(Ok(()));

            // Test that we are notified when a row is inserted.
            let db = ctx.db();
            let mut one_u8_inserted = Some(counter2.add_test(format!("one_u8_inserted_{who}")));
            db.one_u_8().on_insert(move |_, row| {
                (one_u8_inserted.take().unwrap())(check_val(row.n, 42));
            });
            let mut one_u16_inserted = Some(counter2.add_test(format!("one_u16_inserted_{who}")));
            db.one_u_16().on_insert(move |event, row| {
                let run_checks = || {
                    anyhow::ensure!(
                        matches!(event.event, Event::UnknownTransaction),
                        "reducer should be unknown",
                    );
                    check_val(row.n, 24)
                };
                (one_u16_inserted.take().unwrap())(run_checks());
            });
        });
        // conn.subscription_builder()
        //     .on_applied(move |ctx| {
        //         sub_applied(Ok(()));

        //         // Test that we are notified when a row is inserted.
        //         let db = ctx.db();
        //         let mut one_u8_inserted = Some(counter2.add_test(format!("one_u8_inserted_{who}")));
        //         db.one_u_8().on_insert(move |_, row| {
        //             (one_u8_inserted.take().unwrap())(check_val(row.n, 42));
        //         });
        //         let mut one_u16_inserted = Some(counter2.add_test(format!("one_u16_inserted_{who}")));
        //         db.one_u_16().on_insert(move |event, row| {
        //             let run_checks = || {
        //                 anyhow::ensure!(
        //                     matches!(event.event, Event::UnknownTransaction),
        //                     "reducer should be unknown",
        //                 );
        //                 check_val(row.n, 24)
        //             };
        //             (one_u16_inserted.take().unwrap())(run_checks());
        //         });
        //     })
        //     .on_error(|_| panic!("Subscription error"))
        //     .subscribe(["SELECT * FROM one_u8", "SELECT * FROM one_u16"]);
        conn
    });

    // Ensure both have finished connecting
    // and finished subscribing so that there isn't a race condition
    // between Alice executing the reducer and Bob being connected
    // or Alice executing the reducer and either having subscriptions applied.
    pre_ins_counter.wait_for_all();

    // Alice executes a reducer.
    // This should cause a row callback to be received by Alice and Bob.
    // A reducer callback should only be received by Alice.
    let mut alice_gets_reducer_callback = Some(counter.add_test("gets_reducer_callback_alice"));
    conns[0]
        .reducers()
        .on_insert_one_u_8(move |_, &val| (alice_gets_reducer_callback.take().unwrap())(check_val(val, 42)));
    conns[1]
        .reducers()
        .on_insert_one_u_8(move |_, _| panic!("bob received reducer callback"));
    conns[0].reducers().insert_one_u_8(42).unwrap();

    // Alice executes a reducer but decides not to be notified about it, so they shouldn't.
    conns[0]
        .set_reducer_flags()
        .insert_one_u_16(CallReducerFlags::NoSuccessNotify);
    for conn in &conns {
        conn.reducers()
            .on_insert_one_u_16(move |_, _| panic!("received reducer callback"));
    }
    conns[0].reducers().insert_one_u_16(24).unwrap();

    counter.wait_for_all();

    // For the integrity of the test, ensure that Alice != Bob.
    // We do this after `run_threaded` so that the ids have been filled.
    assert_ne!(conns[0].identity(), conns[1].identity());
}
