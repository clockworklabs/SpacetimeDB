//! Exercise Rust Wasm bindings and host admission using actual signed proofs.
use serial_test::serial;
use spacetimedb::auth::hosted_tokens::{sign_hosted_token, HostedTokenBinding, HostedTokenValidator};
use spacetimedb::auth::invocation::InvocationCaller;
use spacetimedb::auth::JwtKeys;
use spacetimedb::db::deployment::install_container_fence;
use spacetimedb::host::{FunctionArgs, ModuleHost};
use spacetimedb_auth::identity::ConnectionAuthCtx;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::system_tables::{
    StContainerFenceRow, ST_CLIENT_ID, ST_CONNECTION_AUTH_ID, ST_CONNECTION_CREDENTIALS_ID,
};
use spacetimedb_lib::sats::{product, AlgebraicValue, ProductValue};
use spacetimedb_lib::{bsatn, ConnectionId, Identity};
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, DEFAULT_CONFIG};
use std::time::{Duration, SystemTime};

fn authenticate(source: Identity, target: Identity, lifetime: Duration) -> ConnectionAuthCtx {
    let keys = JwtKeys::generate().unwrap();
    let now = SystemTime::now();
    let binding = HostedTokenBinding {
        source_database: source,
        target_database: target,
        generation: 1,
        grant_revision: 1,
        lease_expires_at: now + Duration::from_secs(30),
    };
    let token = sign_hosted_token(
        &keys.private,
        "test.platform",
        &binding,
        now,
        now + lifetime,
        "wasm-integration",
    )
    .unwrap();
    HostedTokenValidator::new([("test.platform".into(), keys.public)])
        .unwrap()
        .validate_token(&token, target, now, |issuer, requested_source, requested_target| {
            (issuer == "test.platform" && requested_source == source && requested_target == target).then_some(binding)
        })
        .unwrap()
        .into_connection_auth()
        .unwrap()
}

fn install_fence(module: &ModuleHost, source: Identity, generation: u64, allowed: bool) {
    let db = module.relational_db();
    db.with_auto_commit(Workload::Internal, |tx| -> anyhow::Result<()> {
        install_container_fence(
            db,
            tx,
            &StContainerFenceRow {
                source_identity: source.into(),
                generation,
                target_grant_revision: generation,
                target_set_hash: spacetimedb_lib::hash_bytes(b"configured targets"),
                allowed,
            },
        )?;
        Ok(())
    })
    .unwrap();
}

fn assert_connection_count(module: &ModuleHost, expected: u64) {
    module
        .relational_db()
        .with_auto_commit(Workload::Internal, |tx| -> anyhow::Result<()> {
            for table in [ST_CLIENT_ID, ST_CONNECTION_CREDENTIALS_ID, ST_CONNECTION_AUTH_ID] {
                assert_eq!(tx.table_row_count(table), Some(expected));
            }
            Ok(())
        })
        .unwrap();
}

fn arguments(values: ProductValue) -> FunctionArgs {
    FunctionArgs::Bsatn(bsatn::to_vec(&values).unwrap().into())
}

async fn call(
    module: &ModuleHost,
    caller: impl Into<InvocationCaller> + Send,
    connection: Option<ConnectionId>,
    reducer: &str,
    args: ProductValue,
) -> anyhow::Result<()> {
    module
        .call_reducer(caller, connection, None, None, None, reducer, arguments(args))
        .await?
        .outcome
        .into_result()
}

