use spacetimedb_sdk::{
    disconnect,
    identity::{address, identity, load_credentials, once_on_connect, save_credentials},
    once_on_disconnect, once_on_subscription_applied,
    reducer::Status,
    subscribe,
    table::TableType,
};

#[allow(clippy::too_many_arguments)]
#[allow(clippy::large_enum_variant)]
mod module_bindings;

use module_bindings::*;

use test_counter::TestCounter;

mod simple_test_table;
use simple_test_table::insert_one;

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

        // Close the websocket gracefully before exiting.
        spacetimedb_sdk::disconnect();

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
        "delete_primitive" => exec_delete_primitive(),
        "update_primitive" => exec_update_primitive(),

        "insert_identity" => exec_insert_identity(),
        "delete_identity" => exec_delete_identity(),
        "update_identity" => exec_update_identity(),

        "insert_address" => exec_insert_address(),
        "delete_address" => exec_delete_address(),
        "update_address" => exec_update_address(),

        "on_reducer" => exec_on_reducer(),
        "fail_reducer" => exec_fail_reducer(),

        "insert_vec" => exec_insert_vec(),

        "insert_struct" => exec_insert_struct(),
        "insert_simple_enum" => exec_insert_simple_enum(),
        "insert_enum_with_payload" => exec_insert_enum_with_payload(),

        "insert_long_table" => exec_insert_long_table(),

        "resubscribe" => exec_resubscribe(),

        "reconnect" => exec_reconnect(),

        "reauth_part_1" => exec_reauth_part_1(),
        "reauth_part_2" => exec_reauth_part_2(),

        "should_fail" => exec_should_fail(),

        "reconnect_same_address" => exec_reconnect_same_address(),

        _ => panic!("Unknown test: {}", test),
    }
}

fn assert_table_empty<T: TableType>() -> anyhow::Result<()> {
    let count = T::count();
    if count != 0 {
        anyhow::bail!(
            "Expected table {} to be empty, but found {} rows resident",
            T::TABLE_NAME,
            count,
        )
    }
    Ok(())
}

/// Each test runs against a fresh DB, so all tables should be empty until we call an insert reducer.
///
/// We'll call this function within our initial `on_subscription_applied` callback to verify that.
fn assert_all_tables_empty() -> anyhow::Result<()> {
    assert_table_empty::<OneU8>()?;
    assert_table_empty::<OneU16>()?;
    assert_table_empty::<OneU32>()?;
    assert_table_empty::<OneU64>()?;
    assert_table_empty::<OneU128>()?;

    assert_table_empty::<OneI8>()?;
    assert_table_empty::<OneI16>()?;
    assert_table_empty::<OneI32>()?;
    assert_table_empty::<OneI64>()?;
    assert_table_empty::<OneI128>()?;

    assert_table_empty::<OneBool>()?;

    assert_table_empty::<OneF32>()?;
    assert_table_empty::<OneF64>()?;

    assert_table_empty::<OneString>()?;
    assert_table_empty::<OneIdentity>()?;
    assert_table_empty::<OneAddress>()?;

    assert_table_empty::<OneSimpleEnum>()?;
    assert_table_empty::<OneEnumWithPayload>()?;

    assert_table_empty::<OneUnitStruct>()?;
    assert_table_empty::<OneByteStruct>()?;
    assert_table_empty::<OneEveryPrimitiveStruct>()?;
    assert_table_empty::<OneEveryVecStruct>()?;

    assert_table_empty::<VecU8>()?;
    assert_table_empty::<VecU16>()?;
    assert_table_empty::<VecU32>()?;
    assert_table_empty::<VecU64>()?;
    assert_table_empty::<VecU128>()?;

    assert_table_empty::<VecI8>()?;
    assert_table_empty::<VecI16>()?;
    assert_table_empty::<VecI32>()?;
    assert_table_empty::<VecI64>()?;
    assert_table_empty::<VecI128>()?;

    assert_table_empty::<VecBool>()?;

    assert_table_empty::<VecF32>()?;
    assert_table_empty::<VecF64>()?;

    assert_table_empty::<VecString>()?;
    assert_table_empty::<VecIdentity>()?;
    assert_table_empty::<VecAddress>()?;

    assert_table_empty::<VecSimpleEnum>()?;
    assert_table_empty::<VecEnumWithPayload>()?;

    assert_table_empty::<VecUnitStruct>()?;
    assert_table_empty::<VecByteStruct>()?;
    assert_table_empty::<VecEveryPrimitiveStruct>()?;
    assert_table_empty::<VecEveryVecStruct>()?;

    assert_table_empty::<UniqueU8>()?;
    assert_table_empty::<UniqueU16>()?;
    assert_table_empty::<UniqueU32>()?;
    assert_table_empty::<UniqueU64>()?;
    assert_table_empty::<UniqueU128>()?;

    assert_table_empty::<UniqueI8>()?;
    assert_table_empty::<UniqueI16>()?;
    assert_table_empty::<UniqueI32>()?;
    assert_table_empty::<UniqueI64>()?;
    assert_table_empty::<UniqueI128>()?;

    assert_table_empty::<UniqueBool>()?;

    assert_table_empty::<UniqueString>()?;
    assert_table_empty::<UniqueIdentity>()?;
    assert_table_empty::<UniqueAddress>()?;

    assert_table_empty::<PkU8>()?;
    assert_table_empty::<PkU16>()?;
    assert_table_empty::<PkU32>()?;
    assert_table_empty::<PkU64>()?;
    assert_table_empty::<PkU128>()?;

    assert_table_empty::<PkI8>()?;
    assert_table_empty::<PkI16>()?;
    assert_table_empty::<PkI32>()?;
    assert_table_empty::<PkI64>()?;
    assert_table_empty::<PkI128>()?;

    assert_table_empty::<PkBool>()?;

    assert_table_empty::<PkString>()?;
    assert_table_empty::<PkIdentity>()?;
    assert_table_empty::<PkAddress>()?;

    assert_table_empty::<LargeTable>()?;

    assert_table_empty::<TableHoldsTable>()?;

    Ok(())
}

