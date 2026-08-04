use std::{
    collections::BTreeMap,
    fmt, io,
    ops::Range,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use spacetimedb_commitlog::repo::TxOffset;
use spacetimedb_fs_utils::compression::CompressType;
use spacetimedb_lib::Identity;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::bsatn;
use spacetimedb_snapshot::{
    BoxedPendingSnapshot, CompressionStats, ObjectType, PendingSnapshot, ReconstructedSnapshot, SnapshotError,
    SnapshotReadMetrics, SnapshotRepo, CURRENT_MODULE_ABI_VERSION,
};
use spacetimedb_table::{
    blob_store::{BlobHash, BlobStore, HashMapBlobStore},
    page_pool::PagePool,
    table::Table,
};

pub const DST_SNAPSHOT_RETENTION: usize = 2;

#[derive(Clone)]
pub struct InMemorySnapshotRepo {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug)]
struct Inner {
    database_identity: Identity,
    replica_id: u64,
    retention: usize,
    created_snapshots: u64,
    deleted_snapshots: u64,
    requested_snapshots: u64,
    last_requested_tx_offset: Option<TxOffset>,
    last_created_tx_offset: Option<TxOffset>,
    snapshots: BTreeMap<TxOffset, StoredSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotStats {
    pub created: u64,
    pub deleted: u64,
    pub requested: u64,
    pub queue_len: u64,
    pub last_requested_tx_offset: Option<TxOffset>,
    pub last_created_tx_offset: Option<TxOffset>,
    pub live_offsets: Vec<TxOffset>,
}

impl fmt::Display for SnapshotStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "snapshots: requested={}, queued={}, created={}, deleted={}, last_requested=",
            self.requested, self.queue_len, self.created, self.deleted
        )?;
        match self.last_requested_tx_offset {
            Some(offset) => write!(f, "{offset}")?,
            None => f.write_str("none")?,
        }
        f.write_str(", last_created=")?;
        match self.last_created_tx_offset {
            Some(offset) => write!(f, "{offset}")?,
            None => f.write_str("none")?,
        }
        write!(f, ", live={:?}", self.live_offsets)
    }
}

#[derive(Clone, Debug)]
struct StoredSnapshot {
    database_identity: Identity,
    replica_id: u64,
    tx_offset: TxOffset,
    module_abi_version: [u16; 2],
    blobs: Vec<StoredBlob>,
    tables: Vec<StoredTable>,
}

#[derive(Clone, Debug)]
struct StoredBlob {
    hash: BlobHash,
    uses: usize,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct StoredTable {
    table_id: TableId,
    pages: Vec<StoredPage>,
}

#[derive(Clone, Debug)]
struct StoredPage {
    hash: [u8; 32],
    bytes: Vec<u8>,
}

pub struct InMemoryPendingSnapshot {
    repo: InMemorySnapshotRepo,
    snapshot: StoredSnapshot,
}

impl InMemorySnapshotRepo {
    pub fn new(database_identity: Identity, replica_id: u64) -> Self {
        Self::with_retention(database_identity, replica_id, DST_SNAPSHOT_RETENTION)
    }

    pub fn with_retention(database_identity: Identity, replica_id: u64, retention: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                database_identity,
                replica_id,
                retention: retention.max(1),
                created_snapshots: 0,
                deleted_snapshots: 0,
                requested_snapshots: 0,
                last_requested_tx_offset: None,
                last_created_tx_offset: None,
                snapshots: BTreeMap::new(),
            })),
        }
    }

    pub fn stats(&self) -> SnapshotStats {
        let inner = self.inner.lock().unwrap();
        SnapshotStats {
            created: inner.created_snapshots,
            deleted: inner.deleted_snapshots,
            requested: inner.requested_snapshots,
            queue_len: inner.requested_snapshots.saturating_sub(inner.created_snapshots),
            last_requested_tx_offset: inner.last_requested_tx_offset,
            last_created_tx_offset: inner.last_created_tx_offset,
            live_offsets: inner.snapshots.keys().copied().collect(),
        }
    }

    pub fn observe_snapshot_request(&self, requested_at: Option<TxOffset>) {
        let mut inner = self.inner.lock().unwrap();
        inner.requested_snapshots += 1;
        inner.last_requested_tx_offset = requested_at;
    }

    fn missing_snapshot(tx_offset: TxOffset) -> SnapshotError {
        SnapshotError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            format!("snapshot {tx_offset} does not exist in DST memory repo"),
        ))
    }

    fn source_repo() -> PathBuf {
        PathBuf::from("<dst-memory-snapshot>")
    }
}

impl SnapshotRepo for InMemorySnapshotRepo {
    type Pending = BoxedPendingSnapshot;

    fn database_identity(&self) -> Identity {
        self.inner.lock().unwrap().database_identity
    }

