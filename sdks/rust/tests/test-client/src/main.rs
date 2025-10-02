#[allow(clippy::too_many_arguments)]
#[allow(clippy::large_enum_variant)]
mod module_bindings;

use core::fmt::Display;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};

use module_bindings::*;

use rand::RngCore;
use spacetimedb_sdk::TableWithPrimaryKey;
use spacetimedb_sdk::{
    credentials, i256, u256, unstable::CallReducerFlags, Compression, ConnectionId, DbConnectionBuilder, DbContext,
    Event, Identity, ReducerEvent, Status, SubscriptionHandle, Table, TimeDuration, Timestamp,
};
use test_counter::TestCounter;

mod simple_test_table;
use simple_test_table::{insert_one, on_insert_one, SimpleTestTable};

mod pk_test_table;
use pk_test_table::{insert_update_delete_one, PkTestTable};

mod unique_test_table;
use unique_test_table::{insert_then_delete_one, UniqueTestTable};

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
        "insert-primitive" => exec_insert_primitive(),
        "subscribe-and-cancel" => exec_subscribe_and_cancel(),
        "subscribe-and-unsubscribe" => exec_subscribe_and_unsubscribe(),
        "subscription-error-smoke-test" => exec_subscription_error_smoke_test(),
        "delete-primitive" => exec_delete_primitive(),
        "update-primitive" => exec_update_primitive(),

        "insert-identity" => exec_insert_identity(),
        "insert-caller-identity" => exec_insert_caller_identity(),
        "delete-identity" => exec_delete_identity(),
        "update-identity" => exec_update_identity(),

        "insert-connection-id" => exec_insert_connection_id(),
        "insert-caller-connection-id" => exec_insert_caller_connection_id(),
        "delete-connection-id" => exec_delete_connection_id(),
        "update-connection-id" => exec_update_connection_id(),

        "insert-timestamp" => exec_insert_timestamp(),
        "insert-call-timestamp" => exec_insert_call_timestamp(),

        "on-reducer" => exec_on_reducer(),
        "fail-reducer" => exec_fail_reducer(),

        "insert-vec" => exec_insert_vec(),
        "insert-option-some" => exec_insert_option_some(),
        "insert-option-none" => exec_insert_option_none(),
        "insert-struct" => exec_insert_struct(),
        "insert-simple-enum" => exec_insert_simple_enum(),
        "insert-enum-with-payload" => exec_insert_enum_with_payload(),

        "insert-delete-large-table" => exec_insert_delete_large_table(),

        "insert-primitives-as-strings" => exec_insert_primitives_as_strings(),

        // "resubscribe" => exec_resubscribe(),
        //
        "reauth-part-1" => exec_reauth_part_1(),
        "reauth-part-2" => exec_reauth_part_2(),

        "should-fail" => exec_should_fail(),

        "reconnect-different-connection-id" => exec_reconnect_different_connection_id(),
        "caller-always-notified" => exec_caller_always_notified(),

        "subscribe-all-select-star" => exec_subscribe_all_select_star(),
        "caller-alice-receives-reducer-callback-but-not-bob" => {
            exec_caller_alice_receives_reducer_callback_but_not_bob()
        }
        "row-deduplication" => exec_row_deduplication(),
        "row-deduplication-join-r-and-s" => exec_row_deduplication_join_r_and_s(),
        "row-deduplication-r-join-s-and-r-joint" => exec_row_deduplication_r_join_s_and_r_join_t(),
        "test-lhs-join-update" => test_lhs_join_update(),
        "test-lhs-join-update-disjoint-queries" => test_lhs_join_update_disjoint_queries(),
        "test-intra-query-bag-semantics-for-join" => test_intra_query_bag_semantics_for_join(),
        "two-different-compression-algos" => exec_two_different_compression_algos(),
        "test-parameterized-subscription" => test_parameterized_subscription(),
        "test-rls-subscription" => test_rls_subscription(),
        "pk-simple-enum" => exec_pk_simple_enum(),
        "indexed-simple-enum" => exec_indexed_simple_enum(),

        "overlapping-subscriptions" => exec_overlapping_subscriptions(),

        _ => panic!("Unknown test: {test}"),
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
    assert_table_empty(ctx.db().one_connection_id())?;

    assert_table_empty(ctx.db().one_timestamp())?;

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
    assert_table_empty(ctx.db().vec_connection_id())?;

    assert_table_empty(ctx.db().vec_timestamp())?;

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
    assert_table_empty(ctx.db().unique_connection_id())?;

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
    assert_table_empty(ctx.db().pk_connection_id())?;

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
    "SELECT * FROM one_connection_id;",
    "SELECT * FROM one_timestamp;",
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
    "SELECT * FROM vec_connection_id;",
    "SELECT * FROM vec_timestamp;",
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
    "SELECT * FROM unique_connection_id;",
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
    "SELECT * FROM pk_connection_id;",
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

fn connect(test_counter: &std::sync::Arc<TestCounter>) -> DbConnection {
    connect_then(test_counter, |_| {})
}

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

