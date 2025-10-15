use std::{io, sync::Arc, time::Instant};

use env_logger::Env;
use log::info;
use pretty_assertions::assert_matches;
use rand::seq::IndexedRandom as _;
use spacetimedb::{
    db::{
        relational_db::{
            tests_utils::{TempReplicaDir, TestDB},
            Persistence, SNAPSHOT_FREQUENCY,
        },
        snapshot::{self, SnapshotWorker},
    },
    error::DBError,
    Identity,
};
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::locking_tx_datastore::datastore::Locking;
use spacetimedb_durability::{EmptyHistory, NoDurability, TxOffset};
use spacetimedb_fs_utils::dir_trie::DirTrie;
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
use spacetimedb_snapshot::{
    remote::{synchronize_snapshot, verify_snapshot},
    Snapshot, SnapshotError, SnapshotRepository,
};
use spacetimedb_table::page_pool::PagePool;
use tempfile::{tempdir, TempDir};
use tokio::{sync::OnceCell, task::spawn_blocking};

// TODO: Happy path for compressed snapshot, pending #2034
#[tokio::test]
async fn can_sync_a_snapshot() -> anyhow::Result<()> {
    enable_logging();
    let tmp = tempdir()?;
    let src = SourceSnapshot::get_or_create().await?;

    let dst_path = SnapshotsPath::from_path_unchecked(tmp.path());
    dst_path.create()?;

    let dst_repo = SnapshotRepository::open(dst_path.clone(), Identity::ZERO, 0).map(Arc::new)?;

    let mut src_snapshot = src.meta.clone();
    let total_objects = src_snapshot.total_objects() as u64;

    let blob_provider = src.objects.clone();

    // This is the first snapshot in `dst_repo`, so all objects should be written.
    let stats = synchronize_snapshot(blob_provider.clone(), dst_path.clone(), src_snapshot.clone()).await?;
    assert_eq!(stats.objects_written, total_objects);

    // Assert that the copied snapshot is valid.
    let pool = PagePool::new_for_test();
    let dst_snapshot_full = dst_repo.read_snapshot(src.offset, &pool)?;
    Locking::restore_from_snapshot(dst_snapshot_full, pool)?;

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
    enable_logging();
    let tmp = tempdir()?;
    let src = SourceSnapshot::get_or_create().await?;

    let dst_path = SnapshotsPath::from_path_unchecked(tmp.path());
    dst_path.create()?;

    let src_snapshot = src.meta.clone();
    let blob_provider = src.objects.clone();

    synchronize_snapshot(blob_provider.clone(), dst_path.clone(), src_snapshot.clone()).await?;

    // Try to overwrite with the previous snapshot.
    // A previous snapshot exists because one is created immediately after
    // database initialization.
    let prev_offset = src.repo.latest_snapshot_older_than(src.offset - 1)?.unwrap();
    let src_snapshot_path = src.repo.snapshot_dir_path(prev_offset);
    let (mut src_snapshot, _) = Snapshot::read_from_file(&src_snapshot_path.snapshot_file(prev_offset))?;
    // Pretend it's the current snapshot, thereby altering the preimage.
    src_snapshot.tx_offset = src.offset;

    let res = synchronize_snapshot(blob_provider, dst_path, src_snapshot).await;
    assert_matches!(res, Err(SnapshotError::HashMismatch { .. }));

    Ok(())
}

