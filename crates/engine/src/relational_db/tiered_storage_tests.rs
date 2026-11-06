//! Exercise transactions and recovery against the real disk paging backend.

use super::{tests_utils, *};
use crate::persistence::{Database, LocalPersistenceProvider, PersistenceProvider};
use crate::snapshot::Compression;
use spacetimedb_durability::Durability as _;
use spacetimedb_paths::{server::ServerDataDir, FromPathUnchecked};
use spacetimedb_sats::{bsatn, layout::Size, product};
use spacetimedb_table::tiered::BudgetPermit;
use spacetimedb_table::tiered::{ObjectKey, ObjectMetadata, ObjectStoreError};
use spacetimedb_table::{
    blob_store::BlobHash,
    indexes::{max_rows_in_page, PAGE_DATA_SIZE},
    page::Page,
    tiered::{BackingStore, ByteBudget, ByteBudgetConfig, Object, ObjectBytes, ObjectStore},
};
use spacetimedb_tiered_storage::DiskStorage;
use std::{
    ops::Range,
    path::{Path, PathBuf},
    slice,
    sync::atomic::{AtomicU64, AtomicU8, Ordering::Relaxed},
};
use tempfile::TempDir;

type Result<T = ()> = anyhow::Result<T>;
type StoreResult<T> = std::result::Result<T, ObjectStoreError>;
const MEMORY_LIMIT_BYTES: u64 = 16 * 1024 * 1024;
const PAGE_BYTES: u64 = size_of::<Page>() as u64;

#[derive(Clone, Copy)]
#[repr(u8)]
enum Fault {
    None,
    Read,
    Write,
    CorruptPage,
    CorruptBlob,
}

#[derive(Debug, Default)]
struct DiskIo {
    page_reads: AtomicU64,
    blob_reads: AtomicU64,
    page_writes: AtomicU64,
    blob_writes: AtomicU64,
    fault: AtomicU8,
}

impl DiskIo {
    fn fail(&self, fault: Fault) {
        self.fault.store(fault as u8, Relaxed);
    }
}

/// Keep real filesystem I/O, while making failures independent of OS timing.
#[derive(Debug)]
struct TestDisk {
    disk: DiskStorage,
    directory: PathBuf,
    io: Arc<DiskIo>,
}

impl ObjectStore for TestDisk {
    fn stat(&self, key: ObjectKey) -> StoreResult<Option<ObjectMetadata>> {
        self.disk.stat(key)
    }

    fn read(&self, key: ObjectKey) -> StoreResult<Option<ObjectBytes>> {
        let is_page = key.as_page_hash().is_some();
        let counter = if is_page {
            &self.io.page_reads
        } else {
            &self.io.blob_reads
        };
        counter.fetch_add(1, Relaxed);
        let fault = self.io.fault.load(Relaxed);
        if fault == Fault::Read as u8 {
            return Err(io::Error::other("injected paging read failure").into());
        }
        let corrupt = if is_page {
            Fault::CorruptPage
        } else {
            Fault::CorruptBlob
        };
        if fault == corrupt as u8 {
            let file = object_file(&self.directory, key)?;
            let mut bytes = std::fs::read(&file)?;
            // Preserve the serialized page structure; change its final data byte.
            *bytes.last_mut().expect("stored objects are nonempty") ^= 1;
            std::fs::write(file, bytes)?;
            self.io.fail(Fault::None);
        }
        self.disk.read(key)
    }

    fn write(&self, object: Object<'_>) -> StoreResult<ObjectKey> {
        let counter = match object {
            Object::Page(_) => &self.io.page_writes,
            Object::Blob(_) => &self.io.blob_writes,
        };
        counter.fetch_add(1, Relaxed);
        if self.io.fault.load(Relaxed) == Fault::Write as u8 {
            return Err(io::Error::other("injected paging write failure").into());
        }
        self.disk.write(object)
    }
}

