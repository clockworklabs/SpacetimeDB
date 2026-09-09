//! Real local writes with only their durability acknowledgment delayed.
use super::*;
use crate::db::deployment::{
    install_container_fence, install_publication_fence, record_deployment_commit, DeploymentCommit,
};
use crate::db::relational_db::{
    local_durability,
    tests_utils::{TempReplicaDir, TestDB},
    LocalDurability,
};
use crate::db::{environment, persistence::Persistence};
use spacetimedb_datastore::system_tables::StContainerFenceRow;
use spacetimedb_durability::{Close, Durability, DurableOffset, PreparedTx};
use spacetimedb_lib::container::*;
use spacetimedb_lib::deployment::{DeploymentSpec, DeploymentSpecV1, ModuleComponent};
use spacetimedb_lib::environment::{EnvironmentConstraint, EnvironmentDeclaration, EnvironmentSchema};
use spacetimedb_lib::{hash_bytes, Timestamp, Uuid};
use std::time::Duration;

fn uuid() -> Uuid {
    Uuid::from_u128(uuid::Uuid::now_v7().as_u128())
}

fn environment_schema(keys: &[String]) -> EnvironmentSchema {
    EnvironmentSchema::new(
        keys.iter()
            .map(|name| EnvironmentDeclaration {
                name: name.clone(),
                constraint: EnvironmentConstraint::AnyString,
                optional: true,
            })
            .collect(),
    )
    .unwrap()
}

fn setup(db: &RelationalDB, keys: Vec<String>) -> EnvironmentSnapshotScope {
    let spec = ContainerSpec {
        image_manifest: OciDigest::sha256([7; 32]),
        image_platform: ImagePlatform {
            os: "linux".into(),
            architecture: "arm64".into(),
        },
        argv: vec!["/app/agent".into()],
        user: "1000:1000".into(),
        working_directory: "/app".into(),
        mode: ContainerMode::Job,
        restart: RestartPolicy::Never,
        env_keys: keys,
        resources: ContainerResources {
            cpu_millicores: 1000,
            memory_bytes: 64 * 1024 * 1024,
            scratch_bytes: 64 * 1024 * 1024,
            pids_max: 64,
        },
        ports: vec![],
        mounts: vec![],
        stop_grace_ms: DEFAULT_STOP_GRACE_MS,
    }
    .normalize(&Default::default())
    .unwrap();
    let schema = environment_schema(&spec.env_keys);
    let generated = spacetimedb_lib::deployment::system_empty::generate(&schema).unwrap();
    let request = DeploymentCommit {
        operation_id: uuid(),
        publication_epoch: 1,
        publisher: db.owner_identity(),
        expected_revision: None,
        prepared_manifest_hash: hash_bytes(b"prepared"),
        deployment: DeploymentSpec::V1(DeploymentSpecV1 {
            module: ModuleComponent::SystemEmpty(generated.descriptor),
            container: Some(spec.clone()),
        }),
    };
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<_> {
        install_publication_fence(tx, request.publication_epoch, request.operation_id)?;
        record_deployment_commit(tx, &request, Timestamp::now(), &Default::default())?;
        install_container_fence(db, tx, &self_fence(db, 1, true))?;
        environment::replace(
            db,
            tx,
            &schema,
            &spec.env_keys.iter().map(|key| (key.clone(), "before".into())).collect(),
        )?;
        Ok(())
    })
    .unwrap();
    EnvironmentSnapshotScope {
        cluster: "local-test".into(),
        database_id: 1,
        database_identity: db.database_identity(),
        node_id: 2,
        node_incarnation: uuid(),
        generation: 1,
        deployment_revision: request.deployment.revision().unwrap(),
        start_request: request.operation_id,
        env_generation: uuid(),
        env_keys: spec.env_keys,
    }
}

fn self_fence(db: &RelationalDB, generation: u64, allowed: bool) -> StContainerFenceRow {
    StContainerFenceRow {
        source_identity: db.database_identity().into(),
        generation,
        target_grant_revision: 1,
        target_set_hash: hash_bytes(b"targets"),
        allowed,
    }
}

struct DelayedAcknowledgment {
    writer: LocalDurability,
    acknowledged: DurableOffset,
}
impl Durability for DelayedAcknowledgment {
    type TxData = crate::db::relational_db::Txdata;
    fn append_tx(&self, tx: PreparedTx<Self::TxData>) {
        self.writer.append_tx(tx);
    }
    fn durable_tx_offset(&self) -> DurableOffset {
        self.acknowledged.clone()
    }
    fn close(&self) -> Close {
        self.writer.close()
    }
}

