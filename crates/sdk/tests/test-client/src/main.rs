use anyhow::anyhow;
use spacetimedb_sdk::{
    identity::{identity, once_on_connect, Identity},
    once_on_subscription_applied, subscribe,
    table::{TableType, TableWithPrimaryKey},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

#[allow(clippy::too_many_arguments)]
#[allow(clippy::large_enum_variant)]
#[rustfmt::skip]
mod module_bindings;

use module_bindings::*;

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

#[derive(Default)]
struct TestCounterInner {
    /// Maps test names to their outcomes
    outcomes: HashMap<String, anyhow::Result<()>>,
    /// Set of tests which have started.
    registered: HashSet<String>,
}

struct TestCounter {
    inner: Mutex<TestCounterInner>,
    wait_until_done: Condvar,
}

impl Default for TestCounter {
    fn default() -> Self {
        TestCounter {
            inner: Mutex::new(TestCounterInner::default()),
            wait_until_done: Condvar::new(),
        }
    }
}

impl TestCounter {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn add_test(
        self: &Arc<Self>,
        test_name: impl Into<String> + Clone + std::fmt::Display + Send + 'static,
    ) -> Box<dyn FnOnce(anyhow::Result<()>) + Send + 'static> {
        {
            let mut lock = self.inner.lock().expect("TestCounterInner Mutex is poisoned");
            if !lock.registered.insert(test_name.clone().into()) {
                panic!("Duplicate test name: {}", test_name);
            }
        }
        let dup = Arc::clone(self);

        Box::new(move |outcome| {
            let mut lock = dup.inner.lock().expect("TestCounterInner Mutex is poisoned");
            lock.outcomes.insert(test_name.into(), outcome);
            dup.wait_until_done.notify_all();
        })
    }

    fn wait_for_all(&self) {
        let lock = self.inner.lock().expect("TestCounterInner Mutex is poisoned");
        let (lock, timeout_result) = self
            .wait_until_done
            .wait_timeout_while(lock, Duration::from_secs(5), |inner| {
                inner.outcomes.len() == inner.registered.len()
            })
            .expect("TestCounterInner Mutex is poisoned");
        if timeout_result.timed_out() {
            let mut timeout_count = 0;
            let mut failed_count = 0;
            for test in lock.registered.iter() {
                match lock.outcomes.get(test) {
                    None => {
                        timeout_count += 1;
                        println!("TIMEOUT: {}", test);
                    }
                    Some(Err(e)) => {
                        failed_count += 1;
                        println!("FAILED:  {}:\n\t{:?}\n", test, e);
                    }
                    Some(Ok(())) => {
                        println!("PASSED:  {}", test);
                    }
                }
            }
            panic!("{} tests timed out and {} tests failed", timeout_count, failed_count)
        } else {
            let mut failed_count = 0;
            for (test, outcome) in lock.outcomes.iter() {
                match outcome {
                    Ok(()) => println!("PASSED: {}", test),
                    Err(e) => {
                        failed_count += 1;
                        println!("FAILED: {}:\n\t{:?}\n", test, e);
                    }
                }
            }
            if failed_count != 0 {
                panic!("{} tests failed", failed_count);
            } else {
                println!("All tests passed");
            }
        }
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

trait SimpleTestTable: TableType {
    type Contents: Clone + Send + Sync + PartialEq + std::fmt::Debug + 'static;

    fn as_contents(&self) -> &Self::Contents;
    fn from_contents(contents: Self::Contents) -> Self;

    fn is_insert_reducer_event(event: &Self::ReducerEvent) -> bool;

    fn insert(contents: Self::Contents);
}

macro_rules! impl_simple_test_table {
    ($table:ty {
        Contents = $contents:ty;
        field_name = $field_name:ident;
        insert_reducer = $insert_reducer:ident;
        insert_reducer_event = $insert_reducer_event:ident;
    }) => {
        impl SimpleTestTable for $table {
            type Contents = $contents;

            fn as_contents(&self) -> &Self::Contents {
                &self.$field_name
            }

            fn from_contents(contents: Self::Contents) -> Self {
                Self {
                    $field_name: contents,
                }
            }

            fn is_insert_reducer_event(event: &Self::ReducerEvent) -> bool {
                matches!(event, ReducerEvent::$insert_reducer_event(_))
            }

            fn insert(contents: Self::Contents) {
                $insert_reducer(contents);
            }
        }
    };
    ($($table:ty { $($stuff:tt)* })*) => {
        $(impl_simple_test_table!($table { $($stuff)* });)*
    };
}

impl_simple_test_table! {
    OneU8 {
        Contents = u8;
        field_name = n;
        insert_reducer = insert_one_u_8;
        insert_reducer_event = InsertOneU8;
    }
    OneU16 {
        Contents = u16;
        field_name = n;
        insert_reducer = insert_one_u_16;
        insert_reducer_event = InsertOneU16;
    }
    OneU32 {
        Contents = u32;
        field_name = n;
        insert_reducer = insert_one_u_32;
        insert_reducer_event = InsertOneU32;
    }
    OneU64 {
        Contents = u64;
        field_name = n;
        insert_reducer = insert_one_u_64;
        insert_reducer_event = InsertOneU64;
    }
    OneU128 {
        Contents = u128;
        field_name = n;
        insert_reducer = insert_one_u_128;
        insert_reducer_event = InsertOneU128;
    }

    OneI8 {
        Contents = i8;
        field_name = n;
        insert_reducer = insert_one_i_8;
        insert_reducer_event = InsertOneI8;
    }
    OneI16 {
        Contents = i16;
        field_name = n;
        insert_reducer = insert_one_i_16;
        insert_reducer_event = InsertOneI16;
    }
    OneI32 {
        Contents = i32;
        field_name = n;
        insert_reducer = insert_one_i_32;
        insert_reducer_event = InsertOneI32;
    }
    OneI64 {
        Contents = i64;
        field_name = n;
        insert_reducer = insert_one_i_64;
        insert_reducer_event = InsertOneI64;
    }
    OneI128 {
        Contents = i128;
        field_name = n;
        insert_reducer = insert_one_i_128;
        insert_reducer_event = InsertOneI128;
    }

    OneF32 {
        Contents = f32;
        field_name = f;
        insert_reducer = insert_one_f_32;
        insert_reducer_event = InsertOneF32;
    }
    OneF64 {
        Contents = f64;
        field_name = f;
        insert_reducer = insert_one_f_64;
        insert_reducer_event = InsertOneF64;
    }

    OneBool {
        Contents = bool;
        field_name = b;
        insert_reducer = insert_one_bool;
        insert_reducer_event = InsertOneBool;
    }

    OneString {
        Contents = String;
        field_name = s;
        insert_reducer = insert_one_string;
        insert_reducer_event = InsertOneString;
    }

    OneIdentity {
        Contents = Identity;
        field_name = i;
        insert_reducer = insert_one_identity;
        insert_reducer_event = InsertOneIdentity;
    }

    OneSimpleEnum {
        Contents = SimpleEnum;
        field_name = e;
        insert_reducer = insert_one_simple_enum;
        insert_reducer_event = InsertOneSimpleEnum;
    }
    OneEnumWithPayload {
        Contents = EnumWithPayload;
        field_name = e;
        insert_reducer = insert_one_enum_with_payload;
        insert_reducer_event = InsertOneEnumWithPayload;
    }

    OneUnitStruct {
        Contents = UnitStruct;
        field_name = s;
        insert_reducer = insert_one_unit_struct;
        insert_reducer_event = InsertOneUnitStruct;
    }
    OneByteStruct {
        Contents = ByteStruct;
        field_name = s;
        insert_reducer = insert_one_byte_struct;
        insert_reducer_event = InsertOneByteStruct;
    }
    OneEveryPrimitiveStruct {
        Contents = EveryPrimitiveStruct;
        field_name = s;
        insert_reducer = insert_one_every_primitive_struct;
        insert_reducer_event = InsertOneEveryPrimitiveStruct;
    }
    OneEveryVecStruct {
        Contents = EveryVecStruct;
        field_name = s;
        insert_reducer = insert_one_every_vec_struct;
        insert_reducer_event = InsertOneEveryVecStruct;
    }
}

fn insert_one<T: SimpleTestTable>(test_counter: &Arc<TestCounter>, value: T::Contents) {
    let mut result = Some(test_counter.add_test(format!("insert-{}", T::TABLE_NAME)));
    let value_dup = value.clone();
    T::on_insert(move |row, reducer_event| {
        if result.is_some() {
            let run_checks = || {
                if row.as_contents() != &value_dup {
                    anyhow::bail!("Unexpected row value. Expected {:?} but found {:?}", value_dup, row);
                }
                reducer_event
                    .ok_or(anyhow!("Expected a reducer event, but found None."))
                    .map(T::is_insert_reducer_event)
                    .and_then(|is_good| is_good.then_some(()).ok_or(anyhow!("Unexpected ReducerEvent variant.")))?;

                Ok(())
            };
            (result.take().unwrap())(run_checks());
        }
    });

    T::insert(value);
}

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

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

trait UniqueTestTable: TableType {
    type Key: Clone + Send + Sync + PartialEq + std::fmt::Debug + 'static;

    fn as_key(&self) -> &Self::Key;
    fn as_value(&self) -> i32;

    fn from_key_value(k: Self::Key, v: i32) -> Self;

    fn is_insert_reducer_event(event: &Self::ReducerEvent) -> bool;
    fn is_delete_reducer_event(event: &Self::ReducerEvent) -> bool;

    fn insert(k: Self::Key, v: i32);
    fn delete(k: Self::Key);
}

fn insert_then_delete_one<T: UniqueTestTable>(test_counter: &Arc<TestCounter>, key: T::Key, value: i32) {
    let mut insert_result = Some(test_counter.add_test(format!("insert-{}", T::TABLE_NAME)));
    let mut delete_result = Some(test_counter.add_test(format!("delete-{}", T::TABLE_NAME)));

    let mut on_delete = {
        let key_dup = key.clone();
        Some(move |row: &T, reducer_event: Option<&T::ReducerEvent>| {
            if delete_result.is_some() {
                let run_checks = || {
                    if row.as_key() != &key_dup || row.as_value() != value {
                        anyhow::bail!(
                            "Unexpected row value. Expected ({:?}, {}) but found {:?}",
                            key_dup,
                            value,
                            row
                        );
                    }
                    reducer_event
                        .ok_or(anyhow!("Expected a reducer event, but found None."))
                        .map(T::is_delete_reducer_event)
                        .and_then(|is_good| is_good.then_some(()).ok_or(anyhow!("Unexpected ReducerEvent variant.")))?;
                    Ok(())
                };

                (delete_result.take().unwrap())(run_checks());
            }
        })
    };

    let key_dup = key.clone();

    T::on_insert(move |row, reducer_event| {
        if insert_result.is_some() {
            let run_checks = || {
                if row.as_key() != &key_dup || row.as_value() != value {
                    anyhow::bail!(
                        "Unexpected row value. Expected ({:?}, {}) but found {:?}",
                        key_dup,
                        value,
                        row
                    );
                }
                reducer_event
                    .ok_or(anyhow!("Expected a reducer event, but found None."))
                    .map(T::is_insert_reducer_event)
                    .and_then(|is_good| is_good.then_some(()).ok_or(anyhow!("Unexpected ReducerEvent variant.")))?;

                Ok(())
            };

            (insert_result.take().unwrap())(run_checks());

            T::on_delete(on_delete.take().unwrap());

            T::delete(key_dup.clone());
        }
    });

    T::insert(key, value);
}

macro_rules! impl_unique_test_table {
    ($table:ty {
        Key = $key:ty;
        key_field_name = $field_name:ident;
        insert_reducer = $insert_reducer:ident;
        insert_reducer_event = $insert_reducer_event:ident;
        delete_reducer = $delete_reducer:ident;
        delete_reducer_event = $delete_reducer_event:ident;
    }) => {
        impl UniqueTestTable for $table {
            type Key = $key;

            fn as_key(&self) -> &Self::Key {
                &self.$field_name
            }
            fn as_value(&self) -> i32 {
                self.data
            }

            fn from_key_value(key: Self::Key, value: i32) -> Self {
                Self {
                    $field_name: key,
                    data: value,
                }
            }

            fn is_insert_reducer_event(event: &Self::ReducerEvent) -> bool {
                matches!(event, ReducerEvent::$insert_reducer_event(_))
            }
            fn is_delete_reducer_event(event: &Self::ReducerEvent) -> bool {
                matches!(event, ReducerEvent::$delete_reducer_event(_))
            }

            fn insert(key: Self::Key, value: i32) {
                $insert_reducer(key, value);
            }
            fn delete(key: Self::Key) {
                $delete_reducer(key);
            }
        }
    };
    ($($table:ty { $($stuff:tt)* })*) => {
        $(impl_unique_test_table!($table { $($stuff)* });)*
    };
}

impl_unique_test_table! {
    UniqueU8 {
        Key = u8;
        key_field_name = n;
        insert_reducer = insert_unique_u_8;
        insert_reducer_event = InsertUniqueU8;
        delete_reducer = delete_unique_u_8;
        delete_reducer_event = DeleteUniqueU8;
    }
    UniqueU16 {
        Key = u16;
        key_field_name = n;
        insert_reducer = insert_unique_u_16;
        insert_reducer_event = InsertUniqueU16;
        delete_reducer = delete_unique_u_16;
        delete_reducer_event = DeleteUniqueU16;
    }
    UniqueU32 {
        Key = u32;
        key_field_name = n;
        insert_reducer = insert_unique_u_32;
        insert_reducer_event = InsertUniqueU32;
        delete_reducer = delete_unique_u_32;
        delete_reducer_event = DeleteUniqueU32;
    }
    UniqueU64 {
        Key = u64;
        key_field_name = n;
        insert_reducer = insert_unique_u_64;
        insert_reducer_event = InsertUniqueU64;
        delete_reducer = delete_unique_u_64;
        delete_reducer_event = DeleteUniqueU64;
    }
    UniqueU128 {
        Key = u128;
        key_field_name = n;
        insert_reducer = insert_unique_u_128;
        insert_reducer_event = InsertUniqueU128;
        delete_reducer = delete_unique_u_128;
        delete_reducer_event = DeleteUniqueU128;
    }

    UniqueI8 {
        Key = i8;
        key_field_name = n;
        insert_reducer = insert_unique_i_8;
        insert_reducer_event = InsertUniqueI8;
        delete_reducer = delete_unique_i_8;
        delete_reducer_event = DeleteUniqueI8;
    }
    UniqueI16 {
        Key = i16;
        key_field_name = n;
        insert_reducer = insert_unique_i_16;
        insert_reducer_event = InsertUniqueI16;
        delete_reducer = delete_unique_i_16;
        delete_reducer_event = DeleteUniqueI16;
    }
    UniqueI32 {
        Key = i32;
        key_field_name = n;
        insert_reducer = insert_unique_i_32;
        insert_reducer_event = InsertUniqueI32;
        delete_reducer = delete_unique_i_32;
        delete_reducer_event = DeleteUniqueI32;
    }
    UniqueI64 {
        Key = i64;
        key_field_name = n;
        insert_reducer = insert_unique_i_64;
        insert_reducer_event = InsertUniqueI64;
        delete_reducer = delete_unique_i_64;
        delete_reducer_event = DeleteUniqueI64;
    }
    UniqueI128 {
        Key = i128;
        key_field_name = n;
        insert_reducer = insert_unique_i_128;
        insert_reducer_event = InsertUniqueI128;
        delete_reducer = delete_unique_i_128;
        delete_reducer_event = DeleteUniqueI128;
    }

    UniqueBool {
        Key = bool;
        key_field_name = b;
        insert_reducer = insert_unique_bool;
        insert_reducer_event = InsertUniqueBool;
        delete_reducer = delete_unique_bool;
        delete_reducer_event = DeleteUniqueBool;
    }

    UniqueString {
        Key = String;
        key_field_name = s;
        insert_reducer = insert_unique_string;
        insert_reducer_event = InsertUniqueString;
        delete_reducer = delete_unique_string;
        delete_reducer_event = DeleteUniqueString;
    }

    UniqueIdentity {
        Key = Identity;
        key_field_name = i;
        insert_reducer = insert_unique_identity;
        insert_reducer_event = InsertUniqueIdentity;
        delete_reducer = delete_unique_identity;
        delete_reducer_event = DeleteUniqueIdentity;
    }
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

            sub_applied_nothing_result(assert_all_tables_empty());
        });
    }

    once_on_connect(move |_| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

fn exec_update_primitive() {
    todo!()
}
fn exec_insert_identity() {
    todo!()
}
fn exec_delete_identity() {
    todo!()
}
fn exec_update_identity() {
    todo!()
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
