use spacetimedb_sdk::{
    identity::{identity, once_on_connect},
    once_on_subscription_applied, subscribe,
    table::{TableType, TableWithPrimaryKey},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

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

macro_rules! insert_one {
    ($test_counter:ident; $table:ty; $field_name:ident; $val:expr; $insert_reducer:ident; $insert_reducer_event:ident $(;)?) => {{
        let mut result = Some($test_counter.add_test(concat!("insert-", stringify!($table))));
        let val = $val;
        let val_dup = val.clone();
        <$table as TableType>::on_insert(move |row, reducer_event| {
            (result.take().unwrap())((|| {
                if row.$field_name != val_dup {
                    anyhow::bail!("Unexpected row value. Expected {:?} but found {:?}", val_dup, row);
                }
                if !matches!(reducer_event, Some(ReducerEvent::$insert_reducer_event(_))) {
                    anyhow::bail!(
                        "Unexpected ReducerEvent variant. Expected {} but found {:?}",
                        stringify!($insert_reducer_event),
                        reducer_event
                    );
                }
                Ok(())
            })());
        });

        $insert_reducer($val);
    }};
}

fn exec_insert_primitive() {
    let test_counter = Arc::new(TestCounter::default());
    let name = db_name_or_panic();

    let conn_result = test_counter.add_test("connect");

    let sub_result = test_counter.add_test("subscribe");

    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    {
        let test_counter = test_counter.clone();
        once_on_subscription_applied(move || {
            sub_applied_nothing_result(assert_all_tables_empty());

            insert_one!(test_counter; OneU8; n; 0; insert_one_u_8; InsertOneU8);
            insert_one!(test_counter; OneU16; n; 0; insert_one_u_16; InsertOneU16);
            insert_one!(test_counter; OneU32; n; 0; insert_one_u_32; InsertOneU32);
            insert_one!(test_counter; OneU64; n; 0; insert_one_u_64; InsertOneU64);
            insert_one!(test_counter; OneU128; n; 0; insert_one_u_128; InsertOneU128);

            insert_one!(test_counter; OneI8; n; 0; insert_one_i_8; InsertOneI8);
            insert_one!(test_counter; OneI16; n; 0; insert_one_i_16; InsertOneI16);
            insert_one!(test_counter; OneI32; n; 0; insert_one_i_32; InsertOneI32);
            insert_one!(test_counter; OneI64; n; 0; insert_one_i_64; InsertOneI64);
            insert_one!(test_counter; OneI128; n; 0; insert_one_i_128; InsertOneI128);

            insert_one!(test_counter; OneBool; b; false; insert_one_bool; InsertOneBool);

            insert_one!(test_counter; OneF32; f; 0.0; insert_one_f_32; InsertOneF32);
            insert_one!(test_counter; OneF64; f; 0.0; insert_one_f_64; InsertOneF64);

            insert_one!(test_counter; OneString; s; "".to_string(); insert_one_string; InsertOneString);

            insert_one!(test_counter; OneIdentity; i; identity().unwrap(); insert_one_identity; InsertOneIdentity);
        });
    }

    once_on_connect(move |_| sub_result(subscribe(SUBSCRIBE_ALL)));

    conn_result(connect(LOCALHOST, &name, None));

    test_counter.wait_for_all();
}

fn exec_delete_primitive() {
    todo!()
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