fn store(dir: &ReplicaDir, budget: ByteBudget, io: Arc<DiskIo>) -> io::Result<TieredStorageConfig> {
    let pool = PagePool::new(Some(PAGE_BYTES as usize));
    let mut config = TieredStorageConfig::new_for_memory_database(pool);
    config.resident_budget = budget;
    config.local_store = BackingStore {
        object_store: Box::new(TestDisk {
            disk: DiskStorage::open(dir.tables())?,
            directory: dir.tables().0,
            io,
        }),
        budget: ByteBudget::unlimited(),
    };
    Ok(config)
}

/// Find the actual file so failure tests do not depend on page/blob subdirectory names.
fn object_file(directory: &Path, key: ObjectKey) -> io::Result<PathBuf> {
    let hash = key.to_string();
    let mut directories = vec![directory.to_path_buf()];
    for _ in 0..1024 {
        let Some(directory) = directories.pop() else { break };
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                directories.push(entry.path());
            } else if entry.file_name() == hash.as_str() || entry.file_name() == hash[2..] {
                return Ok(entry.path());
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("cache file for {key} was not written"),
    ))
}

#[derive(Clone, Copy)]
enum PersistenceMode {
    Ephemeral,
    Commitlog,
    Snapshot,
}

struct PagingDb {
    database: Option<RelationalDB>,
    durability: Option<LocalDurability>,
    snapshots: Option<Arc<DynSnapshotRepo>>,
    io: Arc<DiskIo>,
    budget: ByteBudget,
    persistent: bool,
    runtime: tokio::runtime::Runtime,
    directory: TempDir,
}

impl PagingDb {
    fn new(mode: PersistenceMode) -> Result<Self> {
        let persistent = !matches!(mode, PersistenceMode::Ephemeral);
        let mut db = Self {
            database: None,
            durability: None,
            snapshots: None,
            io: Arc::default(),
            budget: ByteBudget::new(ByteBudgetConfig {
                low_water_bytes: 0,
                soft_limit_bytes: MEMORY_LIMIT_BYTES,
                hard_limit_bytes: MEMORY_LIMIT_BYTES,
            })?,
            persistent,
            runtime: tokio::runtime::Builder::new_current_thread().enable_all().build()?,
            directory: TempDir::with_prefix("tiered-engine")?,
        };
        if matches!(mode, PersistenceMode::Snapshot) {
            db.snapshots = Some(open_snapshot_repo(db.replica().snapshots(), Identity::ZERO, 1)?);
        }
        db.open(false)?;
        Ok(db)
    }

    fn replica(&self) -> ReplicaDir {
        ServerDataDir::from_path_unchecked(self.directory.path()).replica(1)
    }

    fn db(&self) -> &RelationalDB {
        self.database
            .as_ref()
            .expect("database is open between fixture operations")
    }

    fn open(&mut self, restoring: bool) -> Result {
        let _entered = self.runtime.enter();
        let storage = store(&self.replica(), self.budget.clone(), self.io.clone())?;
        let database = if self.persistent {
            let runtime = Handle::tokio(self.runtime.handle().clone());
            let (durability, disk_size) =
                self.runtime
                    .block_on(local_durability(self.replica(), runtime.clone(), None))?;
            // Initial snapshots are taken explicitly so they cannot race fault injection.
            let snapshots = restoring
                .then(|| self.snapshots.clone())
                .flatten()
                .map(|repo| SnapshotWorker::new(repo, Compression::Disabled, runtime.clone()));
            let local = durability.clone();
            let services = Persistence::persistent(local, disk_size, snapshots, storage, runtime);
            let history = durability.as_history();
            let database = tests_utils::TestDB::open_db(history, services, None, 0)?;
            self.durability = Some(durability);
            database
        } else {
            tests_utils::TestDB::open_db(
                durability::EmptyHistory::new(),
                Persistence::ephemeral(storage),
                None,
                0,
            )?
        };
        self.database = Some(database);
        Ok(())
    }

    fn close(&mut self) {
        if let Some(database) = self.database.take() {
            self.runtime.block_on(database.shutdown());
            drop(database);
        }
        if let Some(durability) = self.durability.take() {
            self.runtime.block_on(durability.close());
        }
    }

    fn reopen(&mut self) -> Result {
        assert!(self.persistent);
        self.close();
        self.open(true)
    }

