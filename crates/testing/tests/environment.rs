//! Actual publication and module calls exercise configuration atomicity and ABI enforcement.
use serial_test::serial;
use spacetimedb::client::{messages::SerializableMessage, OutboundMessage};
use spacetimedb::host::{FunctionArgs, ModuleHost};
use spacetimedb_client_api_messages::websocket::v1 as ws_v1;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::{bsatn, sats::product, AlgebraicValue, Identity};
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, ModuleHandle, DEFAULT_CONFIG};
use std::collections::BTreeMap;
use std::time::Duration;

type Values = BTreeMap<String, String>;

async fn sql(module: &ModuleHost, statement: &str) -> Vec<spacetimedb_lib::ProductValue> {
    spacetimedb::sql::execute::run(
        module.relational_db().clone(),
        statement.to_string(),
        AuthCtx::for_current(Identity::ZERO),
        Some(module.info.subscriptions.clone()),
        Some(module.clone()),
        &mut vec![],
    )
    .await
    .unwrap()
    .rows
}

async fn publish(handle: &ModuleHandle, values: &Values) -> ModuleHost {
    let result = handle.republish_environment(values.clone()).await.unwrap();
    assert!(result.was_successful(), "configuration publication failed");
    handle.client.module()
}

async fn read(module: &ModuleHost, key: &str) -> AlgebraicValue {
    module
        .call_procedure(
            Identity::ZERO,
            None,
            None,
            "read_environment",
            FunctionArgs::Bsatn(bsatn::to_vec(&product![key]).unwrap().into()),
        )
        .await
        .result
        .unwrap()
        .return_val
}

async fn next_message(handle: &mut ModuleHandle) -> OutboundMessage {
    tokio::time::timeout(Duration::from_secs(10), handle.recv_message())
        .await
        .expect("timed out waiting for environment subscription update")
        .expect("environment subscription disconnected")
}

async fn expect_view_update(handle: &mut ModuleHandle) {
    let message = next_message(handle).await;
    assert!(matches!(message, OutboundMessage::V1(SerializableMessage::TxUpdate(_))));
    // Replacing the one-row view must send both the old row's deletion and
    // the new row's insertion to an already connected subscriber.
    assert_eq!(message.num_rows(), Some(2));
}

async fn check_submodule_scope(handle: &mut ModuleHandle, values: &mut Values) {
    values.insert("EMPTY".into(), "root-visible".into());
    let module = publish(handle, values).await;
    assert!(module.info.module_def.reducer_by_name("lib.env_read_reducer").is_some());
    let child = module
        .call_reducer(
            Identity::ZERO,
            None,
            None,
            None,
            None,
            "lib.env_read_reducer",
            FunctionArgs::Nullary,
        )
        .await;
    assert!(child.is_err() || child.unwrap().outcome.into_result().is_err());
    for procedure in ["lib.env_read_procedure", "lib.env_read_in_tx"] {
        assert!(module.info.module_def.procedure_by_name(procedure).is_some());
        let result = module
            .call_procedure(Identity::ZERO, None, None, procedure, FunctionArgs::Nullary)
            .await;
        assert!(result.result.is_err(), "submodule procedure read the root environment");
    }
    for view in ["lib.env_read_view", "lib.env_read_sql_view", "env_read_root_sql_view"] {
        assert!(module.info.module_def.view_by_name_with_module(view).is_some());
        let result = spacetimedb::sql::execute::run(
            module.relational_db().clone(),
            format!("SELECT * FROM {view}"),
            AuthCtx::for_current(Identity::ZERO),
            Some(module.info.subscriptions.clone()),
            Some(module.clone()),
            &mut vec![],
        )
        .await;
        // Host access must be denied, independently of the general view-error
        // transport contract tracked in #5912. A failed view may yield no rows.
        match result {
            Ok(result) => assert!(result.rows.is_empty(), "forbidden view exposed rows"),
            Err(error) => {
                let error = format!("{error:#}");
                assert!(!error.contains("not found"), "view failed before dispatch: {error}");
                assert!(!error.contains("root-visible"), "view error exposed environment data");
            }
        }
    }

    // HTTP routes are root entries today. Calling an exported child callback
    // as an ordinary helper retains that root entry's authority.
    assert_eq!(
        &handle.call_http_route_get("/env-child").await.unwrap()[..],
        b"root-visible"
    );
}

