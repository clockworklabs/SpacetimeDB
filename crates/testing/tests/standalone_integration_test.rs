use serial_test::serial;
use spacetimedb_testing::modules::{
    CompilationMode, CompiledModule, LogLevel, LoggerRecord, ModuleHandle, DEFAULT_CONFIG,
};
use std::time::{Duration, Instant};

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
            let json = r#"{"call": {"fn": "add", "args": ["Tyrion", 24]}}"#.to_string();
            module.send(json).await.unwrap();

            let json = r#"{"call": {"fn": "add", "args": ["Cersei", 31]}}"#.to_string();
            module.send(json).await.unwrap();

            let json = r#"{"call": {"fn": "say_hello", "args": []}}"#.to_string();
            module.send(json).await.unwrap();

            let json = r#"{"call": {"fn": "list_over_age", "args": [30]}}"#.to_string();
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
            let json = r#"{"call": {"fn": "add_private", "args": ["Tyrion"]}}"#.to_string();
            module.send(json).await.unwrap();
            let json = r#"{"call": {"fn": "query_private", "args": []}}"#.to_string();
            module.send(json).await.unwrap();

            assert_eq!(
                read_logs(&module).await,
                ["Private, Tyrion!", "Private, World!",].map(String::from)
            );
        },
    );
}

#[test]
#[serial]
fn test_call_query_macro() {
    CompiledModule::compile("rust-wasm-test", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let json = r#"
{"call": {"fn": "test", "args":[
    {"x":0, "y":2, "z":"Macro"},
    {"foo":"Foo"},
    {"Foo": {} }
]}}"#
                .to_string();
            module.send(json).await.unwrap();

            let logs = read_logs(&module).await;

            assert_eq!(logs[0], "BEGIN");
            assert!(logs[1].starts_with("sender: "));
            assert!(logs[2].starts_with("timestamp: "));

            assert_eq!(
                logs[3..],
                [
                    r#"bar: "Foo""#,
                    "Foo",
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

async fn bench_call<'a>(module: &ModuleHandle, call: &str, count: &u32) -> Duration {
    let json = format!(r#"{{"call": {{"fn": "{call}", "args": [{count}]}}}}"#);

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
                ("insert_bulk_position", 2_000_000, "INSERT POSITION: 2000000"),
                ("insert_bulk_velocity", 1_000_000, "INSERT VELOCITY: 1000000"),
                ("update_position_all", 2_000_000, "UPDATE POSITION ALL: 2000000, processed: 2000000"),
                ("update_position_with_velocity", 1_000_000, "UPDATE POSITION BY VELOCITY: 1000000, processed: 1000000"),
                ("insert_world", 5_000, "INSERT WORLD PLAYERS: 5000"),
                ("game_loop_enemy_ia", 5_000, "ENEMY IA LOOP PLAYERS: 5000, processed: 2500"),
            ];

            _run_bench_db(module, &benches).await
        },
    );
}
