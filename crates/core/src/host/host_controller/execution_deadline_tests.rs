//! Actual V8 module hosts, transactions and updates. No network or external service.
use super::*;
use crate::db::persistence::LocalPersistenceProvider;
use crate::host::FunctionArgs;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_lib::db::raw_def::{v10::RawModuleDefV10Builder, v9::Lifecycle};
use spacetimedb_paths::FromPathUnchecked;
use spacetimedb_sats::{AlgebraicType, ProductType};

fn config() -> HostRuntimeConfig {
    HostRuntimeConfig {
        v8: V8Config {
            execution_timeout: Duration::from_millis(200),
            ..V8Config::default()
        },
        ..HostRuntimeConfig::default()
    }
}

fn program(loop_in_first_init: bool) -> Program {
    program_with_startup_cutoff(loop_in_first_init, u64::MAX)
}

fn program_with_startup_cutoff(loop_in_first_init: bool, cutoff_millis: u64) -> Program {
    let mut schema = RawModuleDefV10Builder::new();
    schema
        .build_table_with_new_type("rows", [("value", AlgebraicType::U64)], true)
        .finish();
    schema.add_lifecycle_reducer(Lifecycle::Init, "init", ProductType::unit());
    schema.add_reducer("loop", ProductType::unit());
    schema.add_reducer("good", ProductType::unit());
    schema.add_procedure("task", ProductType::unit(), AlgebraicType::U64);
    let schema = spacetimedb_lib::bsatn::to_vec(&spacetimedb_lib::RawModuleDef::V10(schema.finish())).unwrap();
    Program::from_bytes(
        ModuleKind::JS,
        format!(
            r#"
            import {{ register_hooks, table_id_from_name, datastore_insert_bsatn }} from "spacetime:sys@1.0";
            import {{ register_hooks as register_procedure_hooks }} from "spacetime:sys@1.2";
            if (Date.now() > {cutoff_millis}) {{ for (;;) {{}} }}
            let initCalls = 0;
            let nextValue = 0;
            register_hooks({{
                __describe_module__: function() {{ return new Uint8Array({schema:?}); }},
                __call_reducer__: function(id) {{
                    const row = new Uint8Array(8);
                    row[0] = ++nextValue;
                    datastore_insert_bsatn(table_id_from_name("rows"), row);
                    if (id === 1 || (id === 0 && ++initCalls === 1 && {loop_in_first_init})) {{ for (;;) {{}} }}
                    return {{ tag: "ok" }};
                }},
            }});
            let retained;
            register_procedure_hooks({{ __call_procedure__: function() {{
                if (Date.now() > {cutoff_millis}) {{ retained = new Array(4 * 1024 * 1024).fill(1); }}
                return new Uint8Array(8);
            }} }});
            "#
        )
        .into_bytes(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execution_deadline_procedure_startup_failure_keeps_main_module_registered() {
    use std::time::{SystemTime, UNIX_EPOCH};
    let cutoff = SystemTime::now().duration_since(UNIX_EPOCH).unwrap() + Duration::from_secs(3);
    let program = program_with_startup_cutoff(false, cutoff.as_millis() as u64);
    let (_directory, controller, database) = controller_fixture(
        0xed05,
        &program,
        HostRuntimeConfig {
            v8: V8Config {
                // One slot makes a leaked admission permit observable on retry.
                procedure_instance_pool_size: std::num::NonZeroUsize::new(1).unwrap(),
                ..config().v8
            },
            ..config()
        },
    );
    let module = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    assert!(call(&module, "good").await.is_ok());
    let until_cutoff = cutoff.saturating_sub(SystemTime::now().duration_since(UNIX_EPOCH).unwrap());
    tokio::time::sleep(until_cutoff + Duration::from_millis(50)).await;
    // The module's main isolate already ran startup. Only newly created
    // procedure isolates encounter the now-hostile startup branch.
    for _ in 0..2 {
        let result = timeout(
            Duration::from_secs(5),
            module.call_procedure(Identity::ONE, None, None, "task", FunctionArgs::Nullary),
        )
        .await
        .unwrap();
        let error = result.result.unwrap_err();
        assert!(error.to_string().contains("wall-clock limit"), "{error}");
        let registered = controller.get_module_host(database.id).await.unwrap();
        assert!(call(&registered, "good").await.is_ok());
    }
    assert_eq!(row_count(&module), Some(4));
    drop(module);
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
}

fn database(id: u64, program: &Program) -> Database {
    Database {
        id,
        database_identity: Identity::from_u256(id.into()),
        owner_identity: Identity::ONE,
        host_type: HostType::Js,
        initial_program: program.hash,
        bootstrap_generation: 0,
    }
}

fn controller_fixture(
    id: u64,
    program: &Program,
    config: HostRuntimeConfig,
) -> (tempfile::TempDir, HostController, Database) {
    let directory = tempfile::tempdir().unwrap();
    let data_dir = Arc::new(ServerDataDir::from_path_unchecked(directory.path().to_owned()));
    let initial = program.clone();
    let storage = move |hash| {
        let program = initial.clone();
        async move { Ok((program.hash == hash).then_some(program.bytes)) }
    };
    let controller = HostController::new(
        data_dir.clone(),
        db::Config {
            storage: db::Storage::Memory,
            page_pool_max_size: None,
        },
        config,
        Arc::new(storage),
        Arc::new(NullEnergyMonitor),
        Arc::new(()),
        Arc::new(LocalPersistenceProvider::new(data_dir)),
        JobCores::without_pinned_cores(),
    );
    (directory, controller, database(id, program))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execution_deadline_failed_procedure_recreation_keeps_main_module_registered() {
    use std::time::{SystemTime, UNIX_EPOCH};
    let cutoff = SystemTime::now().duration_since(UNIX_EPOCH).unwrap() + Duration::from_secs(3);
    let program = program_with_startup_cutoff(false, cutoff.as_millis() as u64);
    let (_directory, controller, database) = controller_fixture(
        0xed06,
        &program,
        HostRuntimeConfig {
            v8: V8Config {
                procedure_instance_pool_size: std::num::NonZeroUsize::new(1).unwrap(),
                heap_policy: crate::config::V8HeapPolicyConfig {
                    heap_limit_bytes: 64 * 1024 * 1024,
                    heap_check_request_interval: Some(1),
                    heap_gc_trigger_fraction: 0.2,
                    heap_retire_fraction: 0.2,
                    ..Default::default()
                },
                ..config().v8
            },
            ..config()
        },
    );
    let module = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    // Populate the one-slot procedure pool before the startup branch changes.
    assert!(module
        .call_procedure(Identity::ONE, None, None, "task", FunctionArgs::Nullary)
        .await
        .result
        .is_ok());
    let remaining = cutoff.saturating_sub(SystemTime::now().duration_since(UNIX_EPOCH).unwrap());
    tokio::time::sleep(remaining + Duration::from_millis(50)).await;
    // This invocation runs in the existing isolate and leaves32MiB alive. The
    // real post-call heap policy retires it and tries a new, now-looping startup.
    assert!(module
        .call_procedure(Identity::ONE, None, None, "task", FunctionArgs::Nullary)
        .await
        .result
        .is_ok());
    let result = timeout(
        Duration::from_secs(5),
        module.call_procedure(Identity::ONE, None, None, "task", FunctionArgs::Nullary),
    )
    .await
    .unwrap();
    let error = result.result.unwrap_err();
    assert!(
        error.to_string().contains("procedure isolate startup failed"),
        "{error}"
    );
    assert!(error.to_string().contains("wall-clock limit"), "{error}");
    let registered = controller.get_module_host(database.id).await.unwrap();
    assert!(call(&registered, "good").await.is_ok());
    // A subsequent checkout also completes, proving the dead instance's slot
    // was released instead of leaked after its replacement failed.
    assert!(timeout(
        Duration::from_secs(5),
        module.call_procedure(Identity::ONE, None, None, "task", FunctionArgs::Nullary,)
    )
    .await
    .unwrap()
    .result
    .is_err());
    assert!(call(&registered, "good").await.is_ok());
    drop(registered);
    drop(module);
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
}

async fn launch(id: u64, program: Program) -> anyhow::Result<(Program, LaunchedModule)> {
    timeout(
        Duration::from_secs(5),
        Host::try_init_in_memory_to_check(
            &HostRuntimes::new(None, config()),
            PagePool::new(None),
            database(id, &program),
            program,
            AllocatedJobCore::default(),
            BsatnRowListBuilderPool::new(),
        ),
    )
    .await?
}

fn row_count(module: &ModuleHost) -> Option<u64> {
    module.relational_db().with_read_only(Workload::Internal, |tx| {
        tx.table_id_from_name("rows")
            .unwrap()
            .and_then(|table| tx.table_row_count(table))
    })
}

async fn call(module: &ModuleHost, name: &str) -> ReducerCallResult {
    timeout(
        Duration::from_secs(5),
        module.call_reducer(Identity::ONE, None, None, None, None, name, FunctionArgs::Nullary),
    )
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execution_deadline_rolls_back_init_and_reducer_and_preserves_isolate() {
    let (program, launched) = launch(0xed01, program(true)).await.unwrap();
    let module = launched.module_host;
    let result = module.init_database(program.clone()).await.unwrap().reducer.unwrap();
    assert!(result.is_err(), "infinite init must fail");
    assert!(module.relational_db().program().unwrap().is_none());
    assert_eq!(
        row_count(&module),
        None,
        "failed init must roll back the schema and inserted row"
    );

    assert!(module.init_database(program).await.unwrap().reducer.unwrap().is_ok());
    assert_eq!(row_count(&module), Some(1));
    let failed = call(&module, "loop").await;
    assert!(failed.is_err());
    assert_eq!(
        row_count(&module),
        Some(1),
        "timed-out reducer must roll back its write"
    );

    for _ in 0..64 {
        assert!(call(&module, "good").await.is_ok());
    }
    // Cross the original timer boundary before reusing the same isolate again.
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(call(&module, "good").await.is_ok());
    assert_eq!(row_count(&module), Some(66));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execution_deadline_bounds_startup_description_and_failed_update() {
    let valid = program(false);
    let looping_startup = Program::from_bytes(ModuleKind::JS, b"for (;;) {}".to_vec());
    let looping_description = Program::from_bytes(
        ModuleKind::JS,
        br#"import {register_hooks} from "spacetime:sys@1.0";
        register_hooks({__describe_module__: function() {for (;;) {}}, __call_reducer__: function() {}});"#
            .to_vec(),
    );
    for (id, bad) in [(0xed02, &looping_startup), (0xed03, &looping_description)] {
        let error = match launch(id, bad.clone()).await {
            Ok(_) => panic!("infinite JavaScript unexpectedly launched"),
            Err(error) => error,
        };
        assert!(format!("{error:#}").contains("wall-clock limit"), "{error:#}");
    }

    let (_directory, controller, database) = controller_fixture(0xed04, &valid, config());
    let module = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    assert_eq!(row_count(&module), Some(1));
    for bad in [looping_startup, looping_description] {
        assert!(timeout(
            Duration::from_secs(5),
            controller.update_module_host(
                database.clone(),
                HostType::Js,
                database.id,
                bad.bytes,
                MigrationPolicy::Compatible,
            )
        )
        .await
        .unwrap()
        .is_err());
        assert_eq!(module.relational_db().program().unwrap().unwrap().hash, valid.hash);
        assert!(call(&module, "good").await.is_ok());
    }
    let mut newer = valid.bytes.to_vec();
    newer.extend_from_slice(b"\n// Valid replacement after two deadline failures.\n");
    let newer = Program::from_bytes(ModuleKind::JS, newer);
    controller
        .update_module_host(
            database.clone(),
            HostType::Js,
            database.id,
            newer.bytes.clone(),
            MigrationPolicy::Compatible,
        )
        .await
        .unwrap();
    let replacement = controller.get_module_host(database.id).await.unwrap();
    assert_eq!(replacement.relational_db().program().unwrap().unwrap().hash, newer.hash);
    assert!(call(&replacement, "good").await.is_ok());
    drop(replacement);
    drop(module);
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn javascript_logging_handles_direct_startup_and_preserves_wrapped_call_locations() {
    use futures::TryStreamExt as _;

    let mut schema = RawModuleDefV10Builder::new();
    schema.add_reducer("log", ProductType::unit());
    let schema = spacetimedb_lib::bsatn::to_vec(&spacetimedb_lib::RawModuleDef::V10(schema.finish())).unwrap();
    // Direct top-level logging has exactly one JS frame. It previously passed
    // index1 to V8's unchecked GetFrame and could crash the entire host process.
    let source = format!(
        r#"import {{ register_hooks, console_log }} from "spacetime:sys@1.0";
console_log(2, "direct-startup");
function wrapped(message) {{ console_log(2, message); }}
wrapped("wrapped-startup");
register_hooks({{
    __describe_module__: () => new Uint8Array({schema:?}),
    __call_reducer__: () => {{
        console_log(2, "direct-reducer");
        wrapped("wrapped-reducer");
        return {{ tag: "ok" }};
    }},
}});
"#
    );
    let program = Program::from_bytes(ModuleKind::JS, source.into_bytes());
    let (_directory, controller, database) = controller_fixture(0xed08, &program, config());
    let module = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    assert!(call(&module, "log").await.is_ok());
    let chunks: Vec<_> = module
        .database_logger()
        .tail(None, false)
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
    let bytes = chunks.into_iter().flatten().collect::<Vec<_>>();
    let records = String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    for (message, line) in [
        ("direct-startup", 2),
        ("wrapped-startup", 4),
        ("direct-reducer", 8),
        ("wrapped-reducer", 9),
    ] {
        let record = records.iter().find(|record| record["message"] == message).unwrap();
        assert_eq!(record["line_number"], line, "wrong call location for {message}");
        assert!(record["filename"].as_str().is_some_and(|filename| !filename.is_empty()));
    }
}