// Qualification can pin locally built inputs without invoking a nested build.
// Ordinary test runs keep the existing compilation path when no pin is supplied.
fn compiled_fixture(name: &str) -> CompiledModule {
    use spacetimedb::messages::control_db::HostType;
    let input = match name {
        "environment-test" => Some(("SPACETIMEDB_ENV_RUST_MODULE", HostType::Wasm)),
        "module-test-ts" => Some(("SPACETIMEDB_ENV_TYPESCRIPT_MODULE", HostType::Js)),
        "module-test-cs" => Some(("SPACETIMEDB_ENV_CSHARP_MODULE", HostType::Wasm)),
        _ => None,
    };
    if let Some((key, host_type)) = input
        && let Some(path) = std::env::var_os(key)
    {
        let path = std::path::PathBuf::from(path);
        assert!(path.is_absolute() && path.is_file(), "invalid explicit module artifact");
        CompiledModule::from_artifact(name, host_type, path)
    } else {
        CompiledModule::compile(name, CompilationMode::Debug)
    }
}

fn exercise_fixture(name: &str) {
    let initial = if name == "environment-test" {
        Values::from([
            ("REQUIRED".into(), "initial-required".into()),
            ("MODE".into(), "ready".into()),
        ])
    } else {
        Values::new()
    };
    let compiled = compiled_fixture(name);
    compiled.with_module_async_with_environment(DEFAULT_CONFIG, initial.clone(), |mut handle| async move {
        let mut values = initial;
        for (key, expected) in [
            ("MISSING", None),
            ("EMPTY", Some("".to_string())),
            ("UTF8", Some("héllo 🌍".to_string())),
            ("NUL", Some("before\0after".to_string())),
            ("MAXIMUM", Some("é".repeat(4096))),
        ] {
            if let Some(value) = &expected {
                values.insert(key.into(), value.clone());
            }
            let module = publish(&handle, &values).await;
            let result = module
                .call_reducer(
                    Identity::ZERO,
                    None,
                    None,
                    None,
                    None,
                    "expect_environment",
                    FunctionArgs::Bsatn(bsatn::to_vec(&product![key, expected.clone()]).unwrap().into()),
                )
                .await
                .unwrap();
            result.outcome.into_result().unwrap();
            assert_eq!(read(&module, key).await, AlgebraicValue::from(expected.clone()));
            if expected.is_some() {
                values.insert(key.into(), "updated".into());
                let module = publish(&handle, &values).await;
                assert_eq!(
                    read(&module, key).await,
                    AlgebraicValue::from(Some("updated".to_string()))
                );
                values.remove(key);
                let module = publish(&handle, &values).await;
                assert_eq!(read(&module, key).await, AlgebraicValue::from(None::<String>));
            }
        }
        if name == "environment-test" {
            // Required values cannot be inherited from the previous publish,
            // and an invalid literal cannot replace the previous configuration.
            for invalid in [
                Values::new(),
                Values::from([
                    ("REQUIRED".into(), "initial-required".into()),
                    ("MODE".into(), "invalid-secret-marker".into()),
                ]),
            ] {
                let result = handle.republish_environment(invalid).await;
                assert!(result.as_ref().is_err() || !result.as_ref().unwrap().was_successful());
                assert_eq!(
                    read(&handle.client.module(), "REQUIRED").await,
                    AlgebraicValue::from(Some("initial-required".to_string()))
                );
            }
            // A same-module publish does not run init again. Its new required
            // value deliberately differs from the value init asserted.
            values.insert("REQUIRED".into(), "republished".into());
            values.insert("HANDLER".into(), "handler snapshot".into());
            let module = publish(&handle, &values).await;
            let (_, body) = module
                .call_http_handler(
                    module.info.module_def.http_handler_ids_and_defs().next().unwrap().0,
                    spacetimedb_lib::http::Request {
                        method: spacetimedb_lib::http::Method::Get,
                        headers: std::iter::empty().collect(),
                        timeout: None,
                        uri: "/environment".into(),
                        version: spacetimedb_lib::http::Version::Http11,
                    },
                    Default::default(),
                )
                .await
                .unwrap();
            assert_eq!(&body[..], b"handler snapshot");
            let view = "SELECT * FROM environment_value";
            assert_eq!(sql(&module, view).await, vec![product![None::<String>]]);
            let subscribe = ws_v1::ClientMessage::<bytes::Bytes>::Subscribe(ws_v1::Subscribe {
                query_strings: [view.into()].into(),
                request_id: 71,
            });
            handle.send(bsatn::to_vec(&subscribe).unwrap()).await.unwrap();
            let initial_update = next_message(&mut handle).await;
            assert!(matches!(
                initial_update,
                OutboundMessage::V1(SerializableMessage::Subscribe(_))
            ));
            assert_eq!(initial_update.num_rows(), Some(1));
            for value in ["first", "second"] {
                values.insert("WATCHED".into(), value.into());
                let module = publish(&handle, &values).await;
                assert_eq!(sql(&module, view).await, vec![product![Some(value.to_string())]]);
                expect_view_update(&mut handle).await;
            }
            let mut invalid = values.clone();
            invalid.insert("WATCHED".into(), "fail-view".into());
            let failed = handle.republish_environment(invalid).await;
            assert!(failed.as_ref().is_err() || !failed.as_ref().unwrap().was_successful());
            assert_eq!(
                sql(&handle.client.module(), view).await,
                vec![product![Some("second".to_string())]]
            );
            values.remove("WATCHED");
            let module = publish(&handle, &values).await;
            assert_eq!(sql(&module, view).await, vec![product![None::<String>]]);
            expect_view_update(&mut handle).await;
            values.insert("LIMIT".into(), "x".repeat(8192));
            let module = publish(&handle, &values).await;
            for _ in 0..2 {
                module
                    .call_reducer(
                        Identity::ZERO,
                        None,
                        None,
                        None,
                        None,
                        "bounded_environment_sources",
                        FunctionArgs::Nullary,
                    )
                    .await
                    .unwrap()
                    .outcome
                    .into_result()
                    .unwrap();
            }
        }
        if name == "module-test-ts" {
            check_submodule_scope(&mut handle, &mut values).await;
        }
        for key in ["UNDECLARED", "A=B"] {
            let result = handle
                .client
                .module()
                .call_reducer(
                    Identity::ZERO,
                    None,
                    None,
                    None,
                    None,
                    "expect_environment",
                    FunctionArgs::Bsatn(bsatn::to_vec(&product![key, None::<String>]).unwrap().into()),
                )
                .await;
            assert!(result.is_err() || result.unwrap().outcome.into_result().is_err());
        }
    });
}