/// A great big honking query that subscribes to all rows from all tables.
const SUBSCRIBE_ALL: &[&str] = &[
    "SELECT * FROM OneU8;",
    "SELECT * FROM OneU16;",
    "SELECT * FROM OneU32;",
    "SELECT * FROM OneU64;",
    "SELECT * FROM OneU128;",
    "SELECT * FROM OneI8;",
    "SELECT * FROM OneI16;",
    "SELECT * FROM OneI32;",
    "SELECT * FROM OneI64;",
    "SELECT * FROM OneI128;",
    "SELECT * FROM OneBool;",
    "SELECT * FROM OneF32;",
    "SELECT * FROM OneF64;",
    "SELECT * FROM OneString;",
    "SELECT * FROM OneIdentity;",
    "SELECT * FROM OneAddress;",
    "SELECT * FROM OneSimpleEnum;",
    "SELECT * FROM OneEnumWithPayload;",
    "SELECT * FROM OneUnitStruct;",
    "SELECT * FROM OneByteStruct;",
    "SELECT * FROM OneEveryPrimitiveStruct;",
    "SELECT * FROM OneEveryVecStruct;",
    "SELECT * FROM VecU8;",
    "SELECT * FROM VecU16;",
    "SELECT * FROM VecU32;",
    "SELECT * FROM VecU64;",
    "SELECT * FROM VecU128;",
    "SELECT * FROM VecI8;",
    "SELECT * FROM VecI16;",
    "SELECT * FROM VecI32;",
    "SELECT * FROM VecI64;",
    "SELECT * FROM VecI128;",
    "SELECT * FROM VecBool;",
    "SELECT * FROM VecF32;",
    "SELECT * FROM VecF64;",
    "SELECT * FROM VecString;",
    "SELECT * FROM VecIdentity;",
    "SELECT * FROM VecAddress;",
    "SELECT * FROM VecSimpleEnum;",
    "SELECT * FROM VecEnumWithPayload;",
    "SELECT * FROM VecUnitStruct;",
    "SELECT * FROM VecByteStruct;",
    "SELECT * FROM VecEveryPrimitiveStruct;",
    "SELECT * FROM VecEveryVecStruct;",
    "SELECT * FROM UniqueU8;",
    "SELECT * FROM UniqueU16;",
    "SELECT * FROM UniqueU32;",
    "SELECT * FROM UniqueU64;",
    "SELECT * FROM UniqueU128;",
    "SELECT * FROM UniqueI8;",
    "SELECT * FROM UniqueI16;",
    "SELECT * FROM UniqueI32;",
    "SELECT * FROM UniqueI64;",
    "SELECT * FROM UniqueI128;",
    "SELECT * FROM UniqueBool;",
    "SELECT * FROM UniqueString;",
    "SELECT * FROM UniqueIdentity;",
    "SELECT * FROM UniqueAddress;",
    "SELECT * FROM PkU8;",
    "SELECT * FROM PkU16;",
    "SELECT * FROM PkU32;",
    "SELECT * FROM PkU64;",
    "SELECT * FROM PkU128;",
    "SELECT * FROM PkI8;",
    "SELECT * FROM PkI16;",
    "SELECT * FROM PkI32;",
    "SELECT * FROM PkI64;",
    "SELECT * FROM PkI128;",
    "SELECT * FROM PkBool;",
    "SELECT * FROM PkString;",
    "SELECT * FROM PkIdentity;",
    "SELECT * FROM PkAddress;",
    "SELECT * FROM LargeTable;",
    "SELECT * FROM TableHoldsTable;",
];

