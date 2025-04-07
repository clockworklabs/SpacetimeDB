use std::sync::Arc;

use env_logger::Env;
use pretty_assertions::assert_matches;
use spacetimedb::{
    db::{
        datastore::locking_tx_datastore::datastore::Locking,
        relational_db::{
            tests_utils::{TempReplicaDir, TestDB},
            SNAPSHOT_FREQUENCY,
        },
    },
    error::DBError,
    execution_context::Workload,
    Identity,
};
use spacetimedb_durability::{EmptyHistory, TxOffset};
use spacetimedb_lib::{
    bsatn,
    db::raw_def::v9::{RawModuleDefV9Builder, RawTableDefBuilder},
    AlgebraicType, ProductType,
};
use spacetimedb_paths::{server::SnapshotsPath, FromPathUnchecked};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::product;
use spacetimedb_schema::{
    def::ModuleDef,
    schema::{Schema as _, TableSchema},
};
use spacetimedb_snapshot::{remote::synchronize_snapshot, Snapshot, SnapshotError, SnapshotRepository};
use tempfile::tempdir;
use tokio::task::spawn_blocking;

// TODO: Happy path for compressed snapshot, pending #2034
#[tokio::test]
async fn can_sync_a_snapshot() -> anyhow::Result<()> {
    enable_logging();
    let tmp = tempdir()?;

    let src_path = SnapshotsPath::from_path_unchecked(tmp.path().join("src"));
    let dst_path = SnapshotsPath::from_path_unchecked(tmp.path().join("dst"));

    src_path.create()?;
    dst_path.create()?;

    let src_repo = SnapshotRepository::open(src_path, Identity::ZERO, 0).map(Arc::new)?;
    let dst_repo = SnapshotRepository::open(dst_path.clone(), Identity::ZERO, 0).map(Arc::new)?;

    let snapshot_offset = create_snapshot(src_repo.clone()).await?;
    let src_snapshot_path = src_repo.snapshot_dir_path(snapshot_offset);
    let mut src_snapshot = Snapshot::read_from_file(&src_snapshot_path.snapshot_file(snapshot_offset))?;
    let total_objects = src_snapshot.total_objects() as u64;

    let blob_provider = SnapshotRepository::object_repo(&src_snapshot_path).map(Arc::new)?;

    // This is the first snapshot in `dst_repo`, so all objects should be written.
    let stats = synchronize_snapshot(blob_provider.clone(), dst_path.clone(), src_snapshot.clone()).await?;
    assert_eq!(stats.objects_written, total_objects);

    // Assert that the copied snapshot is valid.
    let dst_snapshot_full = dst_repo.read_snapshot(snapshot_offset)?;
    Locking::restore_from_snapshot(dst_snapshot_full)?;

    // Let's also check that running `synchronize_snapshot` again does nothing.
    let stats = synchronize_snapshot(blob_provider.clone(), dst_path.clone(), src_snapshot.clone()).await?;
    assert_eq!(stats.objects_skipped, total_objects);

    // Lastly, pretend the next snapshot has the same objects and
    // assert that they all get hardlinked.
    src_snapshot.tx_offset += SNAPSHOT_FREQUENCY;
    let stats = synchronize_snapshot(blob_provider.clone(), dst_path.clone(), src_snapshot.clone()).await?;
    assert_eq!(stats.objects_hardlinked, total_objects);

    // Try again to ensure we skip all objects previously hardlinked.
    let stats = synchronize_snapshot(blob_provider, dst_path, src_snapshot).await?;
    assert_eq!(stats.objects_skipped, total_objects);

    Ok(())
}

#[tokio::test]
async fn rejects_overwrite() -> anyhow::Result<()> {
    let tmp = tempdir()?;

    let src_path = SnapshotsPath::from_path_unchecked(tmp.path().join("src"));
    let dst_path = SnapshotsPath::from_path_unchecked(tmp.path().join("dst"));

    src_path.create()?;
    dst_path.create()?;

    let src_repo = SnapshotRepository::open(src_path, Identity::ZERO, 0).map(Arc::new)?;

    let snapshot_offset = create_snapshot(src_repo.clone()).await?;
    let src_snapshot_path = src_repo.snapshot_dir_path(snapshot_offset);
    let src_snapshot = Snapshot::read_from_file(&src_snapshot_path.snapshot_file(snapshot_offset))?;

    let blob_provider = SnapshotRepository::object_repo(&src_snapshot_path).map(Arc::new)?;

    synchronize_snapshot(blob_provider.clone(), dst_path.clone(), src_snapshot.clone()).await?;

    // Try to overwrite with the previous snapshot.
    let prev_offset = src_repo.latest_snapshot_older_than(snapshot_offset - 1)?.unwrap();
    let src_snapshot_path = src_repo.snapshot_dir_path(prev_offset);
    let mut src_snapshot = Snapshot::read_from_file(&src_snapshot_path.snapshot_file(prev_offset))?;
    // Pretend it's the current snapshot, thereby altering the preimage.
    src_snapshot.tx_offset = snapshot_offset;

    let res = synchronize_snapshot(blob_provider, dst_path, src_snapshot).await;
    assert_matches!(res, Err(SnapshotError::HashMismatch { .. }));

    Ok(())
}

async fn create_snapshot(repo: Arc<SnapshotRepository>) -> anyhow::Result<TxOffset> {
    let mut watch = spawn_blocking(|| {
        let tmp = TempReplicaDir::new()?;
        let db = TestDB::open_db(&tmp, EmptyHistory::new(), None, Some(repo), 0)?;
        let watch = db.subscribe_to_snapshots().unwrap();

        let table_id = db.with_auto_commit(Workload::Internal, |tx| {
            db.create_table(
                tx,
                table("a", ProductType::from([("x", AlgebraicType::U64)]), |builder| builder),
            )
        })?;

        for i in 0..SNAPSHOT_FREQUENCY {
            db.with_auto_commit(Workload::Internal, |tx| {
                db.insert(tx, table_id, &bsatn::to_vec(&product![i]).unwrap()).map(drop)
            })?;
        }

        Ok::<_, DBError>(watch)
    })
    .await
    .unwrap()?;

    let mut snapshot_offset = 0;
    while watch.changed().await.is_ok() {
        snapshot_offset = *watch.borrow_and_update();
    }
    assert!(snapshot_offset >= SNAPSHOT_FREQUENCY);

    Ok(snapshot_offset)
}

fn table(name: &str, columns: ProductType, f: impl FnOnce(RawTableDefBuilder) -> RawTableDefBuilder) -> TableSchema {
    let mut builder = RawModuleDefV9Builder::new();
    f(builder.build_table_with_new_type(name, columns, true));
    let raw = builder.finish();
    let def: ModuleDef = raw.try_into().expect("table validation failed");
    let table = def.table(name).expect("table not found");
    TableSchema::from_module_def(&def, table, (), TableId::SENTINEL)
}

fn enable_logging() {
    let _ = env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .is_test(true)
        .try_init();
}
