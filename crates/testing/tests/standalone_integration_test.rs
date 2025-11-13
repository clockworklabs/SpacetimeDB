use serial_test::serial;
use spacetimedb_lib::sats::{product, AlgebraicValue};
use spacetimedb_testing::modules::{
    CompilationMode, CompiledModule, Csharp, LogLevel, LoggerRecord, ModuleHandle, ModuleLanguage, Rust, TypeScript,
    DEFAULT_CONFIG, IN_MEMORY_CONFIG,
};
use std::{
    future::Future,
    time::{Duration, Instant},
};

fn init() {
    let _ = env_logger::builder()
        .parse_filters(
            "spacetimedb=trace,spacetimedb_client_api=trace,spacetimedb_lib=trace,spacetimedb_standalone=trace",
        )
        .is_test(true)
        // `try_init` and ignore failures to continue if a logger is already registered.
        // This allows us to call `init` at the start of every test without a `once_cell` or similar.
        .try_init();
}

async fn read_logs(module: &ModuleHandle) -> Vec<String> {
    module
        .read_log(None)
        .await
        .trim()
        .split('\n')
        .map(|line| {
            let record: LoggerRecord = serde_json::from_str(line).unwrap();
            if matches!(record.level, LogLevel::Panic | LogLevel::Error | LogLevel::Warn) {
                panic!("Found an error-like log line: {line}");
            }
            record.message
        })
        .skip_while(|line| line != "Database initialized")
        .skip(1)
        .collect::<Vec<_>>()
}

// The tests MUST be run in sequence because they read the OS environment
// and can cause a race when run in parallel.

fn test_calling_a_reducer_in_module(module_name: &'static str) {
    init();

    CompiledModule::compile(module_name, CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let json =
                r#"{"CallReducer": {"reducer": "add", "args": "[\"Tyrion\", 24]", "request_id": 0, "flags": 0 }}"#
                    .to_string();
            module.send(json).await.unwrap();

            let json =
                r#"{"CallReducer": {"reducer": "add", "args": "[\"Cersei\", 31]", "request_id": 1, "flags": 0 }}"#
                    .to_string();
            module.send(json).await.unwrap();

            let json =
                r#"{"CallReducer": {"reducer": "say_hello", "args": "[]", "request_id": 2, "flags": 0 }}"#.to_string();
            module.send(json).await.unwrap();

            let json = r#"{"CallReducer": {"reducer": "list_over_age", "args": "[30]", "request_id": 3, "flags": 0 }}"#
                .to_string();
            module.send(json).await.unwrap();

            let json =
                r#"{"CallReducer": {"reducer": "log_module_identity", "args": "[]", "request_id": 4, "flags": 0 }}"#
                    .to_string();
            module.send(json).await.unwrap();

            assert_eq!(
                read_logs(&module).await,
                [
                    "Hello, Tyrion!",
                    "Hello, Cersei!",
                    "Hello, World!",
                    "Cersei has age 31 >= 30",
                ]
                .into_iter()
                .map(String::from)
                .chain(std::iter::once(format!("Module identity: {}", module.db_identity)))
                .collect::<Vec<_>>()
            );
        },
    );
}

#[test]
#[serial]
fn test_calling_a_reducer() {
    test_calling_a_reducer_in_module("module-test");
}

#[test]
#[serial]
fn test_calling_a_reducer_csharp() {
    test_calling_a_reducer_in_module("module-test-cs");
}

#[test]
#[serial]
fn test_calling_a_reducer_typescript() {
    test_calling_a_reducer_in_module("module-test-ts");
}

#[test]
#[serial]
fn test_calling_a_reducer_with_private_table() {
    init();

    CompiledModule::compile("module-test", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("add_private", &product!["Tyrion"])
                .await
                .unwrap();
            module.call_reducer_binary("query_private", &product![]).await.unwrap();

            let logs = read_logs(&module)
                .await
                .into_iter()
                .skip_while(|r| r.starts_with("Timestamp"))
                .collect::<Vec<_>>();

            assert_eq!(logs, ["Private, Tyrion!", "Private, World!",].map(String::from));
        },
    );
}

