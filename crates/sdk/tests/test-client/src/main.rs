use spacetimedb_sdk::{
    identity::{identity, once_on_connect},
    once_on_subscription_applied, subscribe,
    table::TableType,
};

#[allow(clippy::too_many_arguments)]
#[allow(clippy::large_enum_variant)]
#[rustfmt::skip]
mod module_bindings;

use module_bindings::*;

mod test_counter;
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

        "on_reducer" => exec_on_reducer(),
        "fail_reducer" => exec_fail_reducer(),

        "insert_vec" => exec_insert_vec(),

        "insert_struct" => exec_insert_struct(),
        "insert_simple_enum" => exec_insert_simple_enum(),
        "insert_enum_with_payload" => exec_insert_enum_with_payload(),

        "insert_long_table" => exec_insert_long_table(),

        "resubscribe" => exec_resubscribe(),

        "reconnect" => exec_reconnect(),
        _ => panic!("Unknown test: {}", test),
    }
}

macro_rules! assert_table_empty {
    ($table:ty) => {{
        let count = <$table as TableType>::count();
        if count != 0 {
            anyhow::bail!(
                "Expected table {} to be empty, but found {} rows resident",
                <$table as TableType>::TABLE_NAME,
                count,
            );
        }
    }};
}

fn assert_all_tables_empty() -> anyhow::Result<()> {
    assert_table_empty!(OneU8);
    assert_table_empty!(OneU16);
    assert_table_empty!(OneU32);
    assert_table_empty!(OneU64);
    assert_table_empty!(OneU128);

    assert_table_empty!(OneI8);
    assert_table_empty!(OneI16);
    assert_table_empty!(OneI32);
    assert_table_empty!(OneI64);
    assert_table_empty!(OneI128);

    assert_table_empty!(OneBool);

    assert_table_empty!(OneF32);
    assert_table_empty!(OneF64);

    assert_table_empty!(OneString);
    assert_table_empty!(OneIdentity);

    assert_table_empty!(OneSimpleEnum);
    assert_table_empty!(OneEnumWithPayload);

    assert_table_empty!(OneUnitStruct);
    assert_table_empty!(OneByteStruct);
    assert_table_empty!(OneEveryPrimitiveStruct);
    assert_table_empty!(OneEveryVecStruct);

    assert_table_empty!(VecU8);
    assert_table_empty!(VecU16);
    assert_table_empty!(VecU32);
    assert_table_empty!(VecU64);
    assert_table_empty!(VecU128);

    assert_table_empty!(VecI8);
    assert_table_empty!(VecI16);
    assert_table_empty!(VecI32);
    assert_table_empty!(VecI64);
    assert_table_empty!(VecI128);

    assert_table_empty!(VecBool);

    assert_table_empty!(VecF32);
    assert_table_empty!(VecF64);

    assert_table_empty!(VecString);
    assert_table_empty!(VecIdentity);

    assert_table_empty!(VecSimpleEnum);
    assert_table_empty!(VecEnumWithPayload);

    assert_table_empty!(VecUnitStruct);
    assert_table_empty!(VecByteStruct);
    assert_table_empty!(VecEveryPrimitiveStruct);
    assert_table_empty!(VecEveryVecStruct);

    assert_table_empty!(UniqueU8);
    assert_table_empty!(UniqueU16);
    assert_table_empty!(UniqueU32);
    assert_table_empty!(UniqueU64);
    assert_table_empty!(UniqueU128);

    assert_table_empty!(UniqueI8);
    assert_table_empty!(UniqueI16);
    assert_table_empty!(UniqueI32);
    assert_table_empty!(UniqueI64);
    assert_table_empty!(UniqueI128);

    assert_table_empty!(UniqueBool);

    assert_table_empty!(UniqueString);
    assert_table_empty!(UniqueIdentity);

    assert_table_empty!(PkU8);
    assert_table_empty!(PkU16);
    assert_table_empty!(PkU32);
    assert_table_empty!(PkU64);
    assert_table_empty!(PkU128);

    assert_table_empty!(PkI8);
    assert_table_empty!(PkI16);
    assert_table_empty!(PkI32);
    assert_table_empty!(PkI64);
    assert_table_empty!(PkI128);

    assert_table_empty!(PkBool);

    assert_table_empty!(PkString);
    assert_table_empty!(PkIdentity);

    assert_table_empty!(LargeTable);

    assert_table_empty!(TableHoldsTable);

    Ok(())
}

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
    "SELECT * FROM LargeTable;",
    "SELECT * FROM TableHoldsTable;",
];

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

    once_on_connect(move |_| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

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

    once_on_connect(move |_| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();

    assert_all_tables_empty().unwrap();
}

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

    once_on_connect(move |_| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();

    assert_all_tables_empty().unwrap();
}
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

    once_on_connect(move |_| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

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

    once_on_connect(move |_| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();

    assert_all_tables_empty().unwrap();
}

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

    once_on_connect(move |_| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();

    assert_all_tables_empty().unwrap();
}

fn exec_on_reducer() {
    todo!()
}
fn exec_fail_reducer() {
    todo!()
}
fn exec_insert_vec() {
    todo!()
}
fn exec_insert_struct() {
    todo!()
}
fn exec_insert_simple_enum() {
    todo!()
}
fn exec_insert_enum_with_payload() {
    todo!()
}
fn exec_insert_long_table() {
    todo!()
}
fn exec_resubscribe() {
    todo!()
}
fn exec_reconnect() {
    todo!()
}
