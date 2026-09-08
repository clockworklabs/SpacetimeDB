//! Actual local database writers, Wasm/JS worker queues and cancelled callers.
use super::*;
use crate::db::persistence::LocalPersistenceProvider;
use crate::host::host_controller::{HostController, HostRuntimeConfig};
use crate::util::jobs::JobCores;
use spacetimedb_auth::identity::SpacetimeIdentityClaims;
use spacetimedb_datastore::system_tables::ModuleKind;
use spacetimedb_lib::db::raw_def::v10::RawModuleDefV10Builder;
use spacetimedb_paths::{server::ServerDataDir, FromPathUnchecked};
use tokio::time::timeout;

fn javascript() -> Program {
    let mut definition = RawModuleDefV10Builder::new();
    definition.add_lifecycle_reducer(
        Lifecycle::OnDisconnect,
        "disconnected",
        spacetimedb_sats::ProductType::unit(),
    );
    let raw = bsatn::to_vec(&spacetimedb_lib::RawModuleDef::V10(definition.finish())).unwrap();
    Program::from_bytes(
        ModuleKind::JS,
        format!(
            r#"
        import {{register_hooks}} from "spacetime:sys@1.0";
        register_hooks({{__describe_module__: () => new Uint8Array({raw:?}),
            __call_reducer__: () => ({{tag:"ok"}}) }});
    "#
        )
        .into_bytes(),
    )
}

fn fixture(id: u64, program: Program) -> (tempfile::TempDir, HostController, Database) {
    let directory = tempfile::tempdir().unwrap();
    let data = Arc::new(ServerDataDir::from_path_unchecked(directory.path().to_owned()));
    let initial = program.clone();
    let controller = HostController::new(
        data.clone(),
        crate::db::Config {
            storage: crate::db::Storage::Disk,
            page_pool_max_size: None,
        },
        HostRuntimeConfig::default(),
        Arc::new(move |hash| {
            let program = initial.clone();
            async move { Ok((program.hash == hash).then_some(program.bytes)) }
        }),
        Arc::new(crate::energy::NullEnergyMonitor),
        Arc::new(()),
        Arc::new(LocalPersistenceProvider::new(data)),
        JobCores::without_pinned_cores(),
    );
    let database = Database {
        id,
        database_identity: Identity::from_u256(id.into()),
        owner_identity: Identity::ONE,
        host_type: if program.kind == ModuleKind::JS {
            HostType::Js
        } else {
            HostType::Wasm
        },
        initial_program: program.hash,
        bootstrap_generation: 0,
    };
    (directory, controller, database)
}

