use serial_test::serial;
use spacetimedb_lib::sats::{product, AlgebraicValue};
use spacetimedb_testing::modules::{
    CompilationMode, CompiledModule, LogLevel, LoggerRecord, ModuleHandle, DEFAULT_CONFIG, IN_MEMORY_CONFIG,
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

            assert_eq!(
                read_logs(&module).await,
                [
                    "Hello, Tyrion!",
                    "Hello, Cersei!",
                    "Hello, World!",
                    "Cersei has age 31 >= 30",
                ]
                .map(String::from)
            );
        },
    );
}

#[test]
#[serial]
fn test_calling_a_reducer() {
    test_calling_a_reducer_in_module("spacetimedb-quickstart");
}

#[test]
#[serial]
fn test_calling_a_reducer_csharp() {
    test_calling_a_reducer_in_module("spacetimedb-quickstart-cs");
}

#[test]
#[serial]
fn test_calling_a_reducer_with_private_table() {
    init();

    CompiledModule::compile("rust-wasm-test", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_json("add_private", &product!["Tyrion"])
                .await
                .unwrap();
            module.call_reducer_json("query_private", &product![]).await.unwrap();

            let logs = read_logs(&module)
                .await
                .into_iter()
                .skip_while(|r| r.starts_with("Timestamp"))
                .collect::<Vec<_>>();

            assert_eq!(logs, ["Private, Tyrion!", "Private, World!",].map(String::from));
        },
    );
}

/// Invoke the `rust-wasm-test` module,
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
    CompiledModule::compile("rust-wasm-test", CompilationMode::Debug).with_module_async(
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

/// Call the `rust-wasm-test` module's `test` reducer with a variety of ways of passing arguments.
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

            module.call_reducer_json("load_location_table", no_args).await.unwrap();

            module
                .call_reducer_json("test_index_scan_on_id", no_args)
                .await
                .unwrap();

            module
                .call_reducer_json("test_index_scan_on_chunk", no_args)
                .await
                .unwrap();

            module
                .call_reducer_json("test_index_scan_on_x_z_dimension", no_args)
                .await
                .unwrap();

            module
                .call_reducer_json("test_index_scan_on_x_z", no_args)
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

async fn bench_call<'a>(module: &ModuleHandle, call: &str, count: &u32) -> Duration {
    let json =
        format!(r#"{{"CallReducer": {{"reducer": "{call}", "args": "[{count}]", "request_id": 0, "flags": 0 }}}}"#);

    let now = Instant::now();

    module.send(json).await.unwrap();

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

#[test]
#[serial]
fn test_calling_bench_db_circles() {
    CompiledModule::compile("benchmarks", CompilationMode::Release).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            #[rustfmt::skip]
            let benches = [
                ("insert_bulk_food", 50, "INSERT FOOD: 50"),
                ("insert_bulk_entity", 50, "INSERT ENTITY: 50"),
                ("insert_bulk_circle", 500, "INSERT CIRCLE: 500"),
                ("cross_join_circle_food", 50 * 500, "CROSS JOIN CIRCLE FOOD: 25000, processed: 2500"),
                ("cross_join_all", 50 * 50 * 500, "CROSS JOIN ALL: 1250000, processed: 1250000"),
            ];
            _run_bench_db(module, &benches).await
        },
    );
}

#[test]
#[serial]
fn test_calling_bench_db_ia_loop() {
    CompiledModule::compile("benchmarks", CompilationMode::Release).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            #[rustfmt::skip]
                let benches = [
                ("insert_bulk_position", 20_000, "INSERT POSITION: 20000"),
                ("insert_bulk_velocity", 10_000, "INSERT VELOCITY: 10000"),
                ("update_position_all", 20_000, "UPDATE POSITION ALL: 20000, processed: 20000"),
                ("update_position_with_velocity", 10_000, "UPDATE POSITION BY VELOCITY: 10000, processed: 10000"),
                ("insert_world", 5_000, "INSERT WORLD PLAYERS: 5000"),
                ("game_loop_enemy_ia", 5_000, "ENEMY IA LOOP PLAYERS: 5000, processed: 5000"),
            ];

            _run_bench_db(module, &benches).await
        },
    );
}
