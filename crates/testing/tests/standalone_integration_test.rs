use serial_test::serial;
use spacetimedb_lib::sats::{product, AlgebraicValue};
use spacetimedb_testing::modules::{
    CompilationMode, CompiledModule, Cpp, Csharp, LogLevel, LoggerRecord, ModuleHandle, ModuleLanguage, Rust,
    TypeScript, DEFAULT_CONFIG,
};
use std::{
    future::Future,
    process::Command,
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
    read_logs_allowing_warnings(module, &[]).await
}

async fn read_logs_allowing_warnings(module: &ModuleHandle, expected_warnings: &[&str]) -> Vec<String> {
    module
        .read_log(None)
        .await
        .trim()
        .split('\n')
        .map(|line| {
            let record: LoggerRecord = serde_json::from_str(line).unwrap();
            if matches!(record.level, LogLevel::Panic | LogLevel::Error)
                || (matches!(record.level, LogLevel::Warn) && !expected_warnings.contains(&record.message.as_str()))
            {
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

fn emcc_is_available() -> bool {
    Command::new("emcc")
        .arg("--version")
        .status()
        .is_ok_and(|status| status.success())
}

fn test_calling_a_reducer_in_module(module_name: &'static str) {
    init();

    CompiledModule::compile(module_name, CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |mut module| async move {
            let json =
                r#"{"CallReducer": {"reducer": "add", "args": "[\"Tyrion\", 24]", "request_id": 0, "flags": 0 }}"#
                    .to_string();
            module.send_reducer_and_recv_update(json, 0).await.unwrap();

            let json =
                r#"{"CallReducer": {"reducer": "add", "args": "[\"Cersei\", 31]", "request_id": 1, "flags": 0 }}"#
                    .to_string();
            module.send_reducer_and_recv_update(json, 1).await.unwrap();

            let json =
                r#"{"CallReducer": {"reducer": "say_hello", "args": "[]", "request_id": 2, "flags": 0 }}"#.to_string();
            module.send_reducer_and_recv_update(json, 2).await.unwrap();

            let json = r#"{"CallReducer": {"reducer": "list_over_age", "args": "[30]", "request_id": 3, "flags": 0 }}"#
                .to_string();
            module.send_reducer_and_recv_update(json, 3).await.unwrap();

            let json =
                r#"{"CallReducer": {"reducer": "log_module_identity", "args": "[]", "request_id": 4, "flags": 0 }}"#
                    .to_string();
            module.send_reducer_and_recv_update(json, 4).await.unwrap();

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
#[cfg_attr(
    target_os = "macos",
    ignore = "NativeAOT-LLVM is only supported on Windows and Linux"
)]
fn test_calling_a_reducer_csharp() {
    test_calling_a_reducer_in_module("module-test-cs");
}

#[test]
#[serial]
fn namespace_csharp_nested_registration() {
    init();
    CompiledModule::compile("nested-namespace-test-cs", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let host = module.client.module();
            let schema = &host.info.module_def;
            let branch = &schema.submodules()["branch_data"];
            assert!(branch.submodules().contains_key("nested_data"));
            assert!(schema.submodules().contains_key("leaf_data"));
            assert!(schema.submodules().contains_key("promoted_data"));
            assert!(schema.submodules().contains_key("second_data"));
            assert!(schema.match_http_route(&spacetimedb_lib::http::Method::Get, "/leaf").is_none());

            let warnings: Vec<_> = ["Branch.Leaf", "Leaf", "Promoted", "SecondLeaf", "class", "class.Branch.Leaf"].into_iter()
                .map(|path| format!("HTTP routes declared in submodule '{path}' are ignored. Define HTTP routes in the root module instead."))
                .collect();
            // Warnings must be emitted during description/publication, before any call.
            let log = module.read_log(None).await;
            for warning in &warnings {
                assert!(log.contains(warning));
            }

            // Reuse the existing functions to exercise real immediate reducer/procedure calls.
            // Run twice so each target also exercises the cached-name path.
            for iteration in 0..2 {
                for stage in ["ping", "pong", "next"] {
                    assert_eq!(module.call_http_route_get(&format!("/schedule/{stage}")).await.unwrap().as_ref(), b"scheduled");
                    tokio::time::timeout(std::time::Duration::from_secs(10), async {
                        loop {
                            let mut complete = true;
                            for (path, id) in [("branch_data.nested_data", 2i32), ("outer_data.branch_data.nested_data", 8)] {
                                let result = spacetimedb::sql::execute::run(
                                    host.relational_db().clone(),
                                    format!("SELECT * FROM {path}.user WHERE id = 1"),
                                    spacetimedb_lib::identity::AuthCtx::for_current(spacetimedb_lib::Identity::ZERO),
                                    Some(host.info.subscriptions.clone()),
                                    Some(host.clone()),
                                    &mut vec![],
                                ).await.unwrap();
                                let expected = match stage {
                                    "ping" => vec![product![1i32, id + 100]],
                                    "pong" => vec![],
                                    _ => vec![product![1i32, id]],
                                };
                                complete &= result.rows == expected;
                            }
                            if stage == "next" {
                                // The first procedure transaction makes rows visible before the
                                // rollback check finishes. Wait for both procedure bodies to finish.
                                let logs = read_logs_allowing_warnings(&module, &warnings.iter().map(String::as_str).collect::<Vec<_>>()).await;
                                complete &= ["next:2", "next:8"].iter().all(|message| logs.iter().filter(|line| line.as_str() == *message).count() == iteration + 1);
                            }
                            if complete {
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        }
                    }).await.expect("immediate scheduling did not reach the selected instances");
                }
            }
            assert_eq!(module.call_http_route_get("/schedule/reset").await.unwrap().as_ref(), b"reset");

            module.call_reducer_binary("ping", &product![]).await.unwrap();
            assert_eq!(module.call_http_route_get("/contexts").await.unwrap().as_ref(), b"selected");
            assert_eq!(
                module.call_procedure_with_args("instance", "[]").await.unwrap(),
                AlgebraicValue::I32(0)
            );
            for (path, id) in [("branch_data.nested_data", 2i32), ("leaf_data", 3), ("promoted_data", 4), ("second_data", 5), ("outer_data", 6), ("outer_data.branch_data.nested_data", 8)] {
                module
                    .call_reducer_binary(&format!("{path}.ping"), &product![])
                    .await
                    .unwrap();
                module
                    .call_reducer_binary(&format!("{path}.pong"), &product![])
                    .await
                    .unwrap();
                assert_eq!(
                    module
                        .call_procedure_with_args(&format!("{path}.instance"), "[]")
                        .await
                        .unwrap(),
                    AlgebraicValue::I32(id)
                );
                assert_eq!(
                    module
                        .call_procedure_with_args(&format!("{path}.next"), "[]")
                        .await
                        .unwrap(),
                    AlgebraicValue::I32(id + 10)
                );
                for (view, expected) in [("current", id), ("anonymous", id + 10), ("query_current", id), ("query_anonymous", id + 10)] {
                    let result = spacetimedb::sql::execute::run(
                        host.relational_db().clone(),
                        format!("SELECT * FROM {path}.{view}"),
                        spacetimedb_lib::identity::AuthCtx::for_current(spacetimedb_lib::Identity::ZERO),
                        Some(host.info.subscriptions.clone()),
                        Some(host.clone()),
                        &mut vec![],
                    )
                    .await
                    .unwrap();
                    assert_eq!(result.rows, [product![if expected == id { 1i32 } else { 2i32 }, expected]]);
                }
            }
            module.call_reducer_binary("check_tables", &product![]).await.unwrap();
            assert_eq!(
                module
                    .call_procedure_with_args("branch_data.instance", "[]")
                    .await
                    .unwrap(),
                AlgebraicValue::I32(1)
            );
            assert_eq!(
                module.call_procedure_with_args("outer_data.branch_data.instance", "[]").await.unwrap(),
                AlgebraicValue::I32(7)
            );
            for (view, expected) in [("nested", 2i32), ("deep", 8)] {
                let result = spacetimedb::sql::execute::run(
                    host.relational_db().clone(),
                    format!("SELECT * FROM {view}"),
                    spacetimedb_lib::identity::AuthCtx::for_current(spacetimedb_lib::Identity::ZERO),
                    Some(host.info.subscriptions.clone()),
                    Some(host.clone()),
                    &mut vec![],
                ).await.unwrap();
                assert_eq!(result.rows, [product![expected]]);
            }
            for (view, expected) in [("branch_data.query_child", 2i32), ("outer_data.branch_data.query_child", 8), ("query_deep", 8), ("query_public", 6)] {
                let result = spacetimedb::sql::execute::run(
                    host.relational_db().clone(),
                    format!("SELECT * FROM {view}"),
                    spacetimedb_lib::identity::AuthCtx::for_current(spacetimedb_lib::Identity::ZERO),
                    Some(host.info.subscriptions.clone()),
                    Some(host.clone()),
                    &mut vec![],
                ).await.unwrap();
                assert_eq!(result.rows, [product![1i32, expected]]);
            }
            let messages = read_logs_allowing_warnings(&module, &warnings.iter().map(String::as_str).collect::<Vec<_>>()).await
                .into_iter().filter(|message| !warnings.contains(message)).collect::<Vec<_>>();
            let (scheduled, messages) = messages.split_at(12);
            let mut scheduled = scheduled.to_vec();
            scheduled.sort();
            assert_eq!(scheduled, ["leaf:2", "leaf:2", "leaf:8", "leaf:8", "next:2", "next:2", "next:8", "next:8", "pong:2", "pong:2", "pong:8", "pong:8"]);
            assert_eq!(messages,
                ["root:0", "leaf:2", "pong:2", "next:2", "leaf:3", "pong:3", "next:3", "leaf:4", "pong:4", "next:4", "leaf:5", "pong:5", "next:5", "leaf:6", "pong:6", "next:6", "leaf:8", "pong:8", "next:8"]);
        },
    );
}

#[test]
#[serial]
fn namespace_csharp_root_selected_at_publish() {
    init();

    // Build the dependency first, then reuse its managed DLL without rebuilding or changing its project.
    let dependency = CompiledModule::compile("root-selection-dependency-cs", CompilationMode::Debug);
    dependency.with_module_async(DEFAULT_CONFIG, |module| async move {
        module
            .call_reducer_binary("dependency_entry", &product![])
            .await
            .unwrap();
        assert!(module.read_log(None).await.contains("dependency published as root"));
    });

    // Both assemblies contain generated host entrypoint declarations. Only the published root's
    // declarations must become native exports; otherwise publishing produces duplicate symbols.
    CompiledModule::compile("root-selection-consumer-cs", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module.call_reducer_binary("consumer_entry", &product![]).await.unwrap();
            assert!(module.read_log(None).await.contains("consumer published as root"));
            module
                .call_reducer_binary("dependency_entry", &product![])
                .await
                .unwrap();
            assert!(module.read_log(None).await.contains("dependency published as root"));
        },
    );
}

#[test]
#[serial]
fn namespace_csharp_cross_namespace_calls() {
    init();
    CompiledModule::compile("namespace-test-cs", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |mut module| async move {
            // Mirror test_submodule_in_module: enter the root through the websocket API,
            // then delegate to exported library callbacks using the same context object.
            module
                .send_reducer_and_recv_update(
                    r#"{"CallReducer":{"reducer":"add_auth_user","args":"[12]","request_id":0,"flags":0}}"#.to_string(),
                    0,
                )
                .await
                .unwrap();
            let warning = "HTTP routes declared in submodule 'MyAuth' are ignored. Define HTTP routes in the root module instead.";
            assert!(module.read_log(None).await.contains(warning));
            assert_eq!(read_logs_allowing_warnings(&module, &[warning]).await, ["Auth users: 1"]);
            assert_eq!(
                module.call_procedure_with_args("count_auth_users", "[]").await.unwrap(),
                AlgebraicValue::U64(1)
            );
            assert_eq!(
                module.call_http_route_get("/root-auth-count").await.unwrap().as_ref(),
                b"1"
            );

            // Mounted routes must not leak into the root HTTP router.
            assert!(module
                .client
                .module()
                .info
                .module_def
                .match_http_route(&spacetimedb_lib::http::Method::Get, "/auth-count")
                .is_none());
            // Dependencies in public still contribute routes and use root dispatch.
            assert_eq!(module.call_http_route_get("/extra-hello").await.unwrap().as_ref(), b"public");
            assert_eq!(
                module
                    .call_procedure_with_args("class.count_users", "[]")
                    .await
                    .unwrap(),
                AlgebraicValue::U64(0)
            );
            assert_eq!(
                module.call_procedure_with_args("count_users", "[]").await.unwrap(),
                AlgebraicValue::U64(1),
                "delegation must not also insert into the root or Audit table"
            );
        },
    );
}

#[test]
#[serial]
fn namespace_csharp_canonical_name_resolution() {
    use spacetimedb_lib::db::raw_def::v10::{CaseConversionPolicy, ExplicitNames, RawModuleDefV10Builder};
    use spacetimedb_schema::def::ModuleDef;
    init();
    CompiledModule::compile("namespace-test-cs", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            // Derive expected names with the actual host validator, not a second test-side converter.
            for accessor in ["MyHTTP2Auth", "public", ""] {
                for root_none in [false, true] {
                    for child_none in [false, true] {
                        for (source, explicit) in [
                            ("HTTP2ReducerTick", None),
                            ("__my__XMLParser99", None),
                            ("already_snake_case", None),
                            ("SourceName", Some("ExplicitNAME")),
                        ] {
                            let mut root = RawModuleDefV10Builder::new();
                            root.set_case_conversion_policy(if root_none {
                                CaseConversionPolicy::None
                            } else {
                                CaseConversionPolicy::SnakeCase
                            });
                            let mut child = RawModuleDefV10Builder::new();
                            child.set_case_conversion_policy(if child_none {
                                CaseConversionPolicy::None
                            } else {
                                CaseConversionPolicy::SnakeCase
                            });
                            let named = !accessor.is_empty() && accessor != "public";
                            let target = if named { &mut child } else { &mut root };
                            target.add_reducer(source, spacetimedb_lib::ProductType::unit());
                            if let Some(name) = explicit {
                                let mut names = ExplicitNames::default();
                                names.insert_function(source, name);
                                target.add_explicit_names(names);
                            }
                            if named {
                                root.add_submodule(accessor, child.finish());
                            }
                            let schema: ModuleDef = root.finish().try_into().unwrap();
                            let expected = schema.all_reducers_with_prefix()[0].2.name.to_string();
                            let args = serde_json::json!([
                                accessor,
                                null,
                                source,
                                explicit.map(|name| serde_json::json!({"some": name})),
                                root_none,
                                child_none
                            ])
                            .to_string();
                            assert_eq!(
                                module
                                    .call_procedure_with_args("resolve_schedule_name", &args)
                                    .await
                                    .unwrap(),
                                AlgebraicValue::String(expected.into()),
                                "{args}"
                            );
                        }
                    }
                }
            }
        },
    );
}

#[test]
#[serial]
fn test_calling_a_reducer_typescript() {
    test_calling_a_reducer_in_module("module-test-ts");
}

#[test]
#[serial]
fn test_calling_a_reducer_cpp() {
    if !emcc_is_available() {
        log::info!("Skipping C++ module test because `emcc` is not available in PATH");
        return;
    }
    test_calling_a_reducer_in_module("module-test-cpp");
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
                .filter(|r| !is_scheduled_test_log(r))
                .collect::<Vec<_>>();

            assert_eq!(logs, ["Private, Tyrion!", "Private, World!",].map(String::from));
        },
    );
}

/// Returns `true` if `line` was produced by the `repeating_test` or `nonrepeating_test` scheduled reducers.
fn is_scheduled_test_log(line: &str) -> bool {
    line.starts_with("Timestamp: ") || line.starts_with("This reducers runs only once")
}

async fn read_log_skip_repeating(module: &ModuleHandle) -> String {
    let logs = read_logs(module).await;
    let mut logs = logs
        .into_iter()
        // Filter out log lines from the `repeating_test` and `nonrepeating_test` reducers,
        // which run on a schedule and can appear in our logs after we've slept.
        .filter(|line| !is_scheduled_test_log(line))
        .collect::<Vec<_>>();

    if logs.len() != 1 {
        panic!("Expected a single log message but found {logs:#?}");
    };

    logs.swap_remove(0)
}

fn test_nonrepeating_scheduled_reducer_in_module(module_name: &'static str) {
    init();

    CompiledModule::compile(module_name, CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            // The `init` reducer schedules `nonrepeating_test` to run 1 second in the future.
            // Wait long enough for it to fire.
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;

            let logs = read_logs(&module).await;

            assert!(
                logs.iter().any(|line| line.starts_with("This reducers runs only once")),
                "Expected nonrepeating_test reducer to have logged, but got: {logs:#?}",
            );
        },
    );
}