/// This tests that we can:
/// - Pass primitive types to reducers.
/// - Deserialize primitive types in rows and in reducer arguments.
/// - Observe `on_insert` callbacks with appropriate reducer events.
fn exec_insert_primitive() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_one::<OneU8>(&test_counter, 0);
            insert_one::<OneU16>(&test_counter, 0);
            insert_one::<OneU32>(&test_counter, 0);
            insert_one::<OneU64>(&test_counter, 0);
            insert_one::<OneU128>(&test_counter, 0);

            insert_one::<OneI8>(&test_counter, 0);
            insert_one::<OneI16>(&test_counter, 0);
            insert_one::<OneI32>(&test_counter, 0);
            insert_one::<OneI64>(&test_counter, 0);
            insert_one::<OneI128>(&test_counter, 0);

            insert_one::<OneBool>(&test_counter, false);

            insert_one::<OneF32>(&test_counter, 0.0);
            insert_one::<OneF64>(&test_counter, 0.0);

            insert_one::<OneString>(&test_counter, "".to_string());

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

/// This tests that we can observe `on_delete` callbacks.
fn exec_delete_primitive() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_then_delete_one::<UniqueU8>(&test_counter, 0, 0xbeef);
            insert_then_delete_one::<UniqueU16>(&test_counter, 0, 0xbeef);
            insert_then_delete_one::<UniqueU32>(&test_counter, 0, 0xbeef);
            insert_then_delete_one::<UniqueU64>(&test_counter, 0, 0xbeef);
            insert_then_delete_one::<UniqueU128>(&test_counter, 0, 0xbeef);

            insert_then_delete_one::<UniqueI8>(&test_counter, 0, 0xbeef);
            insert_then_delete_one::<UniqueI16>(&test_counter, 0, 0xbeef);
            insert_then_delete_one::<UniqueI32>(&test_counter, 0, 0xbeef);
            insert_then_delete_one::<UniqueI64>(&test_counter, 0, 0xbeef);
            insert_then_delete_one::<UniqueI128>(&test_counter, 0, 0xbeef);

            insert_then_delete_one::<UniqueBool>(&test_counter, false, 0xbeef);

            insert_then_delete_one::<UniqueString>(&test_counter, "".to_string(), 0xbeef);

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();

    assert_all_tables_empty().unwrap();
}

/// This tests that we can distinguish between `on_update` and `on_delete` callbacks for tables with primary keys.
fn exec_update_primitive() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_update_delete_one::<PkU8>(&test_counter, 0, 0xbeef, 0xbabe);
            insert_update_delete_one::<PkU16>(&test_counter, 0, 0xbeef, 0xbabe);
            insert_update_delete_one::<PkU32>(&test_counter, 0, 0xbeef, 0xbabe);
            insert_update_delete_one::<PkU64>(&test_counter, 0, 0xbeef, 0xbabe);
            insert_update_delete_one::<PkU128>(&test_counter, 0, 0xbeef, 0xbabe);

            insert_update_delete_one::<PkI8>(&test_counter, 0, 0xbeef, 0xbabe);
            insert_update_delete_one::<PkI16>(&test_counter, 0, 0xbeef, 0xbabe);
            insert_update_delete_one::<PkI32>(&test_counter, 0, 0xbeef, 0xbabe);
            insert_update_delete_one::<PkI64>(&test_counter, 0, 0xbeef, 0xbabe);
            insert_update_delete_one::<PkI128>(&test_counter, 0, 0xbeef, 0xbabe);

            insert_update_delete_one::<PkBool>(&test_counter, false, 0xbeef, 0xbabe);

            insert_update_delete_one::<PkString>(&test_counter, "".to_string(), 0xbeef, 0xbabe);

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();

    assert_all_tables_empty().unwrap();
}