fn test_calling_a_procedure_in_module(module_name: &'static str) {
    init();

    CompiledModule::compile(module_name, CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let json = r#"
{
  "CallProcedure": {
    "procedure": "sleep_one_second",
    "args": "[]",
    "request_id": 0,
    "flags": 0
  }
}"#
            .to_string();
            module.send(json).await.unwrap();

            // It sleeps one second, but we'll wait two just to be safe.
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let logs = read_logs(&module).await;
            let logs = logs
                .into_iter()
                // Filter out log lines from the `repeating_test` reducer,
                // which runs frequently enough to appear in our logs after we've slept a second.
                .filter(|line| !line.starts_with("Timestamp: Timestamp { __timestamp_micros_since_unix_epoch__: "))
                .collect::<Vec<_>>();
            let [log_sleep] = &logs[..] else {
                panic!("Expected a single log message but found {logs:#?}");
            };

            assert!(log_sleep.starts_with("Slept from "));
            assert!(log_sleep.contains("a total of"));
        },
    )
}

#[test]
#[serial]
fn test_calling_a_procedure() {
    test_calling_a_procedure_in_module("module-test");
}

/// Invoke the `module-test` module,
/// use `caller` to invoke its `test` reducer,
/// and assert that its logs look right.
///
/// `caller` must invoke the reducer with args equivalent to:
/// ```ignore
/// [
///     TestA {
///         x: 0,
///         y: 2,
///         z: "Macro".to_string(),
///     },
///     TestB {
///         foo: "Foo".to_string(),
///     },
///     TestC::Foo,
///     TestF::Baz("buzz".to_string()),
/// ]
/// ```
fn test_call_query_macro_with_caller<F: Future<Output = ()>>(caller: impl FnOnce(ModuleHandle) -> F) {
    CompiledModule::compile("module-test", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            caller(module.clone()).await;
            let logs = read_logs(&module)
                .await
                .into_iter()
                .filter(|line| {
                    !(line.starts_with("sender:") || line.starts_with("timestamp:") || line.starts_with("Timestamp"))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                logs,
                [
                    "BEGIN",
                    r#"bar: "Foo""#,
                    "Foo",
                    "buzz",
                    "Row count before delete: 1000",
                    r#"Inserted: TestE { id: 1, name: "Tyler" }"#,
                    "Row count after delete: 995",
                    "Row count filtered by condition: 995",
                    "MultiColumn",
                    "Row count filtered by multi-column condition: 199",
                    "END",
                ]
                .map(String::from)
            );
        },
    );
}

/// Call the `module-test` module's `test` reducer with a variety of ways of passing arguments.
#[test]
#[serial]
fn test_call_query_macro() {
    // Hand-written JSON. This will fail if the JSON encoding of `ClientMessage` changes.
    test_call_query_macro_with_caller(|module| async move {
        // Note that JSON doesn't allow multiline strings, so the encoded args string must be on one line!
        let json = r#"
{ "CallReducer": {
  "reducer": "test",
  "args":
    "[ { \"x\": 0, \"y\": 2, \"z\": \"Macro\" }, { \"foo\": \"Foo\" }, { \"Foo\": {} }, { \"Baz\": \"buzz\" } ]",
  "request_id": 0,
  "flags": 0
} }"#
            .to_string();
        module.send(json).await.unwrap();
    });

    let args_pv = &product![
        product![0u32, 2u32, "Macro"],
        product!["Foo"],
        AlgebraicValue::sum(0, AlgebraicValue::unit()),
        AlgebraicValue::sum(2, AlgebraicValue::String("buzz".into())),
    ];

    // JSON via the `Serialize` path.
    test_call_query_macro_with_caller(|module| async move {
        module.call_reducer_json("test", args_pv).await.unwrap();
    });

    // BSATN via the `Serialize` path.
    test_call_query_macro_with_caller(|module| async move {
        module.call_reducer_binary("test", args_pv).await.unwrap();
    });
}

