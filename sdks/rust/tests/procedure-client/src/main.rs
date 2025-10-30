mod module_bindings;

use module_bindings::*;

use spacetimedb_sdk::DbConnectionBuilder;
use test_counter::TestCounter;
use anyhow::Context;

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
            ctx.procedures.return_struct_then(1234, "foo".to_string(), move |_, res| {
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