/// This tests that we can serialize and deserialize `Identity` in various contexts.
fn exec_insert_identity() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_one::<OneIdentity>(&test_counter, identity().unwrap());

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

/// This test doesn't add much alongside `exec_insert_identity` and `exec_delete_primitive`,
/// but it's here for symmetry.
fn exec_delete_identity() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_then_delete_one::<UniqueIdentity>(&test_counter, identity().unwrap(), 0xbeef);

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();

    assert_all_tables_empty().unwrap();
}

/// This tests that we can distinguish between `on_delete` and `on_update` events
/// for tables with `Identity` primary keys.
fn exec_update_identity() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_update_delete_one::<PkIdentity>(&test_counter, identity().unwrap(), 0xbeef, 0xbabe);

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();

    assert_all_tables_empty().unwrap();
}

/// This tests that we can serialize and deserialize `Address` in various contexts.
fn exec_insert_address() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_one::<OneAddress>(&test_counter, address().unwrap());

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

/// This test doesn't add much alongside `exec_insert_address` and `exec_delete_primitive`,
/// but it's here for symmetry.
fn exec_delete_address() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_then_delete_one::<UniqueAddress>(&test_counter, address().unwrap(), 0xbeef);

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();

    assert_all_tables_empty().unwrap();
}

/// This tests that we can distinguish between `on_delete` and `on_update` events
/// for tables with `Address` primary keys.
fn exec_update_address() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_update_delete_one::<PkAddress>(&test_counter, address().unwrap(), 0xbeef, 0xbabe);

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();

    assert_all_tables_empty().unwrap();
}

/// This tests that we can observe reducer callbacks for successful reducer runs.
fn exec_on_reducer() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let reducer_result = test_counter.add_test("reducer-callback");

    let value = 128;

    once_on_insert_one_u_8(move |caller_id, caller_addr, status, arg| {
        let run_checks = || {
            if *arg != value {
                anyhow::bail!("Unexpected reducer argument. Expected {} but found {}", value, *arg);
            }
            if *caller_id != identity().unwrap() {
                anyhow::bail!(
                    "Unexpected caller_id. Expected:\n{:?}\nFound:\n{:?}",
                    identity().unwrap(),
                    caller_id
                );
            }
            if caller_addr != Some(address().unwrap()) {
                anyhow::bail!(
                    "Unexpected caller_addr. Expected:\n{:?}\nFound:\n{:?}",
                    address().unwrap(),
                    caller_addr
                );
            }
            if !matches!(status, Status::Committed) {
                anyhow::bail!("Unexpected status. Expected Committed but found {:?}", status);
            }
            if OneU8::count() != 1 {
                anyhow::bail!("Expected 1 row in table OneU8, but found {}", OneU8::count());
            }
            let row = OneU8::iter().next().unwrap();
            if row.n != value {
                anyhow::bail!("Unexpected row value. Expected {} but found {:?}", value, row);
            }
            Ok(())
        };

        reducer_result(run_checks());
    });

    once_on_subscription_applied(move || {
        insert_one_u_8(value);

        sub_applied_nothing_result(assert_all_tables_empty());
    });

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