    fn snapshot(&self) -> Result {
        let repo = self.snapshots.as_ref().expect("snapshot repository was requested");
        let offset = self
            .db()
            .inner
            .take_snapshot(repo.as_ref())?
            .expect("test database is nonempty");
        self.runtime
            .block_on(self.db().durable_tx_offset().unwrap().wait_for(offset))?;
        assert_eq!(repo.latest_snapshot()?, Some(offset));
        Ok(())
    }

    fn transaction<T>(&self, action: impl FnOnce(&mut MutTx) -> Result<T>) -> Result<T> {
        tests_utils::with_auto_commit(self.db(), action)
    }

    fn create_table(&self, name: &str, blobs: bool, policy: TableCachePolicy) -> Result<TableId> {
        let mut columns = vec![("id", AlgebraicType::U64), ("group", AlgebraicType::U64)];
        if blobs {
            columns.extend([
                ("label", AlgebraicType::String),
                ("bytes", AlgebraicType::array(AlgebraicType::U8)),
                ("same_bytes", AlgebraicType::array(AlgebraicType::U8)),
            ]);
        }
        let mut builder = RawModuleDefV9Builder::new();
        let columns = ProductType::from_iter(columns);
        builder
            .build_table_with_new_type_for_tests(RawIdentifier::new(name), columns, true)
            .with_primary_key(0)
            .with_unique_constraint(0)
            .with_index_no_accessor_name(btree(0))
            .with_index_no_accessor_name(btree(1))
            .finish();
        let module: ModuleDef = builder.finish().try_into()?;
        let table = module.table(name).unwrap();
        let mut schema = TableSchema::from_module_def(&module, table, (), TableId::SENTINEL);
        schema.cache_policy = policy;
        self.transaction(|tx| Ok(self.db().create_table(tx, schema)?))
    }

    fn insert(&self, table: TableId, row: &ProductValue) -> Result {
        self.transaction(|tx| {
            tests_utils::insert(self.db(), tx, table, row)?;
            Ok(())
        })
    }