    fn create_snapshot<'db>(
        &self,
        tables: &mut dyn Iterator<Item = &'db mut Table>,
        blobs: &'db dyn BlobStore,
        tx_offset: TxOffset,
    ) -> Result<Self::Pending, SnapshotError> {
        let inner = self.inner.lock().unwrap();
        let database_identity = inner.database_identity;
        let replica_id = inner.replica_id;
        drop(inner);

        let blobs = blobs
            .iter_blobs()
            .map(|(hash, uses, bytes)| StoredBlob {
                hash: *hash,
                uses,
                bytes: bytes.to_vec(),
            })
            .collect();

        let mut stored_tables = Vec::new();
        for table in tables {
            let pages = table
                .iter_pages_with_hashes()
                .map(|(hash, page)| {
                    let bytes = bsatn::to_vec(page).map_err(|cause| SnapshotError::Serialize {
                        ty: ObjectType::Page(hash),
                        cause,
                    })?;
                    Ok(StoredPage {
                        hash: *hash.as_bytes(),
                        bytes,
                    })
                })
                .collect::<Result<Vec<_>, SnapshotError>>()?;
            stored_tables.push(StoredTable {
                table_id: table.schema.table_id,
                pages,
            });
        }

        Ok(Box::new(InMemoryPendingSnapshot {
            repo: self.clone(),
            snapshot: StoredSnapshot {
                database_identity,
                replica_id,
                tx_offset,
                module_abi_version: CURRENT_MODULE_ABI_VERSION,
                blobs,
                tables: stored_tables,
            },
        }))
    }

    fn read_snapshot(&self, tx_offset: TxOffset, page_pool: &PagePool) -> Result<ReconstructedSnapshot, SnapshotError> {
        let snapshot = self
            .inner
            .lock()
            .unwrap()
            .snapshots
            .get(&tx_offset)
            .cloned()
            .ok_or_else(|| Self::missing_snapshot(tx_offset))?;

        let mut read_metrics = SnapshotReadMetrics::default();
        read_metrics.blob.files = snapshot.blobs.len() as u64;
        read_metrics.page.files = snapshot.tables.iter().map(|table| table.pages.len() as u64).sum();

        let mut blob_store = HashMapBlobStore::default();
        for blob in &snapshot.blobs {
            let computed = BlobHash::hash_from_bytes(&blob.bytes);
            if computed != blob.hash {
                return Err(SnapshotError::HashMismatch {
                    ty: ObjectType::Blob(blob.hash),
                    expected: blob.hash.data,
                    computed: computed.data,
                    source_repo: Self::source_repo(),
                });
            }
            read_metrics.blob.disk_bytes += blob.bytes.len() as u64;
            blob_store.insert_with_uses(&blob.hash, blob.uses, blob.bytes.clone().into_boxed_slice());
        }

        let mut tables = BTreeMap::new();
        for table in &snapshot.tables {
            let mut pages = Vec::with_capacity(table.pages.len());
            for stored_page in &table.pages {
                read_metrics.page.disk_bytes += stored_page.bytes.len() as u64;
                let page = page_pool.take_deserialize_from(&stored_page.bytes).map_err(|cause| {
                    SnapshotError::Deserialize {
                        ty: ObjectType::Snapshot,
                        source_repo: Self::source_repo(),
                        cause,
                    }
                })?;
                let computed = page.content_hash();
                if computed.as_bytes() != &stored_page.hash {
                    return Err(SnapshotError::HashMismatch {
                        ty: ObjectType::Page(computed),
                        expected: stored_page.hash,
                        computed: *computed.as_bytes(),
                        source_repo: Self::source_repo(),
                    });
                }
                pages.push(page);
            }
            tables.insert(table.table_id, pages);
        }

        Ok(ReconstructedSnapshot {
            database_identity: snapshot.database_identity,
            replica_id: snapshot.replica_id,
            tx_offset: snapshot.tx_offset,
            module_abi_version: snapshot.module_abi_version,
            blob_store,
            tables,
            compress_type: CompressType::None,
            read_metrics,
        })
    }

    fn latest_snapshot_older_than(&self, upper_bound: TxOffset) -> Result<Option<TxOffset>, SnapshotError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .snapshots
            .keys()
            .rev()
            .copied()
            .find(|offset| *offset <= upper_bound))
    }

    fn compress_snapshots(&self, stats: &mut CompressionStats, range: Range<TxOffset>) -> Result<(), SnapshotError> {
        for offset in self.inner.lock().unwrap().snapshots.keys().copied() {
            if range.contains(&offset) {
                stats.skipped += 1;
                stats.last_compressed = Some(offset);
            }
        }
        Ok(())
    }

    fn invalidate_newer_snapshots(&self, upper_bound: TxOffset) -> Result<(), SnapshotError> {
        let mut inner = self.inner.lock().unwrap();
        let newer = inner
            .snapshots
            .range((upper_bound.saturating_add(1))..)
            .map(|(offset, _)| *offset)
            .collect::<Vec<_>>();
        for offset in newer {
            if inner.snapshots.remove(&offset).is_some() {
                inner.deleted_snapshots += 1;
            }
        }
        Ok(())
    }

    fn invalidate_snapshot(&self, tx_offset: TxOffset) -> Result<(), SnapshotError> {
        let mut inner = self.inner.lock().unwrap();
        inner
            .snapshots
            .remove(&tx_offset)
            .map(|_| {
                inner.deleted_snapshots += 1;
            })
            .ok_or_else(|| Self::missing_snapshot(tx_offset))
    }
}

impl PendingSnapshot for InMemoryPendingSnapshot {
    fn sync_all(self: Box<Self>) -> Result<TxOffset, SnapshotError> {
        let InMemoryPendingSnapshot { repo, snapshot } = *self;
        let tx_offset = snapshot.tx_offset;
        let mut inner = repo.inner.lock().unwrap();
        inner.snapshots.insert(tx_offset, snapshot);
        inner.created_snapshots += 1;
        inner.last_created_tx_offset = Some(tx_offset);
        while inner.snapshots.len() > inner.retention {
            let Some(oldest) = inner.snapshots.keys().next().copied() else {
                break;
            };
            if inner.snapshots.remove(&oldest).is_some() {
                inner.deleted_snapshots += 1;
            }
        }
        Ok(tx_offset)
    }
}