#[test]
#[serial]
fn rust_environment_publish_is_atomic_and_reads_follow_declared_configuration() {
    exercise_fixture("environment-test");
}

#[test]
#[serial]
fn rust_module_test_environment_publish_and_checked_reads() {
    exercise_fixture("module-test");
}

#[test]
#[serial]
fn typescript_environment_publish_and_checked_reads() {
    exercise_fixture("module-test-ts");
}

#[test]
#[serial]
fn cpp_environment_publish_and_checked_reads() {
    exercise_fixture("module-test-cpp");
}

#[test]
#[serial]
fn csharp_environment_publish_and_checked_reads() {
    exercise_fixture("module-test-cs");
}

#[cfg(feature = "allow_loopback_http_for_tests")]
#[test]
#[serial]
fn suspended_procedure_cannot_read_environment_from_a_replacement_program() {
    use anyhow::Context as _;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let initial = Values::from([
        ("REQUIRED".into(), "initial-required".into()),
        ("MODE".into(), "ready".into()),
    ]);
    compiled_fixture("environment-test").with_module_async_with_environment(
        DEFAULT_CONFIG,
        initial.clone(),
        |handle| async move {
            for explicit_tx in [false, true] {
                let old = handle.client.module();
                let program = old.relational_db().program().unwrap().unwrap();
                let old_hash = program.hash;
                let mut replacement = program.bytes.to_vec();
                // A valid custom section changes the exact program hash without
                // changing the schema or behavior of this real Wasm module.
                replacement.extend_from_slice(&[0, 3, 1, b'e', u8::from(explicit_tx)]);
                let values = Values::from([
                    ("REQUIRED".into(), "new-program-value".into()),
                    ("MODE".into(), "ready".into()),
                ]);
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let url = format!("http://{}/hold", listener.local_addr().unwrap());
                let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
                let (release_tx, release_rx) = tokio::sync::oneshot::channel();
                let mut server = tokio::spawn(async move {
                    let (mut stream, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept()).await??;
                    let mut request = Vec::new();
                    while !request.ends_with(b"\r\n\r\n") {
                        anyhow::ensure!(request.len() < 4096, "test request exceeded its header bound");
                        request.push(tokio::time::timeout(Duration::from_secs(10), stream.read_u8()).await??);
                    }
                    entered_tx
                        .send(())
                        .map_err(|_| anyhow::anyhow!("test coordinator closed"))?;
                    tokio::time::timeout(Duration::from_secs(20), release_rx).await??;
                    stream
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await?;
                    stream.shutdown().await?;
                    anyhow::Ok(())
                });
                let call = old.call_procedure(
                    Identity::ZERO,
                    None,
                    None,
                    "read_environment_after_http",
                    FunctionArgs::Bsatn(bsatn::to_vec(&product![url, explicit_tx]).unwrap().into()),
                );
                let publish_while_suspended = async {
                    tokio::time::timeout(Duration::from_secs(10), entered_rx)
                        .await
                        .context("procedure never reached owned HTTP barrier")??;
                    let publish = handle.republish_program(replacement.into(), program.kind.into(), values);
                    let release_after_commit = async {
                        tokio::time::timeout(Duration::from_secs(20), async {
                            loop {
                                if old.relational_db().program()?.unwrap().hash != old_hash {
                                    return anyhow::Ok(());
                                }
                                tokio::time::sleep(Duration::from_millis(5)).await;
                            }
                        })
                        .await
                        .context("replacement program did not commit while procedure was suspended")??;
                        release_tx.send(()).map_err(|_| anyhow::anyhow!("HTTP barrier closed"))
                    };
                    // Drive publication even when it waits for the old procedure
                    // to finish; release only after the new program is committed.
                    let (published, released) = tokio::join!(publish, release_after_commit);
                    released?;
                    anyhow::ensure!(published?.was_successful(), "replacement publication failed");
                    anyhow::Ok(())
                };
                let (result, coordinated) = tokio::join!(call, publish_while_suspended);
                let server_result = match tokio::time::timeout(Duration::from_secs(35), &mut server).await {
                    Ok(result) => result.unwrap(),
                    Err(_) => {
                        server.abort();
                        let _ = server.await;
                        Err(anyhow::anyhow!("owned HTTP test server did not finish"))
                    }
                };
                // Join the owned listener and publication before reporting any
                // assertion failure, so failure cannot leave a live test host.
                coordinated.unwrap();
                server_result.unwrap();
                assert!(result.result.is_err(), "old procedure read the replacement environment");
                assert_eq!(
                    read(&handle.client.module(), "REQUIRED").await,
                    AlgebraicValue::from(Some("new-program-value".to_string()))
                );
            }
        },
    );
}