    fn fill_fixed(&self, table: TableId, ids: Range<u64>) -> Result {
        for batch in ids.collect::<Vec<_>>().chunks(256) {
            self.transaction(|tx| {
                for &id in batch {
                    tests_utils::insert(self.db(), tx, table, &product![id, id % 3])?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    fn rows(&self, table: TableId) -> Result<Vec<ProductValue>> {
        self.db().with_read_only(Workload::ForTests, |tx| {
            let mut rows = self
                .db()
                .iter(tx, table)?
                .map(|row| Ok(row?.to_product_value()))
                .collect::<Result<Vec<_>>>()?;
            rows.sort_unstable();
            Ok(rows)
        })
    }

    fn lookup(&self, table: TableId, column: u16, value: u64) -> Result<Vec<ProductValue>> {
        self.db().with_read_only(Workload::ForTests, |tx| {
            let mut rows = self
                .db()
                .iter_by_col_eq(tx, table, ColId(column), &value.into())?
                .map(|row| Ok(row?.to_product_value()))
                .collect::<Result<Vec<_>>>()?;
            rows.sort_unstable();
            Ok(rows)
        })
    }

    fn policy(&self, table: TableId) -> Result<TableCachePolicy> {
        self.db().with_read_only(Workload::ForTests, |tx| {
            Ok(self.db().schema_for_table(tx, table)?.cache_policy)
        })
    }

    fn evict_blobs(&self, table: TableId) -> Result {
        let committed = self.db().inner.committed_state.read();
        let (_, blobs, _) = committed.get_table_and_blob_store(table)?;
        blobs.evict()?;
        Ok(())
    }

    // Reserve the spare budget so a small dataset is enough to force eviction.
    fn leave_memory_available(&self, bytes: u64) -> Result<BudgetPermit> {
        let used = self.budget.usage().accounted_bytes();
        assert!(used + bytes <= MEMORY_LIMIT_BYTES);
        Ok(self.budget.acquire(MEMORY_LIMIT_BYTES - used - bytes)?)
    }
}

impl Drop for PagingDb {
    fn drop(&mut self) {
        self.close();
    }
}

fn blob_row(id: u64, bytes: Vec<u8>) -> ProductValue {
    let bytes = AlgebraicValue::Bytes(bytes.into());
    product![id, id % 3, format!("row-{id}"), bytes.clone(), bytes]
}

#[test]
fn fixed_rows_and_indexes_survive_memory_pressure() -> Result {
    let db = PagingDb::new(PersistenceMode::Ephemeral)?;
    let table = db.create_table("fixed", false, TableCachePolicy::RequiredOnDisk)?;
    let pressure = db.leave_memory_available(3 * PAGE_BYTES)?;
    let count = 3 * max_rows_in_page(Size(16)) as u64 + 1;
    db.fill_fixed(table, 0..count)
        .context("inserting fixed rows under memory pressure")?;
    assert!(db.io.page_writes.load(Relaxed) > 0, "inserts must have evicted pages");

    let mut expected = (0..count).map(|id| product![id, id % 3]).collect::<Vec<_>>();
    assert_eq!(db.rows(table).context("scanning evicted fixed rows")?, expected);
    assert!(
        db.io.page_reads.load(Relaxed) > 0,
        "the scan must have faulted pages back in"
    );
    assert_eq!(db.lookup(table, 0, 0)?, [expected[0].clone()]);
    assert_eq!(
        db.lookup(table, 1, 2)?,
        expected
            .iter()
            .filter(|row| row.elements[1] == 2_u64.into())
            .cloned()
            .collect::<Vec<_>>()
    );

    // The update must be able to pin its source and destination alongside transaction pages.
    drop(pressure);
    db.transaction(|tx| {
        let index = db
            .db()
            .schema_for_table_mut(tx, table)?
            .indexes
            .iter()
            .find(|index| index.index_algorithm.columns().as_singleton() == Some(ColId(0)))
            .unwrap()
            .index_id;
        db.db()
            .update(tx, table, index, &bsatn::to_vec(&product![0_u64, 99_u64])?)?;
        assert_eq!(db.db().delete_by_rel(tx, table, [expected[1].clone()]), 1);
        Ok(())
    })
    .context("updating and deleting previously evicted fixed rows")?;
    expected[0] = product![0_u64, 99_u64];
    expected.remove(1);
    assert_eq!(db.rows(table)?, expected);
    assert_eq!(db.lookup(table, 0, 1)?, []);
    assert_eq!(db.lookup(table, 1, 99)?, [expected[0].clone()]);
    Ok(())
}

#[test]
fn variable_rows_survive_repeated_blob_eviction() -> Result {
    let db = PagingDb::new(PersistenceMode::Ephemeral)?;
    let table = db.create_table("variable", true, TableCachePolicy::RequiredOnDisk)?;
    let rows = [blob_row(1, vec![1; 32]), blob_row(2, vec![2; 8192])];
    for row in &rows {
        db.insert(table, row)?;
    }
    for _ in 0..2 {
        db.evict_blobs(table)?;
        assert_eq!(db.rows(table)?, rows);
        assert_eq!(db.lookup(table, 0, 2)?, [rows[1].clone()]);
    }
    assert!(db.io.blob_reads.load(Relaxed) >= 2);
    Ok(())
}

#[test]
fn byte_array_compression_signature_is_preserved() -> Result {
    let db = PagingDb::new(PersistenceMode::Ephemeral)?;
    let table = db.create_table("bytes", true, TableCachePolicy::RequiredOnDisk)?;
    let mut bytes = vec![0xff; 4096];
    bytes[..4].copy_from_slice(&[0x28, 0xb5, 0x2f, 0xfd]);
    let row = blob_row(1, bytes);
    db.insert(table, &row)?;
    db.evict_blobs(table)?;
    assert_eq!(db.rows(table)?, [row]);
    Ok(())
}

#[test]
fn rollback_and_unique_violation_preserve_rows_and_indexes() -> Result {
    let db = PagingDb::new(PersistenceMode::Ephemeral)?;
    let table = db.create_table("rollback", true, TableCachePolicy::RequiredOnDisk)?;
    let original = blob_row(1, vec![7; 8192]);
    db.insert(table, &original)?;
    db.evict_blobs(table)?;
    let mut tx = tests_utils::begin_mut_tx(db.db());
    assert_eq!(db.db().delete_by_rel(&mut tx, table, [original.clone()]), 1);
    tests_utils::insert(db.db(), &mut tx, table, &blob_row(2, vec![8; 8192]))?;
    let _ = db.db().rollback_mut_tx(tx);
    assert_eq!(db.rows(table)?.as_slice(), slice::from_ref(&original));
    assert!(db.insert(table, &blob_row(1, vec![9; 8192])).is_err());
    assert_eq!(db.lookup(table, 0, 1)?, [original]);
    assert_eq!(db.lookup(table, 0, 2)?, []);
    assert_eq!(db.lookup(table, 1, 2)?, []);
    assert!(db.io.blob_reads.load(Relaxed) > 0);
    Ok(())
}

#[test]
fn failed_drop_rolls_back_other_row_changes_and_allows_retry() -> Result {
    let mut db = PagingDb::new(PersistenceMode::Commitlog)?;
    let table = db.create_table("dropped", true, TableCachePolicy::RequiredOnDisk)?;
    let other = db.create_table("other", false, TableCachePolicy::RequiredOnDisk)?;
    let original = blob_row(1, vec![7; 8192]);
    db.insert(table, &original)?;
    db.insert(other, &product![1_u64, 1_u64])?;
    db.evict_blobs(table)?;
    let mut tx = tests_utils::begin_mut_tx(db.db());
    tests_utils::insert(db.db(), &mut tx, other, &product![2_u64, 2_u64])?;
    db.io.fail(Fault::Read);
    let error = db.db().drop_table(&mut tx, table).unwrap_err();
    let _ = db.db().rollback_mut_tx(tx);
    assert!(format!("{error:#}").contains("injected paging read failure"));
    db.io.fail(Fault::None);
    assert_eq!(db.rows(table)?.as_slice(), slice::from_ref(&original));
    assert_eq!(db.rows(other)?, [product![1_u64, 1_u64]]);
    assert_eq!(db.lookup(other, 0, 2)?, []);
    db.reopen()?;
    assert_eq!(db.lookup(table, 0, 1)?.as_slice(), slice::from_ref(&original));
    assert_eq!(db.rows(table)?, [original]);
    assert_eq!(db.rows(other)?, [product![1_u64, 1_u64]]);
    db.transaction(|tx| {
        tests_utils::insert(db.db(), tx, other, &product![2_u64, 2_u64])?;
        db.db().drop_table(tx, table)?;
        Ok(())
    })?;
    assert_eq!(db.rows(other)?, [product![1_u64, 1_u64], product![2_u64, 2_u64]]);
    Ok(())
}

#[test]
fn failed_page_write_leaves_committed_data_and_allows_retry() -> Result {
    let db = PagingDb::new(PersistenceMode::Ephemeral)?;
    let table = db.create_table("write_failure", false, TableCachePolicy::RequiredOnDisk)?;
    let count = 2 * max_rows_in_page(Size(16)) as u64;
    db.fill_fixed(table, 0..count)?;
    let _pressure = db.leave_memory_available(PAGE_BYTES)?;
    db.io.fail(Fault::Write);
    let error = db.insert(table, &product![count, 0_u64]).unwrap_err();
    assert!(format!("{error:#}").contains("injected paging write failure"));
    db.io.fail(Fault::None);
    assert_eq!(db.lookup(table, 0, count)?, []);
    assert_eq!(
        db.rows(table)?,
        (0..count).map(|id| product![id, id % 3]).collect::<Vec<_>>()
    );
    db.insert(table, &product![count, 0_u64])?;
    assert_eq!(db.lookup(table, 0, count)?, [product![count, 0_u64]]);
    let mut expected = (0..count).map(|id| product![id, id % 3]).collect::<Vec<_>>();
    expected.push(product![count, 0_u64]);
    assert_eq!(db.rows(table)?, expected);
    Ok(())
}

fn recover_paged_database(mode: PersistenceMode) -> Result {
    let mut db = PagingDb::new(mode)?;
    let table = db.create_table("recovered", true, TableCachePolicy::RequiredOnDisk)?;
    let first = blob_row(1, vec![7; 8192]);
    let second = blob_row(2, vec![8; 8192]);
    db.insert(table, &first)?;
    db.evict_blobs(table)?;
    if matches!(mode, PersistenceMode::Snapshot) {
        db.snapshot()?;
    }
    db.insert(table, &second)?;
    db.reopen()?;
    assert_eq!(db.rows(table)?, [first.clone(), second.clone()]);
    assert_eq!(db.lookup(table, 0, 1)?.as_slice(), slice::from_ref(&first));
    assert_eq!(db.lookup(table, 1, 2)?.as_slice(), slice::from_ref(&second));
    assert_eq!(db.policy(table)?, TableCachePolicy::RequiredOnDisk);
    db.transaction(|tx| {
        assert_eq!(db.db().delete_by_rel(tx, table, [first]), 1);
        tests_utils::insert(db.db(), tx, table, &blob_row(3, vec![9; 8192]))?;
        Ok(())
    })?;
    db.evict_blobs(table)?;
    assert_eq!(db.lookup(table, 0, 1)?, []);
    assert_eq!(db.lookup(table, 1, 0)?, [blob_row(3, vec![9; 8192])]);
    assert_eq!(db.rows(table)?, [second, blob_row(3, vec![9; 8192])]);
    assert!(db.io.blob_reads.load(Relaxed) > 0);
    Ok(())
}

#[test]
fn paging_survives_commitlog_recovery() -> Result {
    recover_paged_database(PersistenceMode::Commitlog)
}

#[test]
fn paging_survives_snapshot_and_commitlog_tail_recovery() -> Result {
    recover_paged_database(PersistenceMode::Snapshot)
}

#[test]
fn shared_blob_remains_pinned_until_memory_table_is_dropped() -> Result {
    let db = PagingDb::new(PersistenceMode::Ephemeral)?;
    let memory = db.create_table("memory", true, TableCachePolicy::RequiredInMemory)?;
    let disk = db.create_table("disk", true, TableCachePolicy::RequiredOnDisk)?;
    let first = blob_row(1, vec![7; 8192]);
    let second = blob_row(2, vec![7; 8192]);
    db.insert(memory, &first)?;
    db.insert(memory, &second)?;
    db.insert(disk, &first)?;
    db.evict_blobs(disk)?;
    assert_eq!(db.io.blob_writes.load(Relaxed), 0);
    db.transaction(|tx| {
        assert_eq!(db.db().delete_by_rel(tx, memory, [first.clone()]), 1);
        Ok(())
    })?;
    db.evict_blobs(disk)?;
    assert_eq!(db.io.blob_writes.load(Relaxed), 0);
    db.transaction(|tx| Ok(db.db().drop_table(tx, memory)?))?;
    db.evict_blobs(disk)?;
    assert!(db.io.blob_writes.load(Relaxed) > 0);
    assert_eq!(db.rows(disk)?, [first]);
    db.transaction(|tx| {
        assert_eq!(db.db().clear_table(tx, disk)?, 1);
        Ok(())
    })?;
    assert_eq!(db.rows(disk)?, []);
    Ok(())
}

#[test]
fn cache_policy_migration_preserves_rows_indexes_and_blob_pins() -> Result {
    let mut db = PagingDb::new(PersistenceMode::Snapshot)?;
    let table = db.create_table("migration", true, TableCachePolicy::RequiredInMemory)?;
    let row = blob_row(1, vec![7; 8192]);
    db.insert(table, &row)?;
    db.evict_blobs(table)?;
    assert_eq!(db.io.blob_writes.load(Relaxed), 0);
    db.transaction(|tx| {
        Ok(db
            .db()
            .alter_table_cache_policy(tx, table, TableCachePolicy::RequiredOnDisk)?)
    })?;
    db.evict_blobs(table)?;
    assert!(db.io.blob_writes.load(Relaxed) > 0);
    db.transaction(|tx| {
        Ok(db
            .db()
            .alter_table_cache_policy(tx, table, TableCachePolicy::RequiredInMemory)?)
    })?;
    let writes = db.io.blob_writes.load(Relaxed);
    db.evict_blobs(table)?;
    assert_eq!(db.io.blob_writes.load(Relaxed), writes);
    assert_eq!(db.lookup(table, 0, 1)?.as_slice(), slice::from_ref(&row));
    assert_eq!(db.policy(table)?, TableCachePolicy::RequiredInMemory);
    db.snapshot()?;
    db.reopen()?;
    db.evict_blobs(table)?;
    assert_eq!(db.lookup(table, 1, 1)?.as_slice(), slice::from_ref(&row));
    assert_eq!(db.lookup(table, 0, 1)?, [row]);
    assert_eq!(db.policy(table)?, TableCachePolicy::RequiredInMemory);
    Ok(())
}

#[test]
fn failed_migration_retains_the_old_policy_and_can_be_retried() -> Result {
    let db = PagingDb::new(PersistenceMode::Ephemeral)?;
    let table = db.create_table("migration_failure", true, TableCachePolicy::RequiredOnDisk)?;
    let row = blob_row(1, vec![7; 1024 * 1024]);
    db.insert(table, &row)?;
    db.evict_blobs(table)?;
    let pressure = db.leave_memory_available(PAGE_BYTES)?;
    let error = db
        .transaction(|tx| {
            Ok(db
                .db()
                .alter_table_cache_policy(tx, table, TableCachePolicy::RequiredInMemory)?)
        })
        .unwrap_err();
    assert!(format!("{error:#}").contains("memory limit exceeded"));
    assert_eq!(db.policy(table)?, TableCachePolicy::RequiredOnDisk);
    drop(pressure);
    assert_eq!(db.lookup(table, 0, 1)?.as_slice(), slice::from_ref(&row));
    db.transaction(|tx| {
        Ok(db
            .db()
            .alter_table_cache_policy(tx, table, TableCachePolicy::RequiredInMemory)?)
    })?;
    assert_eq!(db.policy(table)?, TableCachePolicy::RequiredInMemory);
    db.evict_blobs(table)?;
    assert_eq!(db.rows(table)?, [row]);
    Ok(())
}

#[test]
fn corrupted_blob_is_reported_as_an_error() -> Result {
    let db = PagingDb::new(PersistenceMode::Ephemeral)?;
    let table = db.create_table("corrupt_blob", true, TableCachePolicy::RequiredOnDisk)?;
    db.insert(table, &blob_row(1, vec![7; 8192]))?;
    db.evict_blobs(table)?;
    db.io.fail(Fault::CorruptBlob);
    let error = db
        .rows(table)
        .expect_err("corrupt data must not reach callers or cause a panic");
    assert!(format!("{error:#}").contains("doesn't match computed hash"));
    Ok(())
}

#[test]
fn corrupted_page_is_reported_as_an_error() -> Result {
    let db = PagingDb::new(PersistenceMode::Ephemeral)?;
    let table = db.create_table("corrupt_page", false, TableCachePolicy::RequiredOnDisk)?;
    let _pressure = db.leave_memory_available(3 * PAGE_BYTES)?;
    db.fill_fixed(table, 0..3 * max_rows_in_page(Size(16)) as u64 + 1)?;
    assert!(db.io.page_writes.load(Relaxed) > 0);
    db.io.fail(Fault::CorruptPage);
    let error = db
        .lookup(table, 0, 0)
        .expect_err("a corrupt page must be rejected even if it can be decoded");
    assert!(format!("{error:#}").contains("doesn't match computed hash"));
    assert!(db.io.page_reads.load(Relaxed) > 0);
    Ok(())
}

#[test]
fn failed_duplicate_open_preserves_the_running_databases_cache() -> Result {
    let db = PagingDb::new(PersistenceMode::Commitlog)?;
    let table = db.create_table("duplicate_open", true, TableCachePolicy::RequiredOnDisk)?;
    let row = blob_row(1, vec![7; 8192]);
    db.insert(table, &row)?;
    db.evict_blobs(table)?;
    let budget = db.budget.clone();
    let disk_io = db.io.clone();
    let provider = LocalPersistenceProvider::new(
        ServerDataDir::from_path_unchecked(db.directory.path()),
        PagePool::new(Some(PAGE_BYTES as usize)),
    )
    .with_table_storage(move |dir, _| store(&dir, budget.clone(), disk_io.clone()));
    let database = Database {
        id: 1,
        database_identity: Identity::ZERO,
        owner_identity: Identity::ZERO,
    };
    let result = db.runtime.block_on(provider.persistent(&database, 1));
    let error = result.err().expect("the running database still holds db.lock");
    assert!(format!("{error:#}").contains("already opened"));
    assert_eq!(db.rows(table)?, [row]);
    Ok(())
}

#[test]
fn incomplete_disk_object_can_be_replaced_by_a_complete_write() -> Result {
    use spacetimedb_table::tiered::{BlobSet, EvictionPolicy};

    let directory = TempDir::new()?;
    let path = spacetimedb_paths::server::TablesDir::from_path_unchecked(directory.path());
    let disk = DiskStorage::open(path)?;
    let blobs = BlobSet::new_for_test();
    let blob = blobs.insert(EvictionPolicy::Evictable, &[7; 4096])?;
    let key = disk.write(Object::Blob(&blob))?;
    let file = object_file(directory.path(), key)?;
    // Model the final filename left behind when a previous write failed partway through.
    std::fs::write(&file, &blob[..4])?;
    let key = disk.write(Object::Blob(&blob))?;
    assert_eq!(&*disk.read(key)?.unwrap(), &*blob);
    Ok(())
}

#[test]
fn raw_blob_compression_signature_is_preserved() -> Result {
    use spacetimedb_table::tiered::{BlobSet, EvictionPolicy};

    let directory = TempDir::new()?;
    let disk = DiskStorage::open(spacetimedb_paths::server::TablesDir::from_path_unchecked(
        directory.path(),
    ))?;
    let blobs = BlobSet::new_for_test();
    let mut bytes = vec![0xff; 4096];
    bytes[..4].copy_from_slice(&[0x28, 0xb5, 0x2f, 0xfd]);
    let blob = blobs.insert(EvictionPolicy::Evictable, &bytes)?;
    let key = disk.write(Object::Blob(&blob))?;
    assert_eq!(&*disk.read(key)?.unwrap(), bytes);
    Ok(())
}

#[test]
fn page_and_blob_with_the_same_hash_keep_separate_contents() -> Result {
    use spacetimedb_table::tiered::PageManager;
    use spacetimedb_table::tiered::{BlobSet, EvictionPolicy, ResidentObjects, TieredStorage};

    let directory = TempDir::new()?;
    let disk = DiskStorage::open(spacetimedb_paths::server::TablesDir::from_path_unchecked(
        directory.path(),
    ))?;
    let manager = PageManager::new(
        Arc::new(ResidentObjects::new(ByteBudget::unlimited())),
        PagePool::new(Some(PAGE_BYTES as usize)),
        Arc::new(TieredStorage::new_for_test()),
    );
    let slot = manager.allocate(Size(8), EvictionPolicy::Evictable)?;
    let page = slot.page().unwrap();
    let (page_hash, serialized) = page.serialize_for_storage();

    // A blob can contain precisely the bytes hashed by Page::content_hash.
    // Those differ from the BSATN serialization written to the page file.
    let bitset_bytes = u32::from_le_bytes(serialized[6..10].try_into()?) * 8;
    let mut hash_input = serialized[serialized.len() - PAGE_DATA_SIZE..].to_vec();
    hash_input.extend_from_slice(&serialized[..6]);
    hash_input.extend_from_slice(&serialized[10..10 + bitset_bytes as usize + 8]);
    assert_eq!(&BlobHash::hash_from_bytes(&hash_input).data, page_hash.as_bytes());
    assert_ne!(hash_input, serialized);

    let blobs = BlobSet::new_for_test();
    let blob = blobs.insert(EvictionPolicy::Evictable, &hash_input)?;
    let page_key = disk.write(Object::Page(&page))?;
    let blob_key = disk.write(Object::Blob(&blob))?;
    assert_eq!(&*disk.read(page_key)?.unwrap(), serialized);
    assert_eq!(&*disk.read(blob_key)?.unwrap(), hash_input);
    Ok(())
}
