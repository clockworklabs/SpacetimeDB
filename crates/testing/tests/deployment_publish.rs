//! Exercise publication against the real host and Wasm module, including the
//! unchanged-program path and retries after a later module has been installed.
use serial_test::serial;
use spacetimedb::db::deployment::{current_deployment, install_publication_fence, DeploymentCommit};
use spacetimedb::host::{FunctionArgs, UpdateDatabaseResult};
use spacetimedb::messages::control_db::HostType;
use spacetimedb_client_api::{ControlStateReadAccess, NodeDelegate};
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::system_tables::ST_DEPLOYMENT_OPERATION_ID;
use spacetimedb_lib::container::{
    ContainerMode, ContainerResources, ContainerSpec, ImagePlatform, OciDigest, RestartPolicy,
};
use spacetimedb_lib::deployment::{DeploymentSpec, DeploymentSpecV1, ModuleComponent, UserModule, UserModuleKind};
use spacetimedb_lib::{hash_bytes, sats::product, ConnectionId, Identity, Uuid};
use spacetimedb_schema::auto_migrate::{MigrationPolicy, MigrationToken};
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, DEFAULT_CONFIG};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn prepared(bytes: &[u8], epoch: u64, previous: Option<&DeploymentCommit>, command: &str) -> DeploymentCommit {
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    DeploymentCommit {
        operation_id: Uuid::from_u128((now_ms << 80) | (0x7000u128 << 64) | (0x8000u128 << 48) | u128::from(epoch)),
        publication_epoch: epoch,
        publisher: Identity::ZERO,
        expected_revision: previous.map(|request| request.deployment.revision().unwrap()),
        expected_last_operation: previous.map(|request| request.operation_id),
        prepared_manifest_hash: hash_bytes(epoch.to_le_bytes()),
        deployment: DeploymentSpec::V1(DeploymentSpecV1 {
            module: ModuleComponent::User(UserModule {
                kind: UserModuleKind::Wasm,
                program_hash: hash_bytes(bytes),
            }),
            container: Some(ContainerSpec {
                image_manifest: OciDigest::sha256([3; 32]),
                image_platform: ImagePlatform {
                    os: "linux".into(),
                    architecture: "arm64".into(),
                },
                argv: vec![command.into()],
                user: "1000:1000".into(),
                working_directory: "/app".into(),
                mode: ContainerMode::Service,
                restart: RestartPolicy::OnFailure,
                env_keys: vec![],
                resources: ContainerResources {
                    cpu_millicores: 1000,
                    memory_bytes: 64 * 1024 * 1024,
                    scratch_bytes: 64 * 1024 * 1024,
                    pids_max: 64,
                },
                ports: vec![],
                mounts: vec![],
                stop_grace_ms: 1000,
            }),
        }),
    }
}

fn fence(module: &spacetimedb::host::ModuleHost, request: &DeploymentCommit) {
    module
        .relational_db()
        .with_auto_commit(Workload::Internal, |tx| {
            install_publication_fence(tx, request.publication_epoch, request.operation_id)
        })
        .unwrap();
}