// The actual host keeps ENV as exact strings; generated Rust accessors decode
// the selected enum variant after initial publish and complete replacements.
#[test]
#[serial]
fn rust_environment_enums_preserve_exact_typed_mappings() {
    let initial = Values::from([
        ("REQUIRED".into(), "initial-required".into()),
        ("MODE".into(), "ready".into()),
    ]);
    compiled_fixture("environment-test").with_module_async_with_environment(
        DEFAULT_CONFIG,
        initial.clone(),
        |handle| async move {
            let mut values = initial;
            for (value, index) in [
                ("ready", 0u8),
                ("other", 1),
                ("in progress", 2),
                ("Ready", 3),
                ("", 4),
                ("héllo\0世界", 5),
            ] {
                values.insert("MODE".into(), value.into());
                values.insert("TYPED".into(), value.into());
                let module = publish(&handle, &values).await;
                module
                    .call_reducer(
                        Identity::ZERO,
                        None,
                        None,
                        None,
                        None,
                        "expect_typed_environment",
                        FunctionArgs::Bsatn(bsatn::to_vec(&product![index, Some(index)]).unwrap().into()),
                    )
                    .await
                    .unwrap()
                    .outcome
                    .into_result()
                    .unwrap();
                assert_eq!(
                    read(&module, "MODE").await,
                    AlgebraicValue::from(Some(value.to_string()))
                );
            }
            for rejected in ["InProgress", "READY", "in progress "] {
                let mut invalid = values.clone();
                invalid.insert("TYPED".into(), rejected.into());
                let result = handle.republish_environment(invalid).await;
                assert!(result.as_ref().is_err() || !result.as_ref().unwrap().was_successful());
                assert_eq!(
                    read(&handle.client.module(), "TYPED").await,
                    AlgebraicValue::from(Some("héllo\0世界".to_string()))
                );
            }
            values.remove("TYPED");
            let module = publish(&handle, &values).await;
            module
                .call_reducer(
                    Identity::ZERO,
                    None,
                    None,
                    None,
                    None,
                    "expect_typed_environment",
                    FunctionArgs::Bsatn(bsatn::to_vec(&product![5u8, None::<u8>]).unwrap().into()),
                )
                .await
                .unwrap()
                .outcome
                .into_result()
                .unwrap();
        },
    );
}