/// This tests that we can observe reducer callbacks for failed reducers.
fn exec_fail_reducer() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let reducer_success_result = test_counter.add_test("reducer-callback-success");
    let reducer_fail_result = test_counter.add_test("reducer-callback-fail");

    let key = 128;
    let initial_data = 0xbeef;
    let fail_data = 0xbabe;

    once_on_insert_pk_u_8(move |caller_id, caller_addr, status, arg_key, arg_val| {
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
            if *caller_id != identity().unwrap() {
                anyhow::bail!(
                    "Unexpected caller_id. Expected:\n{:?}\nFound:\n{:?}",
                    identity().unwrap(),
                    caller_id,
                );
            }
            if caller_addr != Some(address().unwrap()) {
                anyhow::bail!(
                    "Unexpected caller_addr. Expected:\n{:?}\nFound:\n{:?}",
                    address().unwrap(),
                    caller_addr,
                );
            }
            if !matches!(status, Status::Committed) {
                anyhow::bail!("Unexpected status. Expected Committed but found {:?}", status);
            }
            if PkU8::count() != 1 {
                anyhow::bail!("Expected 1 row in table PkU8, but found {}", PkU8::count());
            }
            let row = PkU8::iter().next().unwrap();
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

        reducer_success_result(run_checks());

        once_on_insert_pk_u_8(move |caller_id, caller_addr, status, arg_key, arg_val| {
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
                if *caller_id != identity().unwrap() {
                    anyhow::bail!(
                        "Unexpected caller_id. Expected:\n{:?}\nFound:\n{:?}",
                        identity().unwrap(),
                        caller_id,
                    );
                }
                if caller_addr != Some(address().unwrap()) {
                    anyhow::bail!(
                        "Unexpected caller_addr. Expected:\n{:?}\nFound:\n{:?}",
                        address().unwrap(),
                        caller_addr,
                    )
                }
                if !matches!(status, Status::Failed(_)) {
                    anyhow::bail!("Unexpected status. Expected Failed but found {:?}", status);
                }
                if PkU8::count() != 1 {
                    anyhow::bail!("Expected 1 row in table PkU8, but found {}", PkU8::count());
                }
                let row = PkU8::iter().next().unwrap();
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

            reducer_fail_result(run_checks());
        });

        insert_pk_u_8(key, fail_data);
    });

    once_on_subscription_applied(move || {
        insert_pk_u_8(key, initial_data);

        sub_applied_nothing_result(assert_all_tables_empty());
    });

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

/// This tests that we can serialize and deserialize `Vec<?>` in various contexts.
fn exec_insert_vec() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_one::<VecU8>(&test_counter, vec![0, 1]);
            insert_one::<VecU16>(&test_counter, vec![0, 1]);
            insert_one::<VecU32>(&test_counter, vec![0, 1]);
            insert_one::<VecU64>(&test_counter, vec![0, 1]);
            insert_one::<VecU128>(&test_counter, vec![0, 1]);

            insert_one::<VecI8>(&test_counter, vec![0, 1]);
            insert_one::<VecI16>(&test_counter, vec![0, 1]);
            insert_one::<VecI32>(&test_counter, vec![0, 1]);
            insert_one::<VecI64>(&test_counter, vec![0, 1]);
            insert_one::<VecI128>(&test_counter, vec![0, 1]);

            insert_one::<VecBool>(&test_counter, vec![false, true]);

            insert_one::<VecF32>(&test_counter, vec![0.0, 1.0]);
            insert_one::<VecF64>(&test_counter, vec![0.0, 1.0]);

            insert_one::<VecString>(&test_counter, vec!["zero".to_string(), "one".to_string()]);

            insert_one::<VecIdentity>(&test_counter, vec![identity().unwrap()]);

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

/// This tests that we can serialize and deserialize structs in various contexts.
fn exec_insert_struct() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_one::<OneUnitStruct>(&test_counter, UnitStruct {});
            insert_one::<OneByteStruct>(&test_counter, ByteStruct { b: 0 });
            insert_one::<OneEveryPrimitiveStruct>(
                &test_counter,
                EveryPrimitiveStruct {
                    a: 0,
                    b: 1,
                    c: 2,
                    d: 3,
                    e: 4,
                    f: -1,
                    g: -2,
                    h: -3,
                    i: -4,
                    j: -5,
                    k: false,
                    l: 1.0,
                    m: -1.0,
                    n: "string".to_string(),
                    o: identity().unwrap(),
                    p: address().unwrap(),
                },
            );
            insert_one::<OneEveryVecStruct>(
                &test_counter,
                EveryVecStruct {
                    a: vec![],
                    b: vec![1],
                    c: vec![2, 2],
                    d: vec![3, 3, 3],
                    e: vec![4, 4, 4, 4],
                    f: vec![-1],
                    g: vec![-2, -2],
                    h: vec![-3, -3, -3],
                    i: vec![-4, -4, -4, -4],
                    j: vec![-5, -5, -5, -5, -5],
                    k: vec![false, true, true, false],
                    l: vec![0.0, -1.0, 1.0, -2.0, 2.0],
                    m: vec![0.0, -0.5, 0.5, -1.5, 1.5],
                    n: ["vec", "of", "strings"].into_iter().map(str::to_string).collect(),
                    o: vec![identity().unwrap()],
                    p: vec![address().unwrap()],
                },
            );

            insert_one::<VecUnitStruct>(&test_counter, vec![UnitStruct {}]);
            insert_one::<VecByteStruct>(&test_counter, vec![ByteStruct { b: 0 }]);
            insert_one::<VecEveryPrimitiveStruct>(
                &test_counter,
                vec![EveryPrimitiveStruct {
                    a: 0,
                    b: 1,
                    c: 2,
                    d: 3,
                    e: 4,
                    f: -1,
                    g: -2,
                    h: -3,
                    i: -4,
                    j: -5,
                    k: false,
                    l: 1.0,
                    m: -1.0,
                    n: "string".to_string(),
                    o: identity().unwrap(),
                    p: address().unwrap(),
                }],
            );
            insert_one::<VecEveryVecStruct>(
                &test_counter,
                vec![EveryVecStruct {
                    a: vec![],
                    b: vec![1],
                    c: vec![2, 2],
                    d: vec![3, 3, 3],
                    e: vec![4, 4, 4, 4],
                    f: vec![-1],
                    g: vec![-2, -2],
                    h: vec![-3, -3, -3],
                    i: vec![-4, -4, -4, -4],
                    j: vec![-5, -5, -5, -5, -5],
                    k: vec![false, true, true, false],
                    l: vec![0.0, -1.0, 1.0, -2.0, 2.0],
                    m: vec![0.0, -0.5, 0.5, -1.5, 1.5],
                    n: ["vec", "of", "strings"].into_iter().map(str::to_string).collect(),
                    o: vec![identity().unwrap()],
                    p: vec![address().unwrap()],
                }],
            );

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

