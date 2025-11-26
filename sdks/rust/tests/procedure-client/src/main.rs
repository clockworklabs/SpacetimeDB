mod module_bindings;

use core::time::Duration;

use anyhow::Context;
use module_bindings::*;
use spacetimedb_lib::db::raw_def::v9::{RawMiscModuleExportV9, RawModuleDefV9};
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
        "procedure-return-values" => exec_procedure_return_values(),
        "procedure-observe-panic" => exec_procedure_panic(),
        "procedure-http-ok" => exec_procedure_http_ok(),
        "procedure-http-err" => exec_procedure_http_err(),
        "insert-with-tx-commit" => exec_insert_with_tx_commit(),
        "insert-with-tx-rollback" => exec_insert_with_tx_rollback(),
        "schedule-procedure" => exec_schedule_procedure(),
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

/// Each subscribing test runs against a fresh DB,
/// so all tables should be empty until we call an insert reducer.
///
/// We'll call this function within our initial `on_subscription_applied` callback to verify that.
fn assert_all_tables_empty(ctx: &impl RemoteDbContext) -> anyhow::Result<()> {
    assert_table_empty(ctx.db().my_table())?;
    Ok(())
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

/// A query that subscribes to all rows from all tables.
const SUBSCRIBE_ALL: &[&str] = &["SELECT * FROM my_table;", "SELECT * FROM proc_inserts_into;"];

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

fn exec_procedure_return_values() {
    let test_counter = TestCounter::new();

    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            let return_primitive_result = test_counter.add_test("return_primitive");
            let return_struct_result = test_counter.add_test("return_struct");
            let return_enum_a_result = test_counter.add_test("return_enum_a");
            let return_enum_b_result = test_counter.add_test("return_enum_b");

            ctx.procedures.return_primitive_then(1, 2, move |_, res| {
                return_primitive_result(res.context("return_primtive failed unexpectedly").and_then(|sum| {
                    if sum == 3 {
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!(
                            "Expected return value from return_primitive of 3 but got {sum}"
                        ))
                    }
                }));
            });
            ctx.procedures
                .return_struct_then(1234, "foo".to_string(), move |_, res| {
                    return_struct_result(res.context("return_struct failed unexpectedly").and_then(|strukt| {
                        anyhow::ensure!(strukt.a == 1234);
                        anyhow::ensure!(&*strukt.b == "foo");
                        Ok(())
                    }));
                });
            ctx.procedures.return_enum_a_then(1234, move |_, res| {
                return_enum_a_result(res.context("return_enum_a failed unexpectedly").and_then(|enum_a| {
                    anyhow::ensure!(matches!(enum_a, ReturnEnum::A(1234)));
                    Ok(())
                }));
            });
            ctx.procedures.return_enum_b_then("foo".to_string(), move |_, res| {
                return_enum_b_result(res.context("return_enum_b failed unexpectedly").and_then(|enum_b| {
                    let ReturnEnum::B(string) = enum_b else {
                        anyhow::bail!("Unexpected variant for returned enum {enum_b:?}");
                    };
                    anyhow::ensure!(&*string == "foo");
                    Ok(())
                }));
            });
        }
    });

    test_counter.wait_for_all();
}

fn exec_procedure_panic() {
    let test_counter = TestCounter::new();

    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            let will_panic_result = test_counter.add_test("will_panic");

            ctx.procedures.will_panic_then(move |_, res| {
                will_panic_result(if res.is_err() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Expected failure but got Ok... huh? {res:?}"))
                });
            });
        }
    });

    test_counter.wait_for_all();
}

fn exec_insert_with_tx_commit() {
    fn expected() -> ReturnStruct {
        ReturnStruct {
            a: 42,
            b: "magic".into(),
        }
    }

    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");
    let inspect_result = test_counter.add_test("insert_with_tx_commit_values");
    let mut callback_result = Some(test_counter.add_test("insert_with_tx_commit_callback"));

    connect_then(&test_counter, {
        move |ctx| {
            ctx.db().my_table().on_insert(move |_, row| {
                assert_eq!(row.field, expected());
                (callback_result.take().unwrap())(Ok(()));
            });

            subscribe_all_then(ctx, move |ctx| {
                sub_applied_nothing_result(assert_all_tables_empty(ctx));

                ctx.procedures.insert_with_tx_commit_then(move |ctx, res| {
                    assert!(res.is_ok(), "Expected Ok result but got {res:?}");
                    let row = ctx.db().my_table().iter().next().unwrap();
                    assert_eq!(row.field, expected());
                    inspect_result(Ok(()));
                });
            });
        }
    });

    test_counter.wait_for_all();
}