async fn until(mut condition: impl FnMut() -> bool) {
    timeout(Duration::from_secs(5), async {
        while !condition() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

fn client_auth() -> ConnectionAuthCtx {
    SpacetimeIdentityClaims {
        identity: Identity::ONE,
        subject: "local-test".into(),
        issuer: "local-test".into(),
        audience: Box::new([]),
        iat: std::time::SystemTime::now(),
        exp: None,
        extra: None,
    }
    .try_into()
    .unwrap()
}

async fn queued_disconnect(js: bool, cancel: bool, id: u64) {
    let program = if js {
        javascript()
    } else {
        crate::host::empty_module::program(1).unwrap()
    };
    let (_directory, controller, database) = fixture(id, program);
    let module = controller
        .get_or_launch_module_host(database.clone(), id)
        .await
        .unwrap();
    let client = ClientActorId::for_test(Identity::ONE);
    module
        .call_identity_connected(client_auth(), client.connection_id)
        .await
        .unwrap();
    let db = module.relational_db().clone();
    let tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    assert!(tx.st_client_row(client.identity, client.connection_id).is_some());
    let call = tokio::spawn({
        let module = module.clone();
        async move { module.disconnect_client(client).await }
    });
    until(|| module.operations.active() == 1).await;
    let closing = tokio::spawn({
        let controller = controller.clone();
        async move { controller.exit_module_host(id, Duration::from_secs(5)).await }
    });
    until(|| module.operations.is_closed()).await;
    if cancel {
        call.abort();
        assert!(call.await.unwrap_err().is_cancelled());
    } else {
        drop(call);
    }
    assert_eq!(module.operations.active(), 1);
    assert!(!closing.is_finished());
    let _ = db.rollback_mut_tx(tx);
    timeout(Duration::from_secs(5), closing)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    assert!(tx.st_client_row(client.identity, client.connection_id).is_none());
    let _ = db.rollback_mut_tx(tx);
    assert!(module.clear_all_clients().await.is_err());
    let result = crate::sql::execute::run(
        db.clone(),
        "SELECT * FROM st_client".into(),
        AuthCtx::new(db.owner_identity(), db.owner_identity()),
        None,
        None,
        &mut Vec::new(),
    )
    .await;
    assert!(matches!(result, Err(DBError::DatabaseClosed)));
    assert!(controller.get_module_host(id).await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn operation_drain_wasm_queued_disconnect_survives_cancellation() {
    queued_disconnect(false, true, 0xd100).await;
    queued_disconnect(false, false, 0xd101).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn operation_drain_js_queued_disconnect_survives_cancellation() {
    queued_disconnect(true, true, 0xd102).await;
    queued_disconnect(true, false, 0xd103).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn operation_drain_wasm_procedure_owns_external_wait_and_late_commit() {
    let id = 0xd104;
    let (_directory, controller, database) = fixture(id, crate::host::empty_module::program(1).unwrap());
    let module = controller.get_or_launch_module_host(database, id).await.unwrap();
    let db = module.relational_db().clone();
    let started = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    let call = tokio::spawn({
        let module = module.clone();
        let db = db.clone();
        let started = started.clone();
        let release = release.clone();
        async move {
            module
                .call_pooled(
                    "external-wait-test",
                    (),
                    async move |_, _| {
                        started.add_permits(1);
                        release.acquire().await.unwrap().forget();
                        db.with_auto_commit(Workload::Internal, |tx| {
                            crate::db::environment::set(&db, tx, "AFTER_IO", "committed").map_err(anyhow::Error::from)
                        })
                        .unwrap();
                    },
                    async |_, _| unreachable!(),
                )
                .await
                .unwrap();
        }
    });
    timeout(Duration::from_secs(5), started.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();
    call.abort();
    assert!(call.await.unwrap_err().is_cancelled());
    let closing = tokio::spawn({
        let controller = controller.clone();
        async move { controller.exit_module_host(id, Duration::from_secs(5)).await }
    });
    until(|| module.operations.is_closed()).await;
    assert_eq!(module.operations.active(), 1);
    assert!(!closing.is_finished());
    release.add_permits(1);
    timeout(Duration::from_secs(5), closing)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let tx = db.begin_tx(Workload::Internal);
    assert_eq!(
        crate::db::environment::get(&tx, "AFTER_IO").unwrap().as_deref(),
        Some("committed")
    );
    let _ = db.release_tx(tx);
}

/// Opt in only for the existing loopback test feature. Production IP filtering
/// stays enabled. Run with proxy variables cleared, as asserted below.
#[cfg(feature = "allow_loopback_http_for_tests")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn operation_drain_js_procedure_owns_actual_http_until_late_commit() {
    use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for variable in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        assert!(
            std::env::var_os(variable).is_none(),
            "clear {variable} for this disposable loopback test"
        );
    }
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    assert!(address.ip().is_loopback());
    let started = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    let server = tokio::spawn({
        let started = started.clone();
        let release = release.clone();
        async move {
            let (mut socket, peer) = listener.accept().await.unwrap();
            assert!(peer.ip().is_loopback());
            let mut request = [0u8; 4096];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            started.add_permits(1);
            release.acquire().await.unwrap().forget();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
        }
    });
    let request = bsatn::to_vec(&spacetimedb_lib::http::Request {
        method: spacetimedb_lib::http::Method::Get,
        headers: std::iter::empty().collect(),
        timeout: None,
        uri: format!("http://{address}/owned-test"),
        version: spacetimedb_lib::http::Version::Http11,
    })
    .unwrap();
    let mut schema = RawModuleDefV10Builder::new();
    schema
        .build_table_with_new_type("rows", [("value", AlgebraicType::U64)], true)
        .finish();
    schema.add_procedure("task", spacetimedb_sats::ProductType::unit(), AlgebraicType::U64);
    let raw = bsatn::to_vec(&spacetimedb_lib::RawModuleDef::V10(schema.finish())).unwrap();
    let program = Program::from_bytes(ModuleKind::JS, format!(r#"
        import {{register_hooks,table_id_from_name,datastore_insert_bsatn}} from "spacetime:sys@1.0";
        import {{register_hooks as procedures,procedure_http_request,procedure_start_mut_tx,procedure_commit_mut_tx}} from "spacetime:sys@1.2";
        register_hooks({{__describe_module__: () => new Uint8Array({raw:?}), __call_reducer__: () => ({{tag:"ok"}})}});
        procedures({{__call_procedure__: () => {{
            procedure_http_request(new Uint8Array({request:?}), "");
            procedure_start_mut_tx();
            datastore_insert_bsatn(table_id_from_name("rows"), new Uint8Array([42,0,0,0,0,0,0,0]));
            procedure_commit_mut_tx();
            return new Uint8Array(8);
        }} }});
    "#).into_bytes());
    let id = 0xd105;
    let (_directory, controller, database) = fixture(id, program);
    let module = controller.get_or_launch_module_host(database, id).await.unwrap();
    let db = module.relational_db().clone();
    let call = tokio::spawn({
        let module = module.clone();
        async move {
            module
                .call_procedure(Identity::ONE, None, None, "task", FunctionArgs::Nullary)
                .await
        }
    });
    timeout(Duration::from_secs(10), started.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();
    call.abort();
    assert!(call.await.unwrap_err().is_cancelled());
    let closing = tokio::spawn({
        let controller = controller.clone();
        async move { controller.exit_module_host(id, Duration::from_secs(10)).await }
    });
    until(|| module.operations.is_closed()).await;
    assert_eq!(module.operations.active(), 1);
    assert!(!closing.is_finished());
    release.add_permits(1);
    timeout(Duration::from_secs(10), closing)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    server.await.unwrap();
    let tx = db.begin_tx(Workload::Internal);
    let table = db.table_id_from_name(&tx, "rows").unwrap().unwrap();
    assert_eq!(tx.table_row_count(table), Some(1));
    let _ = db.release_tx(tx);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn operation_drain_cancelled_js_procedure_startup_retains_physical_slot() {
    use futures::StreamExt;
    let mut schema = RawModuleDefV10Builder::new();
    schema.add_procedure("task", spacetimedb_sats::ProductType::unit(), AlgebraicType::U64);
    let raw = bsatn::to_vec(&spacetimedb_lib::RawModuleDef::V10(schema.finish())).unwrap();
    let program = Program::from_bytes(
        ModuleKind::JS,
        format!(
            r#"
        import {{register_hooks,console_log}} from "spacetime:sys@1.0";
        import {{register_hooks as procedures}} from "spacetime:sys@1.2";
        function startup() {{ console_log(2, "physical-startup-entered"); }}
        startup();
        const until = Date.now() + 2000;
        while (Date.now() < until) {{}}
        register_hooks({{__describe_module__: () => new Uint8Array({raw:?}), __call_reducer__: () => ({{tag:"ok"}})}});
        procedures({{__call_procedure__: () => new Uint8Array(8)}});
    "#
        )
        .into_bytes(),
    );
    let id = 0xd106;
    let (_directory, controller, database) = fixture(id, program);
    let module = controller.get_or_launch_module_host(database, id).await.unwrap();
    let ModuleHostInner::Js(host) = &*module.inner else {
        unreachable!()
    };
    let slots = host.procedure_instances.instance_slots.as_ref().unwrap();
    let maximum = slots.available_permits();
    let mut logs = module.database_logger().tail(Some(0), true).await.unwrap();
    let call = tokio::spawn({
        let module = module.clone();
        async move {
            module
                .call_procedure(Identity::ONE, None, None, "task", FunctionArgs::Nullary)
                .await
        }
    });
    timeout(Duration::from_secs(5), async {
        loop {
            let log = logs.next().await.unwrap().unwrap();
            if String::from_utf8_lossy(&log).contains("physical-startup-entered") {
                break;
            }
        }
    })
    .await
    .unwrap();
    call.abort();
    assert!(call.await.unwrap_err().is_cancelled());
    assert_eq!(module.operations.active(), 1);
    assert_eq!(slots.available_permits(), maximum - 1);
    let closing = tokio::spawn({
        let controller = controller.clone();
        async move { controller.exit_module_host(id, Duration::from_secs(5)).await }
    });
    until(|| module.operations.is_closed()).await;
    assert!(!closing.is_finished());
    timeout(Duration::from_secs(5), closing)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(slots.available_permits(), maximum);
}