/// This tests that we can serialize and deserialize C-style enums in various contexts.
fn exec_insert_simple_enum() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_one::<OneSimpleEnum>(&test_counter, SimpleEnum::One);
            insert_one::<VecSimpleEnum>(&test_counter, vec![SimpleEnum::Zero, SimpleEnum::One, SimpleEnum::Two]);

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

/// This tests that we can serialize and deserialize sum types in various contexts.
fn exec_insert_enum_with_payload() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            insert_one::<OneEnumWithPayload>(&test_counter, EnumWithPayload::U8(0));
            insert_one::<VecEnumWithPayload>(
                &test_counter,
                vec![
                    EnumWithPayload::U8(0),
                    EnumWithPayload::U16(1),
                    EnumWithPayload::U32(2),
                    EnumWithPayload::U64(3),
                    EnumWithPayload::U128(4),
                    EnumWithPayload::I8(0),
                    EnumWithPayload::I16(-1),
                    EnumWithPayload::I32(-2),
                    EnumWithPayload::I64(-3),
                    EnumWithPayload::I128(-4),
                    EnumWithPayload::Bool(true),
                    EnumWithPayload::F32(0.0),
                    EnumWithPayload::F64(100.0),
                    EnumWithPayload::Str("enum holds string".to_string()),
                    EnumWithPayload::Identity(identity().unwrap()),
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

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

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
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        let mut large_table_result = Some(test_counter.add_test("insert-large-table"));
        once_on_subscription_applied(move || {
            let every_primitive_struct = EveryPrimitiveStruct {
                a: 0,
                b: 1,
                c: 2,
                d: 3,
                e: 4,
                f: 0,
                g: -1,
                h: -2,
                i: -3,
                j: -4,
                k: false,
                l: 0.0,
                m: 1.0,
                n: "string".to_string(),
                o: identity().unwrap(),
                p: address().unwrap(),
            };
            let every_vec_struct = EveryVecStruct {
                a: vec![0],
                b: vec![1],
                c: vec![2],
                d: vec![3],
                e: vec![4],
                f: vec![0],
                g: vec![-1],
                h: vec![-2],
                i: vec![-3],
                j: vec![-4],
                k: vec![false],
                l: vec![0.0],
                m: vec![1.0],
                n: vec!["string".to_string()],
                o: vec![identity().unwrap()],
                p: vec![address().unwrap()],
            };

            let every_primitive_dup = every_primitive_struct.clone();
            let every_vec_dup = every_vec_struct.clone();
            LargeTable::on_insert(move |row, reducer_event| {
                if large_table_result.is_some() {
                    let run_tests = || {
                        assert_eq_or_bail!(row.a, 0);
                        assert_eq_or_bail!(row.b, 1);
                        assert_eq_or_bail!(row.c, 2);
                        assert_eq_or_bail!(row.d, 3);
                        assert_eq_or_bail!(row.e, 4);
                        assert_eq_or_bail!(row.f, 0);
                        assert_eq_or_bail!(row.g, -1);
                        assert_eq_or_bail!(row.h, -2);
                        assert_eq_or_bail!(row.i, -3);
                        assert_eq_or_bail!(row.j, -4);
                        assert_eq_or_bail!(row.k, false);
                        assert_eq_or_bail!(row.l, 0.0);
                        assert_eq_or_bail!(row.m, 1.0);
                        assert_eq_or_bail!(&row.n, "string");
                        assert_eq_or_bail!(row.o, SimpleEnum::Zero);
                        assert_eq_or_bail!(row.p, EnumWithPayload::Bool(false));
                        assert_eq_or_bail!(row.q, UnitStruct {});
                        assert_eq_or_bail!(row.r, ByteStruct { b: 0b10101010 });
                        assert_eq_or_bail!(row.s, every_primitive_struct);
                        assert_eq_or_bail!(row.t, every_vec_struct);
                        if !matches!(reducer_event, Some(ReducerEvent::InsertLargeTable(_))) {
                            anyhow::bail!(
                                "Unexpected reducer event: expeced InsertLargeTable but found {:?}",
                                reducer_event
                            );
                        }
                        Ok(())
                    };
                    (large_table_result.take().unwrap())(run_tests());
                }
            });
            insert_large_table(
                0,
                1,
                2,
                3,
                4,
                0,
                -1,
                -2,
                -3,
                -4,
                false,
                0.0,
                1.0,
                "string".to_string(),
                SimpleEnum::Zero,
                EnumWithPayload::Bool(false),
                UnitStruct {},
                ByteStruct { b: 0b10101010 },
                every_primitive_dup,
                every_vec_dup,
            );

            sub_applied_nothing_result(assert_all_tables_empty())
        });
    }

    once_on_connect(move |_, _| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

/// This tests the behavior of re-subscribing
/// by observing `on_delete` callbacks of newly-unsubscribed rows
/// and `on_insert` callbacks of newly-subscribed rows.
fn exec_resubscribe() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    // Boring stuff first: connect and subscribe to everything.
    let connect_result = test_counter.add_test("connect");
    let subscribe_result = test_counter.add_test("initial-subscribe");
    let sub_applied_result = test_counter.add_test("initial-subscription-nothing");

    once_on_subscription_applied(move || {
        sub_applied_result(assert_all_tables_empty());
    });

    once_on_connect(|_, _| {
        subscribe_result(subscribe(SUBSCRIBE_ALL));
    });

    connect_result(connect(LOCALHOST, &name, None));

    // Wait for all previous checks before continuing.
    test_counter.wait_for_all();

    // Insert 256 rows of `OneU8`.
    // At this point, we should be subscribed to all of them.
    let test_counter = TestCounter::new();
    let mut insert_u8s = (0..=255)
        .map(|n| Some(test_counter.add_test(format!("insert-{}", n))))
        .collect::<Vec<_>>();
    let on_insert_u8 = OneU8::on_insert(move |row, _| {
        let n = row.n;
        (insert_u8s[n as usize].take().unwrap())(Ok(()));
    });
    for n in 0..=255 {
        insert_one_u_8(n as u8);
    }
    // Wait for all previous checks before continuing,
    test_counter.wait_for_all();
    // and remove the callback now that we're done with it.
    OneU8::remove_on_insert(on_insert_u8);

    // Re-subscribe with a query that excludes the lower half of the `OneU8` rows,
    // and observe `on_delete` callbacks for those rows.
    let test_counter = TestCounter::new();
    let mut delete_u8s = (0..128)
        .map(|n| Some(test_counter.add_test(format!("unsubscribe-{}-delete", n))))
        .collect::<Vec<_>>();
    let on_delete_verify = OneU8::on_delete(move |row, _| {
        let n = row.n;
        // This indexing will panic if n > 127.
        (delete_u8s[n as usize].take().unwrap())(Ok(()));
    });
    // There should be no newly-subscribed rows, so we'll panic if we get an on-insert event.
    let on_insert_panic = OneU8::on_insert(|row, _| {
        panic!("Unexpected insert during re-subscribe for {:?}", row);
    });
    let subscribe_less_result = test_counter.add_test("resubscribe-fewer-matches");
    once_on_subscription_applied(move || {
        let run_checks = || {
            assert_eq_or_bail!(128, OneU8::count());
            if let Some(row) = OneU8::iter().find(|row| row.n < 128) {
                anyhow::bail!("After subscribing to OneU8 WHERE n > 127, found row with n < {}", row.n);
            }
            Ok(())
        };
        subscribe_less_result(run_checks());
    });
    let subscribe_result = test_counter.add_test("resubscribe");
    subscribe_result(subscribe(&["SELECT * FROM OneU8 WHERE n > 127"]));
    // Wait before continuing, and remove callbacks.
    test_counter.wait_for_all();
    OneU8::remove_on_delete(on_delete_verify);
    OneU8::remove_on_insert(on_insert_panic);

    // Re-subscribe with a query that includes all of the `OneU8` rows again,
    // and observe `on_insert` callbacks for the lower half.
    let test_counter = TestCounter::new();
    let mut insert_u8s = (0..128)
        .map(|n| Some(test_counter.add_test(format!("resubscribe-{}-insert", n))))
        .collect::<Vec<_>>();
    OneU8::on_insert(move |row, _| {
        let n = row.n;
        // This indexing will panic if n > 127.
        (insert_u8s[n as usize].take().unwrap())(Ok(()));
    });
    // There should be no newly-unsubscribed rows, so we'll panic if we get an on-delete event.
    OneU8::on_delete(|row, _| {
        panic!("Unexpected delete during re-subscribe for {:?}", row);
    });
    let subscribe_more_result = test_counter.add_test("resubscribe-more-matches");
    once_on_subscription_applied(move || {
        let run_checks = || {
            assert_eq_or_bail!(256, OneU8::count());
            Ok(())
        };
        subscribe_more_result(run_checks());
    });
    let subscribe_result = test_counter.add_test("resubscribe-again");
    subscribe_result(subscribe(&["SELECT * FROM OneU8"]));
    test_counter.wait_for_all();
}

/// Once we determine appropriate semantics for in-process re-connecting,
/// this test will verify it.
fn exec_reconnect() {
    todo!()
}

/// Part of the `reauth` test, this connects to Spacetime to get new credentials,
/// and saves them to a file.
fn exec_reauth_part_1() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let connect_result = test_counter.add_test("connect");
    let save_result = test_counter.add_test("save-credentials");

    once_on_connect(|creds, _| {
        save_result(save_credentials(".spacetime_rust_sdk_test", creds));
    });

    connect_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

/// Part of the `reauth` test, this loads credentials from a file,
/// and passes them to `connect`.
///
/// Must run after `exec_reauth_part_1`.
fn exec_reauth_part_2() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let connect_result = test_counter.add_test("connect");
    let creds_match_result = test_counter.add_test("credentials-match");

    let creds = load_credentials(".spacetime_rust_sdk_test")
        .expect("Failed to load credentials")
        .expect("Expected credentials but found none");

    let creds_dup = creds.clone();

    once_on_connect(move |received_creds, _| {
        let run_checks = || {
            assert_eq_or_bail!(creds_dup, *received_creds);
            Ok(())
        };

        creds_match_result(run_checks());
    });

    connect_result(connect(LOCALHOST, &name, Some(creds)));

    test_counter.wait_for_all();
}

fn exec_reconnect_same_address() {
    let test_counter = TestCounter::new();
    let name = db_name_or_panic();

    let connect_result = test_counter.add_test("connect");
    let read_addr_result = test_counter.add_test("read_addr");

    let name_dup = name.clone();
    once_on_connect(move |_, received_address| {
        let my_address = address().unwrap();
        let run_checks = || {
            assert_eq_or_bail!(my_address, received_address);
            Ok(())
        };

        read_addr_result(run_checks());
    });

    connect_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();

    let my_address = address().unwrap();

    let test_counter = TestCounter::new();
    let reconnect_result = test_counter.add_test("reconnect");
    let addr_after_reconnect_result = test_counter.add_test("addr_after_reconnect");

    once_on_disconnect(move || {
        once_on_connect(move |_, received_address| {
            let my_address_2 = address().unwrap();
            let run_checks = || {
                assert_eq_or_bail!(my_address, received_address);
                assert_eq_or_bail!(my_address, my_address_2);
                Ok(())
            };

            addr_after_reconnect_result(run_checks());
        });

        reconnect_result(connect(LOCALHOST, &name_dup, None));
    });

    disconnect();

    test_counter.wait_for_all();
}