#[test]
#[serial]
fn test_nonrepeating_scheduled_reducer() {
    test_nonrepeating_scheduled_reducer_in_module("module-test");
}

#[test]
#[serial]
#[cfg_attr(
    target_os = "macos",
    ignore = "NativeAOT-LLVM is only supported on Windows and Linux"
)]
fn test_nonrepeating_scheduled_reducer_csharp() {
    test_nonrepeating_scheduled_reducer_in_module("module-test-cs");
}

#[test]
#[serial]
fn test_nonrepeating_scheduled_reducer_typescript() {
    test_nonrepeating_scheduled_reducer_in_module("module-test-ts");
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

            let log_sleep = read_log_skip_repeating(&module).await;

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

fn test_calling_with_tx_in_module(module_name: &'static str) {
    init();

    CompiledModule::compile(module_name, CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let json = r#"
{
  "CallProcedure": {
    "procedure": "with_tx",
    "args": "[]",
    "request_id": 0,
    "flags": 0
  }
}"#
            .to_string();
            module.send(json).await.unwrap();

            // Wait 1 second just to be safe.
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            let log = read_log_skip_repeating(&module).await;

            assert!(log.contains("Hello, World!"));
        },
    )
}

#[test]
#[serial]
fn test_calling_with_tx() {
    test_calling_with_tx_in_module("module-test");
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
fn test_call_query_macro_with_caller<F: Future<Output = ModuleHandle>>(caller: impl FnOnce(ModuleHandle) -> F) {
    CompiledModule::compile("module-test", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let module = caller(module).await;
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
    test_call_query_macro_with_caller(|mut module| async move {
        // Note that JSON doesn't allow multiline strings, so the encoded args string must be on one line!
        let json = r#"
{ "CallReducer": {
  "reducer": "test",
  "args":
    "[ { \"x\": 0, \"y\": 2, \"z\": \"Macro\" }, { \"foo\": \"Foo\" }, { \"foo\": {} }, { \"baz\": \"buzz\" } ]",
  "request_id": 0,
  "flags": 0
	} }"#
            .to_string();
        module.send_reducer_and_recv_update(json, 0).await.unwrap();
        module
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
        module
    });

    // BSATN via the `Serialize` path.
    test_call_query_macro_with_caller(|module| async move {
        module.call_reducer_binary("test", args_pv).await.unwrap();
        module
    });
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
#[cfg_attr(
    target_os = "macos",
    ignore = "NativeAOT-LLVM is only supported on Windows and Linux"
)]
fn test_calling_bench_db_circles_csharp() {
    test_calling_bench_db_circles::<Csharp>();
}

#[test]
#[serial]
fn test_calling_bench_db_circles_typescript() {
    test_calling_bench_db_circles::<TypeScript>();
}
#[test]
#[serial]
fn test_calling_bench_db_circles_cpp() {
    if !emcc_is_available() {
        log::info!("Skipping C++ module test because `emcc` is not available in PATH");
        return;
    }
    test_calling_bench_db_circles::<Cpp>();
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
#[cfg_attr(
    target_os = "macos",
    ignore = "NativeAOT-LLVM is only supported on Windows and Linux"
)]
fn test_calling_bench_db_ia_loop_csharp() {
    test_calling_bench_db_ia_loop::<Csharp>();
}