#[tokio::test]
async fn verifies_objects() -> anyhow::Result<()> {
    enable_logging();
    let tmp = tempdir()?;
    let src = SourceSnapshot::get_or_create().await?;

    let dst_path = SnapshotsPath::from_path_unchecked(tmp.path());
    dst_path.create()?;

    let src_snapshot = src.meta.clone();

    synchronize_snapshot(src.objects.clone(), dst_path.clone(), src_snapshot.clone()).await?;

    // Read objects for verification from the destination repo.
    let blob_provider = spawn_blocking({
        let dst_path = dst_path.clone();
        let snapshot_offset = src_snapshot.tx_offset;
        move || {
            let repo = SnapshotRepository::open(dst_path, Identity::ZERO, 0)?;
            let objects = SnapshotRepository::object_repo(&repo.snapshot_dir_path(snapshot_offset))?;
            anyhow::Ok(Arc::new(objects))
        }
    })
    .await
    .unwrap()?;
    // Initially, all should be good.
    verify_snapshot(blob_provider.clone(), dst_path.clone(), src_snapshot.clone()).await?;

    // Pick a random object to mess with.
    let random_object_path = {
        let all_objects = src_snapshot.objects().collect::<Box<[_]>>();
        let random_object = all_objects.choose(&mut rand::rng()).copied().unwrap();
        blob_provider.file_path(random_object.as_bytes())
    };

    // Truncate the object file and assert that verification fails.
    tokio::fs::File::options()
        .write(true)
        .open(&random_object_path)
        .await?
        .set_len(1)
        .await?;
    info!("truncated object file {}", random_object_path.display());
    let err = verify_snapshot(blob_provider.clone(), dst_path.clone(), src_snapshot.clone())
        .await
        .unwrap_err();
    assert_matches!(
        err,
        // If the object is a page, we'll get `Deserialize`,
        // otherwise `HashMismatch`.
        SnapshotError::HashMismatch { .. } | SnapshotError::Deserialize { .. }
    );

    // Delete the object file and assert that verification fails.
    tokio::fs::remove_file(&random_object_path).await?;
    info!("deleted object file {}", random_object_path.display());
    let err = verify_snapshot(blob_provider, dst_path, src_snapshot)
        .await
        .unwrap_err();
    assert_matches!(err, SnapshotError::ReadObject { cause, ..} if cause.kind() == io::ErrorKind::NotFound);

    Ok(())
}

/// Creating a snapshot takes a long time, because we need to commit
/// `SNAPSHOT_FREQUENCY` transactions to trigger one.
///
/// Until the snapshot frequency becomes configurable,
/// avoid creating the source snapshot repeatedly.
struct SourceSnapshot {
    offset: TxOffset,
    meta: Snapshot,
    objects: Arc<DirTrie>,
    repo: Arc<SnapshotRepository>,

    #[allow(unused)]
    tmp: TempDir,
}

impl SourceSnapshot {
    async fn get_or_create() -> anyhow::Result<&'static Self> {
        static SOURCE_SNAPSHOT: OnceCell<SourceSnapshot> = OnceCell::const_new();
        SOURCE_SNAPSHOT.get_or_try_init(Self::try_init).await
    }

    async fn try_init() -> anyhow::Result<Self> {
        let tmp = tempdir()?;

        let repo_path = SnapshotsPath::from_path_unchecked(tmp.path());
        let repo = spawn_blocking(move || {
            repo_path.create()?;
            SnapshotRepository::open(repo_path, Identity::ZERO, 0).map(Arc::new)
        })
        .await
        .unwrap()?;
        let offset = create_snapshot(repo.clone()).await?;

        let dir_path = repo.snapshot_dir_path(offset);
        let (meta, objects) = spawn_blocking(move || {
            let meta = Snapshot::read_from_file(&dir_path.snapshot_file(offset)).map(|(file, _)| file)?;
            let objects = SnapshotRepository::object_repo(&dir_path).map(Arc::new)?;

            Ok::<_, SnapshotError>((meta, objects))
        })
        .await
        .unwrap()?;

        Ok(SourceSnapshot {
            offset,
            meta,
            objects,
            repo,
            tmp,
        })
    }
}

async fn create_snapshot(repo: Arc<SnapshotRepository>) -> anyhow::Result<TxOffset> {
    let start = Instant::now();
    // NOTE: `_db` needs to stay alive until the snapshot is taken,
    // because the snapshot worker holds only a weak reference.
    let (mut watch, _db) = spawn_blocking(|| {
        let tmp = TempReplicaDir::new()?;

        let persistence = Persistence {
            durability: Arc::new(NoDurability::default()),
            disk_size: Arc::new(|| Ok(0)),
            snapshots: Some(SnapshotWorker::new(repo, snapshot::Compression::Disabled)),
        };
        let db = TestDB::open_db(&tmp, EmptyHistory::new(), Some(persistence), None, 0)?;
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

        Ok::<_, DBError>((watch, db))
    })
    .await
    .unwrap()?;

    let mut snapshot_offset = *watch.borrow();
    while snapshot_offset < SNAPSHOT_FREQUENCY && watch.changed().await.is_ok() {
        snapshot_offset = *watch.borrow_and_update();
    }
    assert!(snapshot_offset >= SNAPSHOT_FREQUENCY);
    info!(
        "snapshot creation took {}s",
        Instant::now().duration_since(start).as_secs_f32()
    );

    Ok(snapshot_offset)
}

fn table(
    name: &str,
    columns: ProductType,
    f: impl FnOnce(RawTableDefBuilder<'_>) -> RawTableDefBuilder,
) -> TableSchema {
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