#[test]
#[serial]
/// This test runs the index scan workloads in the `perf-test` module.
/// Timing spans should be < 1ms if the correct index was used.
/// Otherwise these workloads will degenerate into full table scans.
fn test_index_scans() {
    init();
    CompiledModule::compile("perf-test", CompilationMode::Release).with_module_async(
        IN_MEMORY_CONFIG,
        |module| async move {
            let no_args = &product![];

            module
                .call_reducer_binary("load_location_table", no_args)
                .await
                .unwrap();

            module
                .call_reducer_binary("test_index_scan_on_id", no_args)
                .await
                .unwrap();

            module
                .call_reducer_binary("test_index_scan_on_chunk", no_args)
                .await
                .unwrap();

            module
                .call_reducer_binary("test_index_scan_on_x_z_dimension", no_args)
                .await
                .unwrap();

            module
                .call_reducer_binary("test_index_scan_on_x_z", no_args)
                .await
                .unwrap();

            let logs = read_logs(&module).await;

            // Each timing span should be < 1ms
            let timing = |line: &str| {
                line.starts_with("Timing span")
                    && (line.ends_with("ns") || line.ends_with("us") || line.ends_with("µs"))
            };
            assert!(timing(&logs[0]));
            assert!(timing(&logs[1]));
            assert!(timing(&logs[2]));
            assert!(timing(&logs[3]));
        },
    );
}

async fn bench_call(module: &ModuleHandle, call: &str, count: &u32) -> Duration {
    let now = Instant::now();

    // Note: using JSON variant because some functions accept u64 instead, so we rely on JSON's dynamic typing.
    module.call_reducer_json(call, &product![*count]).await.unwrap();

    now.elapsed()
}

#[allow(clippy::disallowed_macros)]
async fn _run_bench_db(module: ModuleHandle, benches: &[(&str, u32, &str)]) {
    let expect: Vec<_> = benches.iter().map(|x| x.2.to_string()).collect();
    let mut timings = Vec::with_capacity(benches.len());
    for (name, count, _) in benches {
        let elapsed = bench_call(&module, name, count).await;
        timings.push((name, count, elapsed));
    }

    assert_eq!(read_logs(&module).await, expect);

    for (name, rows, elapsed) in timings {
        println!("RUN {name:<30} x {rows:>10} rows: {elapsed:>20.3?}");
    }
}

fn test_calling_bench_db_circles<L: ModuleLanguage>() {
    L::get_module().with_module_async(DEFAULT_CONFIG, |module| async move {
        #[rustfmt::skip]
        let benches = [
            ("insert_bulk_food", 50, "INSERT FOOD: 50"),
            ("insert_bulk_entity", 50, "INSERT ENTITY: 50"),
            ("insert_bulk_circle", 500, "INSERT CIRCLE: 500"),
            ("cross_join_circle_food", 50 * 500, "CROSS JOIN CIRCLE FOOD: 25000, processed: 2500"),
            ("cross_join_all", 50 * 50 * 500, "CROSS JOIN ALL: 1250000, processed: 1250000"),
        ];

        _run_bench_db(module, &benches).await
    });
}

#[test]
#[serial]
fn test_calling_bench_db_circles_rust() {
    test_calling_bench_db_circles::<Rust>();
}

#[test]
#[serial]
fn test_calling_bench_db_circles_csharp() {
    test_calling_bench_db_circles::<Csharp>();
}

#[test]
#[serial]
fn test_calling_bench_db_circles_typescript() {
    test_calling_bench_db_circles::<TypeScript>();
}

fn test_calling_bench_db_ia_loop<L: ModuleLanguage>() {
    L::get_module().with_module_async(DEFAULT_CONFIG, |module| async move {
        #[rustfmt::skip]
        let benches = [
            ("insert_bulk_position", 20_000, "INSERT POSITION: 20000"),
            ("insert_bulk_velocity", 10_000, "INSERT VELOCITY: 10000"),
            ("update_position_all", 20_000, "UPDATE POSITION ALL: 20000, processed: 20000"),
            ("update_position_with_velocity", 10_000, "UPDATE POSITION BY VELOCITY: 10000, processed: 10000"),
            ("insert_world", 5_000, "INSERT WORLD PLAYERS: 5000"),
            // Note: we set lower amount of ia loop players here than in benchmarks.
            // Otherwise tests will take forever because they are built in debug mode.
            ("game_loop_enemy_ia", 100, "ENEMY IA LOOP PLAYERS: 100, processed: 5000"),
        ];

        _run_bench_db(module, &benches).await
    });
}

#[test]
#[serial]
fn test_calling_bench_db_ia_loop_rust() {
    test_calling_bench_db_ia_loop::<Rust>();
}

#[test]
#[serial]
fn test_calling_bench_db_ia_loop_csharp() {
    test_calling_bench_db_ia_loop::<Csharp>();
}

#[test]
#[serial]
fn test_calling_bench_db_ia_loop_typescript() {
    test_calling_bench_db_ia_loop::<TypeScript>();
}
