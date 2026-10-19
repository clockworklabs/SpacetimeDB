use std::sync::Arc;

use spacetimedb_memory_usage::MemoryUsage;

use crate::page_pool::PagePool;

mod blob_set;
mod budget;
mod error;
mod map;
mod mem;
mod page_manager;
mod page_ref;
pub use page_ref::{PageBorrow, PageRead, PageRef};
mod page_set;
mod storage;

pub use self::{
    blob_set::{BlobHandle, BlobMap, BlobReadSet, BlobSet},
    budget::{BudgetExceeded, BudgetPermit, ByteBudget, ByteBudgetConfig, ByteBudgetUsage},
    error::{BlobError, ObjectStoreError, PageError},
    mem::ResidentObjects,
    page_manager::{PageEvictionPolicy, PageHandle, PageManager, ReservedPage},
    page_set::{DeleteRowError, PageSet, PreparedCommit},
    storage::{MemoryObjectStore, ObjectStore, TieredStorage},
};

pub struct TieredStorageConfig {
    pub page_pool: PagePool,
    pub local_object_store: Box<dyn ObjectStore>,
    pub remote_object_store: Option<Box<dyn ObjectStore>>,
    pub resident_budget: ByteBudget,
}

impl TieredStorageConfig {
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
            local_object_store: Box::new(MemoryObjectStore::default()),
            remote_object_store: None,
            resident_budget: ByteBudget::unlimited(),
        }
    }
}

#[derive(Clone)]
pub struct TieredDatabaseStorage {
    pages: Arc<PageManager>,
    blobs: Arc<BlobSet>,

    cache: Arc<ResidentObjects>,
    store: Arc<TieredStorage>,
}

impl TieredDatabaseStorage {
    pub fn new(
        TieredStorageConfig {
            page_pool,
            resident_budget,
            local_object_store,
            remote_object_store,
        }: TieredStorageConfig,
    ) -> Self {
        let store = Arc::new(TieredStorage::new(local_object_store, remote_object_store));
        let cache = Arc::new(ResidentObjects::new(resident_budget));
        let pages = Arc::new(PageManager::new(cache.clone(), page_pool, store.clone()));
        let blobs = Arc::new(BlobSet::new(cache.clone(), store.clone()));

        Self {
            pages,
            blobs,
            cache,
            store,
        }
    }

    pub fn with_blobs(config: TieredStorageConfig, blobs: BlobMap) -> Self {
        let mut this = Self::new(config);
        this.blobs = BlobSet::from_blob_map(this.cache.clone(), this.store.clone(), blobs).into();
        this
    }

    pub fn pages(&self) -> &Arc<PageManager> {
        &self.pages
    }

    pub fn blobs(&self) -> &Arc<BlobSet> {
        &self.blobs
    }

    pub fn physical_bytes_used_by_blobs(&self) -> u64 {
        self.blobs.physical_bytes_used_by_blobs()
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