#[test]
#[serial]
fn test_calling_bench_db_ia_loop_typescript() {
    test_calling_bench_db_ia_loop::<TypeScript>();
}
#[test]
#[serial]
fn test_calling_bench_db_ia_loop_cpp() {
    if !emcc_is_available() {
        log::info!("Skipping C++ module test because `emcc` is not available in PATH");
        return;
    }
    test_calling_bench_db_ia_loop::<Cpp>();
}

fn test_submodule_in_module(module_name: &'static str) {
    init();

    CompiledModule::compile(module_name, CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |mut module| async move {
            // ── 1. Cross-namespace reducer call ──────────────────────────────────
            // `useSubmodule` is exported as camelCase; its wire name is the canonical
            // snake_case form.
            let json =
                r#"{"CallReducer": {"reducer": "use_submodule", "args": "[\"hello_submodule\"]", "request_id": 0, "flags": 0}}"#
                    .to_string();
            module.send_reducer_and_recv_update(json, 0).await.unwrap();

            let logs = read_logs(&module).await;
            let relevant: Vec<_> = logs
                .into_iter()
                .filter(|l| !is_scheduled_test_log(l))
                .collect();
            assert_eq!(relevant, ["libInsert: hello_submodule"].map(String::from));

            // ── 2. Cross-namespace procedure call ─────────────────────────────────
            // useSubmoduleProcedure calls libCount in the lib submodule.
            // We inserted one row above, so the count should be 1.
            let return_val = module
                .call_procedure_with_args("use_submodule_procedure", "[]")
                .await
                .expect("use_submodule_procedure should succeed");
            assert_eq!(return_val, AlgebraicValue::U64(1), "libCount should return 1 after one insert");

            // ── 3. Cross-namespace HTTP handler ───────────────────────────────────
            // The root module's /lib-hello route delegates to lib_submodule's libHello handler.
            let body = module
                .call_http_route_get("/lib-hello")
                .await
                .expect("GET /lib-hello should succeed");
            assert_eq!(body.as_ref(), b"Hello from lib submodule!");
        },
    );
}

#[test]
#[serial]
fn test_submodule_typescript() {
    test_submodule_in_module("module-test-ts");
}