async fn confirmed(result: UpdateDatabaseResult) {
    let (offset, durable) = match result {
        UpdateDatabaseResult::UpdatePerformed {
            tx_offset,
            durable_offset,
        }
        | UpdateDatabaseResult::UpdatePerformedWithClientDisconnect {
            tx_offset,
            durable_offset,
        }
        | UpdateDatabaseResult::DeploymentAlreadyCommitted {
            tx_offset,
            durable_offset,
            ..
        } => (tx_offset, durable_offset),
        other => panic!("expected a durable publication result, got {other:?}"),
    };
    tokio::time::timeout(Duration::from_secs(10), async {
        let offset = offset.await.unwrap();
        if let Some(mut durable) = durable {
            durable.wait_for(offset).await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[test]
#[serial]
fn container_only_changes_and_old_retries_preserve_the_current_module() {
    let compiled = CompiledModule::compile("hosted-auth-test", CompilationMode::Debug);
    let bytes = compiled.program_bytes();
    compiled.with_module_async(DEFAULT_CONFIG, |handle| async move {
        let env = handle.environment();
        let database = env.get_database_by_identity(&handle.db_identity).await.unwrap().unwrap();
        let host = env.leader(database.id).await.unwrap();
        let module = host.module().await.unwrap();
        let first = prepared(&bytes, 1, None, "/app/first");
        fence(&module, &first);
        // Even the first in-progress container publication fences the legacy
        // raw API, including a byte-identical module update.
        assert!(host.update(database.clone(), HostType::Wasm, bytes.to_vec().into(), MigrationPolicy::Compatible).await.is_err());
        // The rejected updater must restore the old host in its controller.
        assert_eq!(host.module().await.unwrap().info.module_hash, hash_bytes(&bytes));
        let result = host.update_with_deployment(database.clone(), HostType::Wasm, bytes.to_vec().into(), MigrationPolicy::Compatible, first.clone()).await.unwrap();
        confirmed(result).await;
        let first_revision = first.deployment.revision().unwrap();
        let second = prepared(&bytes, 2, Some(&first), "/app/second");
        fence(&host.module().await.unwrap(), &second);
        confirmed(host.update_with_deployment(database.clone(), HostType::Wasm, bytes.to_vec().into(), MigrationPolicy::Compatible, second.clone()).await.unwrap()).await;
        assert_ne!(first_revision, second.deployment.revision().unwrap());

        // A valid custom section changes program bytes/hash without changing
        // its definition. This exercises the actual Wasm migration and swap.
        let mut newer_bytes = bytes.to_vec();
        newer_bytes.extend_from_slice(&[0, 3, 1, b'x', 1]);
        let third = prepared(&newer_bytes, 3, Some(&second), "/app/third");
        fence(&host.module().await.unwrap(), &third);
        confirmed(host.update_with_deployment(database.clone(), HostType::Wasm, newer_bytes.clone().into(), MigrationPolicy::Compatible, third.clone()).await.unwrap()).await;
        let newest = third.deployment.revision().unwrap();
        assert_eq!(host.module().await.unwrap().info.module_hash, hash_bytes(&newer_bytes));
        let before_retry = host.module().await.unwrap();
        let result = host.update_with_deployment(database.clone(), HostType::Wasm, bytes.to_vec().into(), MigrationPolicy::Compatible, first.clone()).await.unwrap();
        assert!(matches!(&result, UpdateDatabaseResult::DeploymentAlreadyCommitted { result, .. } if result.revision == first_revision));
        confirmed(result).await;
        let current = host.module().await.unwrap();
        assert_eq!(current.info.module_hash, before_retry.info.module_hash);
        current.relational_db().with_read_only(Workload::Internal, |tx| {
            assert_eq!(current_deployment(tx).unwrap().unwrap().0, newest);
            assert_eq!(tx.table_row_count(ST_DEPLOYMENT_OPERATION_ID), Some(3));
        });
        current.call_reducer(Identity::ZERO, None, None, None, None, "private_only", FunctionArgs::Nullary).await.unwrap().outcome.into_result().unwrap();

        // A stale CAS and a forged association between bytes and declaration
        // cannot change either component, and rejection leaves service usable.
        let stale = prepared(&bytes, 4, Some(&first), "/app/stale");
        fence(&current, &stale);
        assert!(host.update_with_deployment(database.clone(), HostType::Wasm, bytes.to_vec().into(), MigrationPolicy::Compatible, stale).await.is_err());
        let mismatched = prepared(&bytes, 5, Some(&third), "/app/mismatch");
        fence(&current, &mismatched);
        assert!(host.update_with_deployment(database.clone(), HostType::Wasm, newer_bytes.clone().into(), MigrationPolicy::Compatible, mismatched).await.is_err());
        let current = host.module().await.unwrap();
        assert_eq!(current.info.module_hash, hash_bytes(&newer_bytes));
        current.relational_db().with_read_only(Workload::Internal, |tx| {
            assert_eq!(current_deployment(tx).unwrap().unwrap().0, newest);
            assert_eq!(tx.table_row_count(ST_DEPLOYMENT_OPERATION_ID), Some(3));
        });

        let empty = spacetimedb::host::empty_module::program(spacetimedb::host::empty_module::VERSION).unwrap();
        let mut remove_module = prepared(&empty.bytes, 6, Some(&third), "/app/image-only");
        let DeploymentSpec::V1(spec) = &mut remove_module.deployment;
        spec.module = ModuleComponent::SystemEmpty(spacetimedb_lib::deployment::system_empty::empty().descriptor);
        // An init request cannot execute this instance's schema while storing
        // the bytes of a different valid, prepared program.
        assert!(current.init_database_with_deployment(empty.clone(), Some(remove_module.clone())).await.is_err());

        // Removing a nonempty table fails during migration execution, after
        // the tentative program and deployment/receipt writes. All roll back.
        let observation_name = current.info.module_def.tables()
            .find(|table| table.name.to_ascii_lowercase().contains("observation"))
            .unwrap().name.to_string();
        let observation_table = current.relational_db().with_auto_commit(Workload::Internal, |tx| -> anyhow::Result<_> {
            let table = tx.table_id_from_name(&observation_name)?.unwrap();
            tx.insert_via_serialize_bsatn(table, &product![ConnectionId::from_u128(700), Identity::ZERO, false, Identity::ZERO, false])?;
            Ok(table)
        }).unwrap();
        let policy = MigrationPolicy::BreakClients(MigrationToken {
            database_identity: handle.db_identity,
            old_module_hash: current.info.module_hash,
            new_module_hash: empty.hash,
        }.hash());
        fence(&current, &remove_module);
        let result = host.update_with_deployment(database.clone(), HostType::Wasm, empty.bytes.clone(), policy.clone(), remove_module.clone()).await.unwrap();
        assert!(matches!(result, UpdateDatabaseResult::ErrorExecutingMigration(ref error) if error.to_string().contains("table contains data")), "{result:?}");
        let current = host.module().await.unwrap();
        assert_eq!(current.info.module_hash, hash_bytes(&newer_bytes));
        assert_eq!(current.relational_db().program().unwrap().unwrap().hash, hash_bytes(&newer_bytes));
        current.relational_db().with_read_only(Workload::Internal, |tx| {
            assert_eq!(current_deployment(tx).unwrap().unwrap().0, newest);
            assert_eq!(tx.table_row_count(ST_DEPLOYMENT_OPERATION_ID), Some(3));
            assert_eq!(tx.table_row_count(observation_table), Some(1));
        });
        current.relational_db().with_auto_commit(Workload::Internal, |tx| -> anyhow::Result<()> {
            tx.clear_table(observation_table)?;
            Ok(())
        }).unwrap();
        // The same operation remains eligible after an execution rollback.
        confirmed(host.update_with_deployment(database, HostType::Wasm, empty.bytes.clone(), policy, remove_module.clone()).await.unwrap()).await;
        let current = host.module().await.unwrap();
        assert!(current.info.module_def.tables().next().is_none());
        assert!(spacetimedb::host::empty_module::matches_program(&spacetimedb_lib::deployment::system_empty::empty().descriptor, &current.relational_db().program().unwrap().unwrap()));
        current.relational_db().with_read_only(Workload::Internal, |tx| {
            assert_eq!(current_deployment(tx).unwrap().unwrap().0, remove_module.deployment.revision().unwrap());
            assert_eq!(tx.table_row_count(ST_DEPLOYMENT_OPERATION_ID), Some(4));
        });
    });
}