fn exec_insert_with_tx_rollback() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");
    let inspect_result = test_counter.add_test("insert_with_tx_rollback_values");

    connect_then(&test_counter, {
        move |ctx| {
            ctx.db()
                .my_table()
                .on_insert(|_, _| unreachable!("should not have inserted a row"));

            subscribe_all_then(ctx, move |ctx| {
                sub_applied_nothing_result(assert_all_tables_empty(ctx));

                ctx.procedures.insert_with_tx_rollback_then(move |ctx, res| {
                    assert!(res.is_ok(), "Expected Ok result but got {res:?}");
                    assert_eq!(ctx.db().my_table().iter().next(), None);
                    inspect_result(Ok(()));
                });
            });
        }
    });

    test_counter.wait_for_all();
}

/// Test that a procedure can perform an HTTP request and return a string derived from the response.
///
/// Invoke the procedure `read_my_schema`,
/// which does an HTTP request to the `/database/schema` route and returns a JSON-ified [`RawModuleDefV9`],
/// then (in the client) deserialize the response and assert that it contains a description of that procedure.
fn exec_procedure_http_ok() {
    let test_counter = TestCounter::new();
    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            let result = test_counter.add_test("invoke_http");
            ctx.procedures.read_my_schema_then(move |_ctx, res| {
                result(
                    // It's a try block!
                    #[allow(clippy::redundant_closure_call)]
                    (|| {
                        anyhow::ensure!(res.is_ok(), "Expected Ok result but got {res:?}");
                        let module_def: RawModuleDefV9 = spacetimedb_lib::de::serde::deserialize_from(
                            &mut serde_json::Deserializer::from_str(&res.unwrap()),
                        )?;
                        anyhow::ensure!(module_def.misc_exports.iter().any(|misc_export| {
                            if let RawMiscModuleExportV9::Procedure(procedure_def) = misc_export {
                                &*procedure_def.name == "read_my_schema"
                            } else {
                                false
                            }
                        }));
                        Ok(())
                    })(),
                )
            })
        }
    });
    test_counter.wait_for_all();
}

/// Test that a procedure can perform an HTTP request, handle its failure and return a string derived from the error.
///
/// Invoke the procedure `invalid_request`,
/// which does an HTTP request to a reserved invalid URL and returns a string-ified error,
/// then (in the client) assert that the error message looks sane.
fn exec_procedure_http_err() {
    let test_counter = TestCounter::new();
    connect_then(&test_counter, {
        let test_counter = test_counter.clone();
        move |ctx| {
            let result = test_counter.add_test("invoke_http");
            ctx.procedures.invalid_request_then(move |_ctx, res| {
                result(
                    // It's a try block!
                    #[allow(clippy::redundant_closure_call)]
                    (|| {
                        anyhow::ensure!(res.is_ok(), "Expected Ok result but got {res:?}");
                        let error = res.unwrap();
                        anyhow::ensure!(error.contains("error"));
                        anyhow::ensure!(error.contains("http://foo.invalid/"));
                        Ok(())
                    })(),
                )
            })
        }
    });

    test_counter.wait_for_all();
}

fn exec_schedule_procedure() {
    let test_counter = TestCounter::new();
    let sub_applied_nothing_result = test_counter.add_test("on_subscription_applied_nothing");

    let mut callback_result = Some(test_counter.add_test("insert_with_tx_commit_callback"));

    connect_then(&test_counter, {
        move |ctx| {
            ctx.db().proc_inserts_into().on_insert(move |_, row| {
                assert_eq!(row.x, 42);
                assert_eq!(row.y, 24);

                // Ensure that the elapsed time
                // between the reducer and procedure
                // is at least 1 second
                // but no more than 2 seconds.
                let elapsed = row
                    .procedure_ts
                    .duration_since(row.reducer_ts)
                    .expect("procedure ts > reducer ts");
                const MS_1000: Duration = Duration::from_secs(1);
                const MS_2000: Duration = Duration::from_secs(2);
                assert!(elapsed >= MS_1000);
                assert!(elapsed <= MS_2000);

                (callback_result.take().unwrap())(Ok(()));
            });

            subscribe_all_then(ctx, move |ctx| {
                sub_applied_nothing_result(assert_all_tables_empty(ctx));
                ctx.reducers.schedule_proc().unwrap();
            });
        }
    });

    test_counter.wait_for_all();
}