struct Fixture {
    db: Arc<RelationalDB>,
    writer: LocalDurability,
    acknowledge: tokio::sync::watch::Sender<Option<u64>>,
    _directory: TempReplicaDir,
}
impl Fixture {
    async fn new() -> Self {
        let directory = TempReplicaDir::new().unwrap();
        let (writer, disk_size) =
            local_durability((*directory).clone(), spacetimedb_runtime::Handle::tokio_current(), None)
                .await
                .unwrap();
        let (acknowledge, acknowledged) = tokio::sync::watch::channel(None);
        let db = Arc::new(
            TestDB::open_db(
                writer.as_history(),
                Some(Persistence {
                    durability: Arc::new(DelayedAcknowledgment {
                        writer: writer.clone(),
                        acknowledged: acknowledged.into(),
                    }),
                    disk_size,
                    snapshots: None,
                    runtime: spacetimedb_runtime::Handle::tokio_current(),
                }),
                None,
                0,
            )
            .unwrap(),
        );
        Self {
            db,
            writer,
            acknowledge,
            _directory: directory,
        }
    }
    async fn acknowledge_current(&self) -> u64 {
        let tx = self.db.begin_tx(Workload::ForTests);
        let (offset, metrics, reducer) = self.db.release_tx(tx);
        self.db.report_read_tx_metrics(reducer, metrics);
        let mut actual = self.writer.durable_tx_offset();
        let durable = tokio::time::timeout(Duration::from_secs(5), actual.wait_for(offset))
            .await
            .unwrap()
            .unwrap();
        self.acknowledge.send_replace(Some(durable));
        offset
    }
    async fn finish(self) {
        self.db.shutdown().await;
        drop(self.db);
        self.writer.close().await;
    }
}

async fn wait_for_slot(capacity: &Arc<Semaphore>) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while capacity.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
async fn released(capacity: &Arc<Semaphore>) {
    let permit = tokio::time::timeout(Duration::from_secs(5), capacity.clone().acquire_owned())
        .await
        .unwrap()
        .unwrap();
    drop(permit);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn container_environment_capture_cancellation_retains_slot_until_durable_acknowledgment() {
    let fixture = Fixture::new().await;
    let db = fixture.db.clone();
    let scope = setup(&db, vec!["SECRET".into()]);
    let capacity = Arc::new(Semaphore::new(1));
    let action_db = db.clone();
    let (captured, result) = tokio::sync::oneshot::channel();
    let caller = tokio::spawn(mutate_with_capacity(db.clone(), capacity.clone(), move |tx| {
        let receipt = storage::capture(&action_db, tx, &scope)?;
        captured.send(receipt.clone()).unwrap();
        Ok(receipt)
    }));
    let captured = tokio::time::timeout(Duration::from_secs(5), result)
        .await
        .unwrap()
        .unwrap();
    // This read waits for the actual capture transaction to commit.
    db.with_read_only(Workload::ForTests, |tx| storage::read(&db, tx, &captured))
        .unwrap();
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert_eq!(capacity.available_permits(), 0);
    assert!(matches!(
        mutate_with_capacity(db.clone(), capacity.clone(), |_| Ok(())).await,
        Err(EnvironmentSnapshotError::Capacity)
    ));
    fixture.acknowledge_current().await;
    released(&capacity).await;
    let values = read(db.clone(), captured).await.unwrap();
    assert_eq!(values.receipt.selected_values["SECRET"], "before");
    drop(db);
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn container_environment_read_cancellation_retains_slot_until_durable_acknowledgment() {
    let fixture = Fixture::new().await;
    let db = fixture.db.clone();
    let scope = setup(&db, vec!["SECRET".into()]);
    let receipt = db
        .with_auto_commit(Workload::ForTests, |tx| storage::capture(&db, tx, &scope))
        .unwrap();
    let capacity = Arc::new(Semaphore::new(1));
    let caller = tokio::spawn(read_with_capacity(db.clone(), receipt.clone(), capacity.clone()));
    wait_for_slot(&capacity).await;
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert_eq!(capacity.available_permits(), 0);
    assert!(matches!(
        read_with_capacity(db.clone(), receipt.clone(), capacity.clone()).await,
        Err(EnvironmentSnapshotError::Capacity)
    ));
    fixture.acknowledge_current().await;
    released(&capacity).await;
    let values = read_with_capacity(db.clone(), receipt, capacity).await.unwrap();
    assert_eq!(values.receipt.selected_values["SECRET"], "before");
    drop(db);
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn container_environment_close_cancellation_retains_slot_until_durable_acknowledgment() {
    let fixture = Fixture::new().await;
    let db = fixture.db.clone();
    let scope = setup(&db, vec!["SECRET".into()]);
    let receipt = db
        .with_auto_commit(Workload::ForTests, |tx| storage::capture(&db, tx, &scope))
        .unwrap();
    db.with_auto_commit(Workload::ForTests, |tx| {
        install_container_fence(&db, tx, &self_fence(&db, 2, false))
    })
    .unwrap();
    let capacity = Arc::new(Semaphore::new(1));
    let action_db = db.clone();
    let (closed, result) = tokio::sync::oneshot::channel();
    let caller = tokio::spawn(mutate_with_capacity(db.clone(), capacity.clone(), move |tx| {
        let receipt = storage::close(&action_db, tx, &scope, 2)?;
        closed.send(()).unwrap();
        Ok(receipt)
    }));
    tokio::time::timeout(Duration::from_secs(5), result)
        .await
        .unwrap()
        .unwrap();
    db.with_read_only(Workload::ForTests, |tx| {
        use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
        assert_eq!(
            tx.table_row_count(spacetimedb_datastore::system_tables::ST_CONTAINER_ENVIRONMENT_ID),
            Some(0)
        );
    });
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert_eq!(capacity.available_permits(), 0);
    assert!(matches!(
        mutate_with_capacity(db.clone(), capacity.clone(), |_| Ok(())).await,
        Err(EnvironmentSnapshotError::Capacity)
    ));
    fixture.acknowledge_current().await;
    released(&capacity).await;
    assert!(matches!(
        read(db.clone(), receipt).await,
        Err(EnvironmentSnapshotError::Fenced)
    ));
    drop(db);
    fixture.finish().await;
}