fn exec_subscribe_and_cancel() {
    let test_counter = TestCounter::new();
    let cb = test_counter.add_test("unsubscribe_then_called");
    connect_then(&test_counter, {
        move |ctx| {
            let handle = ctx
                .subscription_builder()
                .on_applied(move |_ctx: &SubscriptionEventContext| {
                    panic!("Subscription should never be applied");
                })
                .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
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
            ctx.reducers.insert_one_u_8(1).unwrap();
            let handle_cell: Arc<Mutex<Option<module_bindings::SubscriptionHandle>>> = Arc::new(Mutex::new(None));
            let hc_clone = handle_cell.clone();
            let handle = ctx
                .subscription_builder()
                .on_applied(move |ctx: &SubscriptionEventContext| {
                    let handle = { hc_clone.lock().unwrap().as_ref().unwrap().clone() };
                    assert!(ctx.is_active());
                    assert!(handle.is_active());
                    assert!(!handle.is_ended());
                    assert!(ctx.db.one_u_8().count() == 1);
                    let handle_clone = handle.clone();
                    handle
                        .unsubscribe_then(Box::new(move |ectx| {
                            assert!(!handle_clone.is_active());
                            assert!(handle_clone.is_ended());
                            assert!(ectx.db.one_u_8().count() == 0);
                            cb(Ok(()));
                        }))
                        .unwrap();
                })
                .on_error(|_ctx, error| panic!("Subscription errored: {error:?}"))
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
                .on_applied(move |_ctx: &SubscriptionEventContext| {
                    panic!("Subscription should never be applied");
                })
                .on_error(|_, _| cb(Ok(())))
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

/// This tests that we can serialize and deserialize `ConnectionId` in various contexts.
fn exec_insert_connection_id() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                insert_one::<OneConnectionId>(ctx, &test_counter, ctx.connection_id());

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();
}

/// This tests that we can serialize and deserialize `ConnectionId` in various contexts.
fn exec_insert_caller_connection_id() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                on_insert_one::<OneConnectionId>(ctx, &test_counter, ctx.connection_id(), |event| {
                    matches!(event, Reducer::InsertCallerOneConnectionId)
                });
                ctx.reducers.insert_caller_one_connection_id().unwrap();
                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();
}

/// This test doesn't add much alongside `exec_insert_connection_id` and `exec_delete_primitive`,
/// but it's here for symmetry.
fn exec_delete_connection_id() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                insert_then_delete_one::<UniqueConnectionId>(ctx, &test_counter, ctx.connection_id(), 0xbeef);

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();

    assert_all_tables_empty(&connection).unwrap();
}

/// This tests that we can distinguish between `on_delete` and `on_update` events
/// for tables with `ConnectionId` primary keys.
fn exec_update_connection_id() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    subscribe_all_then(&connection, {
        let test_counter = test_counter.clone();
        move |ctx| {
            insert_update_delete_one::<PkConnectionId>(
                ctx,
                &test_counter,
                ConnectionId::ZERO, // connection.connection_id().unwrap(),
                0xbeef,
                0xbabe,
            );

            sub_applied_nothing_result(assert_all_tables_empty(ctx));
        }
    });

    test_counter.wait_for_all();

    assert_all_tables_empty(&connection).unwrap();
}

fn exec_insert_timestamp() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                insert_one::<OneTimestamp>(ctx, &test_counter, Timestamp::now());

                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            })
        }
    });

    test_counter.wait_for_all();
}