#[test]
#[serial]
fn hosted_wasm_calls_preserve_authority_and_disconnect_after_revocation() {
    CompiledModule::compile("hosted-auth-test", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |handle| async move {
            let module = handle.client.module();
            let target = handle.db_identity;
            let foreign = Identity::ONE;
            let owner = Identity::ZERO;
            let self_connection = ConnectionId::from_u128(101);
            let foreign_connection = ConnectionId::from_u128(102);
            let failed_disconnect = ConnectionId::from_u128(999);
            let expired_connection = ConnectionId::from_u128(103);
            install_fence(&module, target, 1, true);
            install_fence(&module, foreign, 1, true);
            let self_auth = authenticate(target, target, Duration::from_secs(30));
            let foreign_auth = authenticate(foreign, target, Duration::from_secs(30));
            let expiring_auth = authenticate(target, target, Duration::from_secs(3));
            for (auth, connection) in [
                (self_auth.clone(), self_connection),
                (foreign_auth.clone(), foreign_connection),
                (self_auth.clone(), failed_disconnect),
                (expiring_auth.clone(), expired_connection),
            ] {
                module.call_identity_connected(auth, connection).await.unwrap();
            }
            assert_connection_count(&module, 4);
            for (auth, sender, connection, internal) in [
                (&self_auth, target, self_connection, true),
                (&foreign_auth, foreign, foreign_connection, false),
            ] {
                call(
                    &module,
                    auth,
                    Some(connection),
                    "inspect_context",
                    product![sender, Some(connection), internal, true],
                )
                .await
                .unwrap();
                let result = module
                    .call_procedure(
                        auth,
                        Some(connection),
                        None,
                        "inspect_procedure",
                        arguments(product![sender, Some(connection), internal]),
                    )
                    .await;
                assert_eq!(result.result.unwrap().return_val, AlgebraicValue::Bool(true));
                for lifecycle in ["connected", "disconnected"] {
                    assert!(call(&module, auth, Some(connection), lifecycle, product![])
                        .await
                        .is_err());
                }
            }
            // Ordinary owner calls and equal database identities remain external.
            for ordinary in [target, owner] {
                call(
                    &module,
                    ordinary,
                    None,
                    "inspect_context",
                    product![ordinary, Option::<ConnectionId>::None, false, false],
                )
                .await
                .unwrap();
                assert!(call(&module, ordinary, None, "internal_only", product![])
                    .await
                    .is_err());
            }
            call(&module, owner, None, "private_only", product![]).await.unwrap();
            call(&module, &self_auth, Some(self_connection), "internal_only", product![])
                .await
                .unwrap();
            call(&module, &self_auth, Some(self_connection), "private_only", product![])
                .await
                .unwrap();
            for reducer in ["internal_only", "private_only"] {
                assert!(
                    call(&module, &foreign_auth, Some(foreign_connection), reducer, product![])
                        .await
                        .is_err()
                );
            }
            call(&module, owner, None, "schedule_check", product![]).await.unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let result = module
                        .call_procedure(owner, None, None, "scheduled_finished", FunctionArgs::Nullary)
                        .await;
                    if result.result.unwrap().return_val == AlgebraicValue::Bool(true) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("scheduled call did not observe internal authority");
            // Tokens remain cryptographically valid, but their persisted grants are revoked.
            install_fence(&module, target, 2, false);
            install_fence(&module, foreign, 2, false);
            assert!(
                call(&module, &self_auth, Some(self_connection), "internal_only", product![])
                    .await
                    .is_err()
            );
            let result = module
                .call_procedure(
                    &self_auth,
                    Some(self_connection),
                    None,
                    "inspect_procedure",
                    arguments(product![target, Some(self_connection), true]),
                )
                .await;
            assert!(result.result.is_err());
            if let Ok(remaining) = expiring_auth
                .hosted
                .as_ref()
                .unwrap()
                .expires_at()
                .duration_since(SystemTime::now())
            {
                tokio::time::sleep(remaining + Duration::from_millis(20)).await;
            }
            assert!(call(
                &module,
                &expiring_auth,
                Some(expired_connection),
                "internal_only",
                product![]
            )
            .await
            .is_err());
            // Host cleanup retains captured flags and JWT sender after revocation and expiry.
            for (sender, connection) in [
                (target, self_connection),
                (foreign, foreign_connection),
                (target, failed_disconnect),
                (target, expired_connection),
            ] {
                module.call_identity_disconnected(sender, connection).await.unwrap();
            }
            assert_connection_count(&module, 0);
            for (connection, sender, internal, disconnected) in [
                (self_connection, target, true, true),
                (foreign_connection, foreign, false, true),
                (failed_disconnect, target, true, false),
                (expired_connection, target, true, true),
            ] {
                call(
                    &module,
                    owner,
                    None,
                    "inspect_observation",
                    product![connection, sender, internal, disconnected],
                )
                .await
                .unwrap();
            }
        },
    );
}
