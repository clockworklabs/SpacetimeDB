use std::{io, sync::Arc};

use spacetimedb_memory_usage::MemoryUsage;

use crate::page_pool::PagePool;

mod blob_set;
mod budget;
mod error;
mod map;
mod mem;
pub mod metrics;
mod page_manager;
mod page_ref;
mod page_set;
mod storage;

pub use self::{
    blob_set::{BlobHandle, BlobMap, BlobReadSet, BlobSet},
    budget::{BudgetExceeded, BudgetPermit, ByteBudget, ByteBudgetConfig, ByteBudgetUsage},
    error::{BlobError, ObjectStoreError, PageError, TrimError},
    map::ObjectKey,
    mem::ResidentObjects,
    page_manager::{PageHandle, PageManager, ReservedPage},
    page_ref::{PageBorrow, PageRead, PageRef},
    page_set::{PageSet, PreparedCommit},
    storage::{
        BackingStore, LocalBackingStore, LocalStorage, MemoryObjectStore, Object, ObjectBytes, ObjectMetadata,
        ObjectStore, RemoteBackingStore, RemoteStorage, RemoveObject, TieredStorage,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// Always keep resident in main memory.
    Never,
    /// Allow to evict to local disk.
    Memory,
    /// Allow to evict from disk to remote storage.
    Disk,
}

impl EvictionPolicy {
    /// If `self` is [Self::Never], run `if_never`, or else `or_else`.
    pub fn never_or_else<T>(&self, if_never: impl FnOnce() -> T, or_else: impl FnOnce() -> T) -> T {
        match self {
            Self::Never => if_never(),
            Self::Memory | Self::Disk => or_else(),
        }
    }

    /// If `self` is [Self::Never], return `if_never`, or else `default`.
    pub fn never_or<T>(&self, if_never: T, default: T) -> T {
        match self {
            Self::Never => if_never,
            Self::Memory | Self::Disk => default,
        }
    }
}

pub struct TieredStorageConfig {
    pub page_pool: PagePool,
    pub resident_budget: ByteBudget,
    pub local_store: LocalBackingStore,
    pub remote_store: Option<RemoteBackingStore>,
}

impl TieredStorageConfig {
    pub fn with_metrics(mut self, metrics: metrics::Metrics) -> Self {
        self.resident_budget.metrics = metrics.clone();
        self.local_store.budget.metrics = metrics.clone();
        self.local_store.object_store.set_metrics(metrics);
        self
    }

    pub fn new_for_test() -> Self {
        Self::memory_unlimited(PagePool::new_for_test())
    }

    pub fn new_for_module_check(page_pool: PagePool) -> Self {
        Self::memory_unlimited(page_pool)
    }

    pub fn new_for_memory_database(page_pool: PagePool) -> Self {
        Self::memory_unlimited(page_pool)
    }

    fn memory_unlimited(page_pool: PagePool) -> Self {
        Self {
            page_pool,
            resident_budget: ByteBudget::unlimited(),
            local_store: BackingStore {
                object_store: Box::new(MemoryObjectStore::default()),
                budget: ByteBudget::new(ByteBudgetConfig {
                    tier: metrics::CacheTier::Disk,
                    ..ByteBudgetConfig::unlimited()
                })
                .unwrap(),
            },
            remote_store: None,
        }
    }
}

pub trait TieredStorageProvider: Send + Sync {
    fn tiered_storage(&self) -> impl Future<Output = io::Result<TieredStorageConfig>> + Send;
}

#[derive(Clone)]
pub struct TieredDatabaseStorage {
    pages: Arc<PageManager>,
    blobs: Arc<BlobSet>,

    cache: Arc<ResidentObjects>,
    store: Arc<TieredStorage>,
}

impl TieredDatabaseStorage {
    /// Create a storage handle with empty cache, pages and blobs.
    pub fn empty(config: TieredStorageConfig) -> Self {
        Self::from_snapshot(config, <_>::default())
    }

    /// Create a storage handle from the [BlobMap] obtained from a snapshot.
    ///
    /// Installs the blobs into a [BlobSet], but leaves the page manager empty.
    /// Snapshot restore subsequently installs the pages from the snapshot
    /// directly into tables, thereby populating the page manager and cache.
    ///
    /// [Self::trim] should be called after restore is complete, so that
    /// resident objects are evicted until the memory budget is met.
    pub fn from_snapshot(
        TieredStorageConfig {
            page_pool,
            local_store,
            remote_store,
            resident_budget,
        }: TieredStorageConfig,
        blobs: BlobMap,
    ) -> Self {
        let store = Arc::new(TieredStorage::empty(local_store, remote_store));
        let cache = Arc::new(ResidentObjects::new(resident_budget));
        let pages = Arc::new(PageManager::new(cache.clone(), page_pool, store.clone()));
        let blobs = Arc::new(BlobSet::from_blob_map(cache.clone(), store.clone(), blobs));

        Self {
            pages,
            blobs,
            cache,
            store,
        }
    }

    /// Attempt to evict from the shared blob set and local storage.
    pub fn trim(&self) -> Result<(), TrimError> {
        self.blobs.trim()?;
        self.store.trim()?;

        Ok(())
    }

    pub fn budgets(&self) -> [(metrics::CacheTier, &ByteBudget); 2] {
        [
            (metrics::CacheTier::Memory, self.cache.budget()),
            (metrics::CacheTier::Disk, self.store.local_budget()),
        ]
    }

    pub fn metrics(&self) -> &metrics::Metrics {
        self.cache.metrics()
    }

    pub fn pages(&self) -> &Arc<PageManager> {
        &self.pages
    }

    pub fn blobs(&self) -> &Arc<BlobSet> {
        &self.blobs
    }

    pub fn resident_bytes_used_by_blobs(&self) -> u64 {
        self.blobs.resident_bytes_used_by_blobs()
    }

    pub fn new_empty_blob_set(&self) -> BlobSet {
        BlobSet::new(self.cache.clone(), self.store.clone())
    }
}

impl MemoryUsage for TieredDatabaseStorage {
    fn heap_usage(&self) -> usize {
        todo!()
    }
}