fn exec_insert_call_timestamp() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            subscribe_all_then(ctx, move |ctx| {
                let mut on_insert_result = Some(test_counter.add_test("on_insert"));
                ctx.db.one_timestamp().on_insert(move |ctx, row| {
                    let run_checks = || {
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
                        if reducer_event.caller_connection_id != Some(ctx.connection_id()) {
                            anyhow::bail!(
                                "Expected caller_connection_id to be my own connection_id {:?}, but found {:?}",
                                ctx.connection_id(),
                                reducer_event.caller_connection_id,
                            )
                        }
                        if !matches!(reducer_event.status, Status::Committed) {
                            anyhow::bail!(
                                "Unexpected status. Expected Committed but found {:?}",
                                reducer_event.status
                            );
                        }
                        let expected_reducer = Reducer::InsertCallTimestamp;
                        if reducer_event.reducer != expected_reducer {
                            anyhow::bail!(
                                "Unexpected Reducer in ReducerEvent: expected {expected_reducer:?} but found {:?}",
                                reducer_event.reducer,
                            );
                        };

                        assert_eq_or_bail!(reducer_event.timestamp, row.t);
                        Ok(())
                    };
                    (on_insert_result.take().unwrap())(run_checks());
                });
                ctx.reducers.insert_call_timestamp().unwrap();
            });
            sub_applied_nothing_result(assert_all_tables_empty(ctx));
        }
    });
    test_counter.wait_for_all();
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
            if ctx.event.caller_identity != ctx.identity() {
                anyhow::bail!(
                    "Expected caller_identity to be my own identity {:?}, but found {:?}",
                    ctx.identity(),
                    ctx.event.caller_identity,
                );
            }
            if ctx.event.caller_connection_id != Some(ctx.connection_id()) {
                anyhow::bail!(
                    "Expected caller_connection_id to be my own connection_id {:?}, but found {:?}",
                    ctx.connection_id(),
                    ctx.event.caller_connection_id,
                )
            }
            if !matches!(ctx.event.status, Status::Committed) {
                anyhow::bail!("Unexpected status. Expected Committed but found {:?}", ctx.event.status);
            }
            let expected_reducer = Reducer::InsertOneU8 { n: value };
            if ctx.event.reducer != expected_reducer {
                anyhow::bail!(
                    "Unexpected Reducer in ReducerEvent: expected {expected_reducer:?} but found {:?}",
                    ctx.event.reducer
                );
            }

            if ctx.db.one_u_8().count() != 1 {
                anyhow::bail!("Expected 1 row in table OneU8, but found {}", ctx.db.one_u_8().count());
            }
            let row = ctx.db.one_u_8().iter().next().unwrap();
            if row.n != value {
                anyhow::bail!("Unexpected row value. Expected {value} but found {row:?}");
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
                if ctx.event.caller_identity != ctx.identity() {
                    anyhow::bail!(
                        "Expected caller_identity to be my own identity {:?}, but found {:?}",
                        ctx.identity(),
                        ctx.event.caller_identity,
                    );
                }
                if ctx.event.caller_connection_id != Some(ctx.connection_id()) {
                    anyhow::bail!(
                        "Expected caller_connection_id to be my own connection_id {:?}, but found {:?}",
                        ctx.connection_id(),
                        ctx.event.caller_connection_id,
                    )
                }
                if !matches!(ctx.event.status, Status::Committed) {
                    anyhow::bail!("Unexpected status. Expected Committed but found {:?}", ctx.event.status);
                }
                let expected_reducer = Reducer::InsertPkU8 {
                    n: key,
                    data: initial_data,
                };
                if ctx.event.reducer != expected_reducer {
                    anyhow::bail!(
                        "Unexpected Reducer in ReducerEvent: expected {expected_reducer:?} but found {:?}",
                        ctx.event.reducer
                    );
                }

                if ctx.db.pk_u_8().count() != 1 {
                    anyhow::bail!("Expected 1 row in table PkU8, but found {}", ctx.db.pk_u_8().count());
                }
                let row = ctx.db.pk_u_8().iter().next().unwrap();
                if row.n != key || row.data != initial_data {
                    anyhow::bail!("Unexpected row value. Expected ({key}, {initial_data}) but found {row:?}");
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
                if ctx.event.caller_identity != ctx.identity() {
                    anyhow::bail!(
                        "Expected caller_identity to be my own identity {:?}, but found {:?}",
                        ctx.identity(),
                        ctx.event.caller_identity,
                    );
                }
                if ctx.event.caller_connection_id != Some(ctx.connection_id()) {
                    anyhow::bail!(
                        "Expected caller_connection_id to be my own connection_id {:?}, but found {:?}",
                        ctx.connection_id(),
                        ctx.event.caller_connection_id,
                    )
                }
                if !matches!(ctx.event.status, Status::Failed(_)) {
                    anyhow::bail!("Unexpected status. Expected Committed but found {:?}", ctx.event.status);
                }
                let expected_reducer = Reducer::InsertPkU8 {
                    n: key,
                    data: fail_data,
                };
                if ctx.event.reducer != expected_reducer {
                    anyhow::bail!(
                        "Unexpected Reducer in ReducerEvent: expected {expected_reducer:?} but found {:?}",
                        ctx.event.reducer
                    );
                }

                if ctx.db.pk_u_8().count() != 1 {
                    anyhow::bail!("Expected 1 row in table PkU8, but found {}", ctx.db.pk_u_8().count());
                }
                let row = ctx.db.pk_u_8().iter().next().unwrap();
                if row.n != key || row.data != initial_data {
                    anyhow::bail!("Unexpected row value. Expected ({key}, {initial_data}) but found {row:?}");
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
            insert_one::<VecConnectionId>(ctx, &test_counter, vec![ctx.connection_id()]);

            insert_one::<VecTimestamp>(ctx, &test_counter, vec![Timestamp::now()]);

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
        r: ConnectionId::ZERO,
        s: Timestamp::from_micros_since_unix_epoch(9876543210),
        t: TimeDuration::from_micros(-67_419_000_000_003),
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
        r: vec![ConnectionId::ZERO],
        s: vec![Timestamp::from_micros_since_unix_epoch(9876543210)],
        t: vec![TimeDuration::from_micros(-67_419_000_000_003)],
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

/// This test invokes a reducer with many arguments of many types,
/// and observes a callback for an inserted table with many columns of many types.
fn exec_insert_delete_large_table() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let connection = connect(&test_counter);

    subscribe_all_then(&connection, {
        let test_counter = test_counter.clone();
        let mut insert_result = Some(test_counter.add_test("insert-large-table"));
        let mut delete_result = Some(test_counter.add_test("delete-large-table"));
        move |ctx| {
            let table = ctx.db.large_table();
            table.on_insert(move |ctx, large_table_inserted| {
                if let Some(insert_result) = insert_result.take() {
                    let run_tests = || {
                        assert_eq_or_bail!(large_table(), *large_table_inserted);
                        if !matches!(
                            ctx.event,
                            Event::Reducer(ReducerEvent {
                                reducer: Reducer::InsertLargeTable { .. },
                                ..
                            })
                        ) {
                            anyhow::bail!("Unexpected event: expected InsertLargeTable but found {:?}", ctx.event,);
                        }

                        // Now we'll delete the row we just inserted and check that the delete callback is called.
                        let large_table = large_table();
                        ctx.reducers.delete_large_table(
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
                        )?;

                        Ok(())
                    };
                    insert_result(run_tests());
                }
            });
            table.on_delete(move |ctx, row| {
                if let Some(delete_result) = delete_result.take() {
                    let run_tests = || {
                        assert_eq_or_bail!(large_table(), *row);
                        if !matches!(
                            ctx.event,
                            Event::Reducer(ReducerEvent {
                                reducer: Reducer::DeleteLargeTable { .. },
                                ..
                            })
                        ) {
                            anyhow::bail!("Unexpected event: expected DeleteLargeTable but found {:?}", ctx.event,);
                        }
                        Ok(())
                    };
                    delete_result(run_tests());
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
                s.s.to_string(),
                s.t.to_string(),
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
                                "Unexpected Event: expected reducer InsertPrimitivesAsStrings but found {:?}",
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
        .on_connect(|_, _identity, token| {
            save_result(creds_store().save(token).map_err(Into::into));
        })
        .on_connect_error(|_ctx, error| panic!("Connect failed: {error:?}"))
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

    let token = creds_store().load().unwrap().unwrap();

    DbConnection::builder()
        .on_connect({
            let token = token.clone();
            move |_, _recv_identity, recv_token| {
                let run_checks = || {
                    assert_eq_or_bail!(token, recv_token);
                    Ok(())
                };
                creds_match_result(run_checks());
            }
        })
        .on_connect_error(|_ctx, error| panic!("Connect failed: {error:?}"))
        .with_module_name(name)
        .with_token(Some(token))
        .with_uri(LOCALHOST)
        .build()
        .unwrap()
        .run_threaded();

    test_counter.wait_for_all();
}

// Ensure a new connection gets a different connection id.
fn exec_reconnect_different_connection_id() {
    let initial_test_counter = TestCounter::new();
    let initial_connect_result = initial_test_counter.add_test("connect");

    let disconnect_test_counter = TestCounter::new();
    let disconnect_result = disconnect_test_counter.add_test("disconnect");

    let initial_connection = DbConnection::builder()
        .with_module_name(db_name_or_panic())
        .with_uri(LOCALHOST)
        .on_connect_error(|_ctx, error| panic!("on_connect_error: {error:?}"))
        .on_connect(move |_, _, _| {
            initial_connect_result(Ok(()));
        })
        .on_disconnect(|_, error| match error {
            None => disconnect_result(Ok(())),
            Some(err) => disconnect_result(Err(anyhow::anyhow!("{err:?}"))),
        })
        .build()
        .unwrap();

    initial_connection.run_threaded();

    initial_test_counter.wait_for_all();

    let my_connection_id = initial_connection.connection_id();

    initial_connection.disconnect().unwrap();

    disconnect_test_counter.wait_for_all();

    let reconnect_test_counter = TestCounter::new();
    let reconnect_result = reconnect_test_counter.add_test("reconnect");
    let addr_after_reconnect_result = reconnect_test_counter.add_test("addr_after_reconnect");

    let re_connection = DbConnection::builder()
        .with_module_name(db_name_or_panic())
        .with_uri(LOCALHOST)
        .on_connect_error(|_ctx, error| panic!("on_connect_error: {error:?}"))
        .on_connect(move |ctx, _, _| {
            reconnect_result(Ok(()));
            let run_checks = || {
                // A new connection should have a different connection id.
                anyhow::ensure!(ctx.connection_id() != my_connection_id);
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
            ReducerEvent {
                status: Status::Committed,
                reducer: Reducer::NoOpSucceeds,
                ..
            } => Ok(()),
            _ => Err(anyhow::anyhow!(
                "Unexpected event from no_op_succeeds reducer: {:?}",
                ctx.event,
            )),
        });
    });

    connection.reducers.no_op_succeeds().unwrap();

    test_counter.wait_for_all();
}

/// Duplicates the test `insert_primitive`,
/// but using `SubscriptionBuilder::subscribe_to_all_tables` rather than an explicit query set.
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
        .on_error(|_, _| panic!("Subscription error"))
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

type ResultRecorder = Box<dyn Send + FnOnce(Result<(), anyhow::Error>)>;

/// [`Option::take`] the `result` function, and invoke it with `res`. Panic if `result` is `None`.
///
/// Used in [`exec_row_deduplication`] to determine that row callbacks are invoked only once,
/// since this will panic if invoked on the same `result` function twice.
fn put_result(result: &mut Option<ResultRecorder>, res: Result<(), anyhow::Error>) {
    (result.take().unwrap())(res);
}

fn exec_row_deduplication() {
    let test_counter = TestCounter::new();

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let mut ins_24_result = Some(test_counter.add_test("ins_24"));
    let mut ins_42_result = Some(test_counter.add_test("ins_42"));
    let mut del_24_result = Some(test_counter.add_test("del_24"));
    let mut upd_42_result = Some(test_counter.add_test("upd_42"));

    let conn = connect_then(&test_counter, {
        move |ctx| {
            let queries = [
                "SELECT * FROM pk_u32 WHERE pk_u32.n < 100;",
                "SELECT * FROM pk_u32 WHERE pk_u32.n < 200;",
            ];

            // The general approach in this test is that
            // we expect at most a single `on_X` callback per row.
            // If we receive duplicate callbacks,
            // there's a problem with row deduplication and `put_result` will panic.
            PkU32::on_insert(ctx, move |ctx, i| match i.n {
                24 => {
                    put_result(&mut ins_24_result, Ok(()));
                    // Trigger the delete we expect.
                    PkU32::delete(ctx, 24);
                }
                42 => {
                    put_result(&mut ins_42_result, Ok(()));
                    // Trigger the update we expect.
                    PkU32::update(ctx, 42, 0xfeeb);
                }
                _ => unreachable!("only 24 and 42 were expected insertions"),
            });

            PkU32::on_delete(ctx, move |_, d| match d.n {
                24 => put_result(&mut del_24_result, Ok(())),
                42 => panic!("should not have received delete for 42, only update"),
                x => unreachable!("only 24 and 42 were expected rows, got: {x}"),
            });

            PkU32::on_update(ctx, move |_, d, i| match (d.n, i.n, d.data, i.data) {
                (24, 24, ..) => panic!("should not have received update for 24, only delete"),
                (42, 42, 0xbeef, 0xfeeb) => put_result(&mut upd_42_result, Ok(())),
                x => unreachable!("only 24 and 42 were expected rows, got: `{x:?}`"),
            });

            subscribe_these_then(ctx, &queries, move |ctx| {
                PkU32::insert(ctx, 24, 0xbeef);
                PkU32::insert(ctx, 42, 0xbeef);
                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
        }
    });

    test_counter.wait_for_all();

    // Ensure we're not double counting anything.
    let table = conn.db.pk_u_32();
    assert_eq!(table.count(), 1);
    assert_eq!(table.n().find(&24), None);
    assert_eq!(table.n().find(&42), Some(PkU32 { n: 42, data: 0xfeeb }));
}

fn exec_row_deduplication_join_r_and_s() {
    let test_counter = TestCounter::new();

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let mut pk_u32_on_insert_result = Some(test_counter.add_test("pk_u32_on_insert"));
    let mut pk_u32_on_update_result = Some(test_counter.add_test("pk_u32_on_update"));
    let mut unique_u32_on_insert_result = Some(test_counter.add_test("unique_u32_on_insert"));

    connect_then(&test_counter, {
        move |ctx| {
            let queries = [
                "SELECT * FROM pk_u32;",
                "SELECT unique_u32.* FROM unique_u32 JOIN pk_u32 ON unique_u32.n = pk_u32.n;",
            ];

            // These never happen. In the case of `PkU32` we get an update instead.
            UniqueU32::on_delete(ctx, move |_, _| panic!("we never delete a `UniqueU32`"));
            PkU32::on_delete(ctx, move |_, _| panic!("we never delete a `PkU32`"));

            const KEY: u32 = 42;
            const DU: i32 = 0xbeef;
            const D1: i32 = 50;
            const D2: i32 = 100;

            // Here is where we start.
            subscribe_these_then(ctx, &queries, move |ctx| {
                PkU32::insert(ctx, KEY, D1);
                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
            // We first get an insert for `PkU32` from ^---
            // and then update that row and insert into `UniqueU32` in ---------v.
            PkU32::on_insert(ctx, move |ctx, val| {
                assert_eq!(val.n, KEY);
                assert_eq!(val.data, D1);
                put_result(&mut pk_u32_on_insert_result, Ok(()));
                ctx.reducers.insert_unique_u_32_update_pk_u_32(KEY, DU, D2).unwrap();
            });
            // This is caused by the reducer invocation ^-----
            PkU32::on_update(ctx, move |_, old, new| {
                assert_eq!(old.n, KEY);
                assert_eq!(new.n, KEY);
                assert_eq!(old.data, D1);
                assert_eq!(new.data, D2);
                put_result(&mut pk_u32_on_update_result, Ok(()));
            });
            // This is caused by the reducer invocation ^-----
            UniqueU32::on_insert(ctx, move |_, val| {
                assert_eq!(val.n, KEY);
                assert_eq!(val.data, DU);
                put_result(&mut unique_u32_on_insert_result, Ok(()));
            });
        }
    });

    test_counter.wait_for_all();
}

fn exec_row_deduplication_r_join_s_and_r_join_t() {
    let test_counter: Arc<TestCounter> = TestCounter::new();

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let mut pk_u32_on_insert_result = Some(test_counter.add_test("pk_u32_on_insert"));
    let mut pk_u32_on_delete_result = Some(test_counter.add_test("pk_u32_on_delete"));
    let mut pk_u32_two_on_insert_result = Some(test_counter.add_test("pk_u32_two_on_insert"));

    let count_unique_u32_on_insert = Arc::new(AtomicUsize::new(0));
    let count_unique_u32_on_insert_dup = count_unique_u32_on_insert.clone();

    connect_then(&test_counter, {
        move |ctx| {
            let queries = [
                "SELECT * FROM pk_u32;",
                "SELECT * FROM pk_u32_two;",
                "SELECT unique_u32.* FROM unique_u32 JOIN pk_u32 ON unique_u32.n = pk_u32.n;",
                "SELECT unique_u32.* FROM unique_u32 JOIN pk_u32_two ON unique_u32.n = pk_u32_two.n;",
            ];

            const KEY: u32 = 42;
            const DATA: i32 = 0xbeef;

            UniqueU32::insert(ctx, KEY, DATA);

            subscribe_these_then(ctx, &queries, move |ctx| {
                PkU32::insert(ctx, KEY, DATA);
                sub_applied_nothing_result(assert_all_tables_empty(ctx));
            });
            PkU32::on_insert(ctx, move |ctx, val| {
                assert_eq!(val, &PkU32 { n: KEY, data: DATA });
                put_result(&mut pk_u32_on_insert_result, Ok(()));
                ctx.reducers.delete_pk_u_32_insert_pk_u_32_two(KEY, DATA).unwrap();
            });
            PkU32Two::on_insert(ctx, move |_, val| {
                assert_eq!(val, &PkU32Two { n: KEY, data: DATA });
                put_result(&mut pk_u32_two_on_insert_result, Ok(()));
            });
            PkU32::on_delete(ctx, move |_, val| {
                assert_eq!(val, &PkU32 { n: KEY, data: DATA });
                put_result(&mut pk_u32_on_delete_result, Ok(()));
            });
            UniqueU32::on_insert(ctx, move |_, _| {
                count_unique_u32_on_insert_dup.fetch_add(1, Ordering::SeqCst);
            });
            UniqueU32::on_delete(ctx, move |_, _| panic!());
            PkU32Two::on_delete(ctx, move |_, _| panic!());
        }
    });

    test_counter.wait_for_all();

    assert_eq!(count_unique_u32_on_insert.load(Ordering::SeqCst), 1);
}

/// This test asserts that the correct callbacks are invoked when updating the lhs table of a join
fn test_lhs_join_update() {
    let insert_counter = TestCounter::new();
    let update_counter = TestCounter::new();
    let mut on_update_1 = Some(update_counter.add_test("on_update_1"));
    let mut on_update_2 = Some(update_counter.add_test("on_update_2"));
    let mut on_insert_1 = Some(insert_counter.add_test("on_insert_1"));
    let mut on_insert_2 = Some(insert_counter.add_test("on_insert_2"));

    let conn = Arc::new(connect_then(&update_counter, {
        move |ctx| {
            subscribe_these_then(
                ctx,
                &[
                    "SELECT p.* FROM pk_u32 p WHERE n = 1",
                    "SELECT p.* FROM pk_u32 p JOIN unique_u32 u ON p.n = u.n WHERE u.data > 0 AND u.data < 5",
                ],
                |_| {},
            );
        }
    }));

    conn.reducers.on_insert_pk_u_32(move |_, n, data| {
        if *n == 1 && *data == 0 {
            return put_result(&mut on_insert_1, Ok(()));
        }
        if *n == 2 && *data == 0 {
            return put_result(&mut on_insert_2, Ok(()));
        }
        panic!("unexpected insert: pk_u32(n: {n}, data: {data})");
    });

    conn.reducers.on_update_pk_u_32(move |ctx, n, data| {
        if *n == 2 && *data == 1 {
            PkU32::update(ctx, 2, 0);
            return put_result(&mut on_update_1, Ok(()));
        }
        if *n == 2 && *data == 0 {
            return put_result(&mut on_update_2, Ok(()));
        }
        panic!("unexpected update: pk_u32(n: {n}, data: {data})");
    });

    // Add two pk_u32 rows to the subscription
    conn.reducers.insert_pk_u_32(1, 0).unwrap();
    conn.reducers.insert_pk_u_32(2, 0).unwrap();
    conn.reducers.insert_unique_u_32(1, 3).unwrap();
    conn.reducers.insert_unique_u_32(2, 4).unwrap();

    // Wait for the subscription to be updated,
    // then update one of the pk_u32 rows.
    insert_counter.wait_for_all();
    conn.reducers.update_pk_u_32(2, 1).unwrap();

    // Wait for the second row update for pk_u32
    update_counter.wait_for_all();
}

/// This test asserts that the correct callbacks are invoked when updating the lhs table of a join
fn test_lhs_join_update_disjoint_queries() {
    let insert_counter = TestCounter::new();
    let update_counter = TestCounter::new();
    let mut on_update_1 = Some(update_counter.add_test("on_update_1"));
    let mut on_update_2 = Some(update_counter.add_test("on_update_2"));
    let mut on_insert_1 = Some(insert_counter.add_test("on_insert_1"));
    let mut on_insert_2 = Some(insert_counter.add_test("on_insert_2"));

    let conn = Arc::new(connect_then(&update_counter, {
        move |ctx| {
            subscribe_these_then(ctx, &[
                "SELECT p.* FROM pk_u32 p WHERE n = 1",
                "SELECT p.* FROM pk_u32 p JOIN unique_u32 u ON p.n = u.n WHERE u.data > 0 AND u.data < 5 AND u.n != 1",
            ], |_| {});
        }
    }));

    conn.reducers.on_insert_pk_u_32(move |_, n, data| {
        if *n == 1 && *data == 0 {
            return put_result(&mut on_insert_1, Ok(()));
        }
        if *n == 2 && *data == 0 {
            return put_result(&mut on_insert_2, Ok(()));
        }
        panic!("unexpected insert: pk_u32(n: {n}, data: {data})");
    });

    conn.reducers.on_update_pk_u_32(move |ctx, n, data| {
        if *n == 2 && *data == 1 {
            PkU32::update(ctx, 2, 0);
            return put_result(&mut on_update_1, Ok(()));
        }
        if *n == 2 && *data == 0 {
            return put_result(&mut on_update_2, Ok(()));
        }
        panic!("unexpected update: pk_u32(n: {n}, data: {data})");
    });

    // Add two pk_u32 rows to the subscription
    conn.reducers.insert_pk_u_32(1, 0).unwrap();
    conn.reducers.insert_pk_u_32(2, 0).unwrap();
    conn.reducers.insert_unique_u_32(1, 3).unwrap();
    conn.reducers.insert_unique_u_32(2, 4).unwrap();

    // Wait for the subscription to be updated,
    // then update one of the pk_u32 rows.
    insert_counter.wait_for_all();
    conn.reducers.update_pk_u_32(2, 1).unwrap();

    // Wait for the second row update for pk_u32
    update_counter.wait_for_all();
}

/// Test that when subscribing to a single join query,
/// the server returns a bag of rows to the client - not a set.
///
/// This is a regression test for [2397](https://github.com/clockworklabs/SpacetimeDB/issues/2397),
/// where the server was incorrectly deduplicating incremental subscription updates.
fn test_intra_query_bag_semantics_for_join() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");
    let mut pk_u32_on_delete_result = Some(test_counter.add_test("pk_u32_on_delete"));

    connect_then(&test_counter, {
        move |ctx| {
            subscribe_these_then(
                ctx,
                &[
                    "SELECT * from btree_u32",
                    "SELECT pk_u32.* FROM pk_u32 JOIN btree_u32 ON pk_u32.n = btree_u32.n",
                ],
                move |ctx| {
                    // Insert (n: 0, data: 1) into btree_u32.
                    //
                    // At this point pk_u32 is empty,
                    // so no subscription update will be sent,
                    // and no callbacks invoked.
                    ctx.reducers
                        .insert_into_btree_u_32(vec![BTreeU32 { n: 0, data: 0 }])
                        .unwrap();

                    // Insert (n: 0, data: 0) into pk_u32.
                    // Insert (n: 0, data: 1) into btree_u32.
                    //
                    // Now we have a row that passes the query,
                    // namely pk_u32(n: 0, data: 0),
                    // so an update will be sent from the server,
                    // and on_insert invoked for the row.
                    //
                    // IMPORTANT: The multiplicity of this row is 2.
                    ctx.reducers
                        .insert_into_pk_btree_u_32(vec![PkU32 { n: 0, data: 0 }], vec![BTreeU32 { n: 0, data: 1 }])
                        .unwrap();

                    // Delete (n: 0, data: 0) from btree_u32.
                    //
                    // While this row joins with pk_u32(n: 0, data: 0),
                    // btree_u32(n: 0, data: 1) still joins with it as well.
                    // Hence on_delete should not be invoked,
                    // Only the multiplicity should be decremented by 1.
                    ctx.reducers
                        .delete_from_btree_u_32(vec![BTreeU32 { n: 0, data: 0 }])
                        .unwrap();

                    // Delete (n: 0, data: 1) from btree_u32.
                    //
                    // There are no more rows that join with pk_u32(n: 0, data: 0),
                    // so on_delete should be invoked.
                    ctx.reducers
                        .delete_from_btree_u_32(vec![BTreeU32 { n: 0, data: 1 }])
                        .unwrap();

                    sub_applied_nothing_result(assert_all_tables_empty(ctx));
                },
            );
            PkU32::on_delete(ctx, move |ctx, _| {
                assert!(
                    ctx.db.btree_u_32().count() == 0,
                    "Bag semantics not implemented correctly"
                );
                put_result(&mut pk_u32_on_delete_result, Ok(()));
            });
        }
    });
}

/// Test that several clients subscribing to the same query and using the same protocol (bsatn)
/// can use different compression algorithms than each other.
///
/// This is a regression test.
fn exec_two_different_compression_algos() {
    use Compression::*;

    // Create 32 KiB of random bytes to make it very likely that compression is used.
    // The actual threshold used currently is 1 KiB
    // but let's use more than that in case we change it and forget to update here.
    let mut rng = rand::rng();
    let mut bytes = [0; 1 << 15];
    rng.fill_bytes(&mut bytes);
    let bytes: Arc<[u8]> = bytes.into();

    // Connect with brotli, gzip, and no compression.
    // One of them will insert and all of them will subscribe.
    // All should get back `bytes`.
    fn connect_with_compression(
        test_counter: &Arc<TestCounter>,
        compression_name: &str,
        compression: Compression,
        mut recorder: Option<ResultRecorder>,
        barrier: &Arc<Barrier>,
        expected: &Arc<[u8]>,
    ) {
        let expected1 = expected.clone();
        let expected2 = expected1.clone();
        let barrier = barrier.clone();
        connect_with_then(
            test_counter,
            compression_name,
            |b| b.with_compression(compression),
            move |ctx| {
                subscribe_these_then(ctx, &["SELECT * FROM vec_u8"], move |ctx| {
                    VecU8::on_insert(ctx, move |_, actual| {
                        let actual: &[u8] = actual.n.as_slice();
                        let res = if actual == &*expected1 {
                            Ok(())
                        } else {
                            Err(anyhow::anyhow!(
                                "got bad row, expected: {expected1:?}, actual: {actual:?}"
                            ))
                        };
                        put_result(&mut recorder, res)
                    });

                    // All clients must have subscribed and registered the `on_insert` callback
                    // before we actually insert the row.
                    barrier.wait();

                    if compression == None {
                        VecU8::insert(ctx, expected2.to_vec());
                    }
                })
            },
        );
    }
    let test_counter: Arc<TestCounter> = TestCounter::new();
    let barrier = Arc::new(Barrier::new(3));
    let got_brotli = Some(test_counter.add_test("got_right_row_brotli"));
    let got_gzip = Some(test_counter.add_test("got_right_row_gzip"));
    let got_none = Some(test_counter.add_test("got_right_row_none"));
    connect_with_compression(&test_counter, "brotli", Brotli, got_brotli, &barrier, &bytes);
    connect_with_compression(&test_counter, "gzip", Gzip, got_gzip, &barrier, &bytes);
    connect_with_compression(&test_counter, "none", None, got_none, &barrier, &bytes);
    test_counter.wait_for_all();
}

/// In this test we have two clients issue parameterized subscriptions.
/// These subscriptions are identical syntactically but not semantically,
/// because they are parameterized by `:sender` - the caller's identity.
fn test_parameterized_subscription() {
    let ctr_for_test = TestCounter::new();
    let ctr_for_subs = TestCounter::new();
    let sub_0 = Some(ctr_for_subs.add_test("sub_0"));
    let sub_1 = Some(ctr_for_subs.add_test("sub_1"));
    let insert_0 = Some(ctr_for_test.add_test("insert_0"));
    let insert_1 = Some(ctr_for_test.add_test("insert_1"));
    let update_0 = Some(ctr_for_test.add_test("update_0"));
    let update_1 = Some(ctr_for_test.add_test("update_1"));

    fn subscribe_and_update(
        test_name: &str,
        old: i32,
        new: i32,
        waiters: [Arc<TestCounter>; 2],
        senders: [Option<ResultRecorder>; 3],
    ) {
        let [ctr_for_test, ctr_for_subs] = waiters;
        let [mut record_sub, mut record_ins, mut record_upd] = senders;
        connect_with_then(&ctr_for_test, test_name, |builder| builder, {
            move |ctx| {
                let sender = ctx.identity();
                subscribe_these_then(ctx, &["SELECT * FROM pk_identity WHERE i = :sender"], move |ctx| {
                    put_result(&mut record_sub, Ok(()));
                    // Wait to insert until both client connections have been made
                    ctr_for_subs.wait_for_all();
                    PkIdentity::insert(ctx, sender, old);
                    PkIdentity::update(ctx, sender, new);
                });
                PkIdentity::on_insert(ctx, move |_, row| {
                    assert_eq!(row.i, sender);
                    assert_eq!(row.data, old);
                    put_result(&mut record_ins, Ok(()));
                });
                PkIdentity::on_update(ctx, move |_, old_row, new_row| {
                    assert_eq!(old_row.i, sender);
                    assert_eq!(new_row.i, sender);
                    assert_eq!(old_row.data, old);
                    assert_eq!(new_row.data, new);
                    put_result(&mut record_upd, Ok(()));
                });
            }
        });
    }

    subscribe_and_update(
        "client_0",
        1,
        2,
        [ctr_for_test.clone(), ctr_for_subs.clone()],
        [sub_0, insert_0, update_0],
    );
    subscribe_and_update(
        "client_1",
        3,
        4,
        [ctr_for_test.clone(), ctr_for_subs.clone()],
        [sub_1, insert_1, update_1],
    );
    ctr_for_test.wait_for_all();
}

/// In this test we have two clients subscribe to the `users` table.
/// Access to this table is controlled using the following RLS rule:
/// ```rust
/// #[spacetimedb::client_visibility_filter]
/// const USERS_FILTER: spacetimedb::Filter = spacetimedb::Filter::Sql(
///     "SELECT * FROM users WHERE identity = :sender"
/// );
/// ```
/// Hence each client should receive different rows.
fn test_rls_subscription() {
    let ctr_for_test = TestCounter::new();
    let ctr_for_subs = TestCounter::new();
    let sub_0 = Some(ctr_for_subs.add_test("sub_0"));
    let sub_1 = Some(ctr_for_subs.add_test("sub_1"));
    let ins_0 = Some(ctr_for_test.add_test("insert_0"));
    let ins_1 = Some(ctr_for_test.add_test("insert_1"));

    fn subscribe_and_update(
        test_name: &str,
        user_name: &str,
        waiters: [Arc<TestCounter>; 2],
        senders: [Option<ResultRecorder>; 2],
    ) {
        let [ctr_for_test, ctr_for_subs] = waiters;
        let [mut record_sub, mut record_ins] = senders;
        let user_name = user_name.to_owned();
        let expected_name = user_name.to_owned();
        connect_with_then(&ctr_for_test, test_name, |builder| builder, {
            move |ctx| {
                let sender = ctx.identity();
                let expected_identity = sender;
                subscribe_these_then(ctx, &["SELECT * FROM users"], move |ctx| {
                    put_result(&mut record_sub, Ok(()));
                    // Wait to insert until both client connections have been made
                    ctr_for_subs.wait_for_all();
                    ctx.reducers.insert_user(user_name, sender).unwrap();
                });
                ctx.db.users().on_insert(move |_, user| {
                    assert_eq!(user.name, expected_name);
                    assert_eq!(user.identity, expected_identity);
                    put_result(&mut record_ins, Ok(()));
                });
            }
        });
    }

    subscribe_and_update(
        "client_0",
        "Alice",
        [ctr_for_test.clone(), ctr_for_subs.clone()],
        [sub_0, ins_0],
    );
    subscribe_and_update(
        "client_1",
        "Bob",
        [ctr_for_test.clone(), ctr_for_subs.clone()],
        [sub_1, ins_1],
    );
    ctr_for_test.wait_for_all();
}

fn exec_pk_simple_enum() {
    let test_counter: Arc<TestCounter> = TestCounter::new();
    let mut updated = Some(test_counter.add_test("updated"));
    connect_then(&test_counter, move |ctx| {
        subscribe_these_then(ctx, &["SELECT * FROM pk_simple_enum"], move |ctx| {
            let data1 = 42;
            let data2 = 24;
            let a = SimpleEnum::Two;
            ctx.db.pk_simple_enum().on_update(move |_, old, new| {
                assert_eq!(old.data, data1);
                assert_eq!(new.data, data2);
                assert_eq!(old.a, a);
                assert_eq!(new.a, a);
                put_result(&mut updated, Ok(()));
            });
            ctx.db.pk_simple_enum().on_insert(move |ctx, row| {
                assert_eq!(row.data, data1);
                assert_eq!(row.a, a);
                ctx.reducers().update_pk_simple_enum(a, data2).unwrap();
            });
            ctx.db.pk_simple_enum().on_delete(|_, _| unreachable!());
            ctx.reducers().insert_pk_simple_enum(a, data1).unwrap();
        });
    });
    test_counter.wait_for_all();
}

fn exec_indexed_simple_enum() {
    let test_counter: Arc<TestCounter> = TestCounter::new();
    let mut updated = Some(test_counter.add_test("updated"));
    connect_then(&test_counter, move |ctx| {
        subscribe_these_then(ctx, &["SELECT * FROM indexed_simple_enum"], move |ctx| {
            let a1 = SimpleEnum::Two;
            let a2 = SimpleEnum::One;
            ctx.db.indexed_simple_enum().on_insert(move |ctx, row| match &row.n {
                SimpleEnum::Two => ctx.reducers().update_indexed_simple_enum(a1, a2).unwrap(),
                SimpleEnum::One => {
                    assert_eq!(row.n, a2);
                    put_result(&mut updated, Ok(()));
                }
                SimpleEnum::Zero => unreachable!(),
            });
            ctx.reducers().insert_into_indexed_simple_enum(a1).unwrap();
        });
    });
    test_counter.wait_for_all();
}

/// This tests for a bug we once had where the Rust client SDK would
/// drop all but the last `TableUpdate` for a particular table within a `DatabaseUpdate`.
///
/// This manifested as a panic when applying incremental updates
/// after an initial subscription to two or more overlapping queries each of which matched the same row.
/// That row would incorrectly have multiplicity 1 in the client cache,
/// since the SDK would ignore all but the first query's initial responses,
/// but would then see incremental updates from all of the queries.
///
/// A simple reproducer is available at [https://github.com/lavirlifiliol/spacetime-repro].
fn exec_overlapping_subscriptions() {
    // First, a bit of setup: insert the row `{ n: 1, data: 0 }`,
    // and wait for it to be present.
    let setup_counter = TestCounter::new();

    let call_insert_result = setup_counter.add_test("call_insert_reducer");
    let mut row_inserted = Some(setup_counter.add_test("insert_reducer_done"));

    let conn = connect_then(&setup_counter, move |ctx| {
        ctx.reducers.on_insert_pk_u_8(move |ctx, _n, _data| {
            (row_inserted.take().unwrap())(match &ctx.event.status {
                Status::Committed => Ok(()),
                s @ (Status::Failed(_) | Status::OutOfEnergy) => {
                    Err(anyhow::anyhow!("insert_pk_u_8 during setup failed: {s:?}"))
                }
            });
        });

        call_insert_result(ctx.reducers.insert_pk_u_8(1, 0).map_err(|e| e.into()));
    });

    setup_counter.wait_for_all();

    let test_counter = TestCounter::new();

    let subscribe_result = test_counter.add_test("subscribe_with_row_present");

    let call_update_result = test_counter.add_test("call_update_reducer");

    let mut update_result = Some(test_counter.add_test("update_row"));

    // Now, subscribe to two queries which each match that row.
    subscribe_these_then(
        &conn,
        &["select * from pk_u8 where n < 100", "select * from pk_u8 where n > 0"],
        move |ctx| {
            // It's not exposed to users of the SDK, so we won't assert on it,
            // but we expect the row to have multiplicity 2.
            subscribe_result(if ctx.db.pk_u_8().count() == 1 {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Expected one row for PkU8 but found {}",
                    ctx.db.pk_u_8().count()
                ))
            });
        },
    );

    // Once the row is in the cache, update it by replacing it with `{ n: 1, data: 1 }`.
    conn.db.pk_u_8().on_update(move |ctx, old, new| {
        // It's not exposed, so no assert,
        // but we expect to have received two deletes for the old row,
        // and two inserts for the new row.
        // The SDK will combine all of these into a single update event.
        (update_result.take().unwrap())((|| {
            anyhow::ensure!(old.n == new.n);
            anyhow::ensure!(old.n == 1);
            anyhow::ensure!(old.data == 0);
            anyhow::ensure!(new.data == 1);
            anyhow::ensure!(ctx.db.pk_u_8().count() == 1);
            Ok(())
        })())
    });

    call_update_result(conn.reducers.update_pk_u_8(1, 1).map_err(|e| e.into()));

    test_counter.wait_for_all();
}
