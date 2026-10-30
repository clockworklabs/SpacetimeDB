use std::{io, iter, sync::Arc};

use spacetimedb_memory_usage::MemoryUsage;

use crate::{indexes::PAGE_SIZE, page::Page, page_pool::PagePool, tiered::map::PageHash};

mod blob_set;
mod budget;
mod error;
mod map;
mod mem;
mod page_manager;
mod page_ref;
mod page_set;
mod storage;

pub use self::{
    blob_set::{BlobHandle, BlobMap, BlobReadSet, BlobSet},
    budget::{BudgetExceeded, BudgetPermit, ByteBudget, ByteBudgetConfig, ByteBudgetUsage},
    error::{BlobError, ObjectStoreError, PageError},
    map::ObjectKey,
    mem::ResidentObjects,
    page_manager::{PageEvictionPolicy, PageHandle, PageManager, ReservedPage},
    page_ref::{PageBorrow, PageRead, PageRef},
    page_set::{PageSet, PreparedCommit},
    storage::{BackingStore, MemoryObjectStore, Object, ObjectBytes, ObjectMetadata, ObjectStore, TieredStorage},
};

pub struct TieredStorageConfig {
    pub page_pool: PagePool,
    pub resident_budget: ByteBudget,
    pub local_store: BackingStore,
    pub remote_store: Option<BackingStore>,
    /// Allow tables to evict pages according to their cache policy.
    pub evict_pages: bool,
    /// Allow blob eviction. Disable this when tables need their blobs to stay in
    /// memory: blobs can be shared, and their references do not track table policies.
    pub evict_blobs: bool,
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
            resident_budget: ByteBudget::unlimited(),
            local_store: BackingStore {
                object_store: Box::new(MemoryObjectStore::default()),
                budget: ByteBudget::unlimited(),
            },
            remote_store: None,
            evict_pages: false,
            evict_blobs: true,
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
    evict_pages: bool,
}

impl TieredDatabaseStorage {
    pub fn empty(config: TieredStorageConfig) -> io::Result<Self> {
        Self::from_snapshot(config, <_>::default(), iter::empty())
    }

    pub fn from_snapshot<'a>(
        TieredStorageConfig {
            page_pool,
            local_store,
            remote_store,
            resident_budget,
            evict_pages,
            evict_blobs,
        }: TieredStorageConfig,
        blobs: BlobMap,
        pages: impl Iterator<Item = &'a Page>,
    ) -> io::Result<Self> {
        let store = Arc::new(TieredStorage::with_catalog(
            local_store,
            remote_store,
            blobs
                .iter()
                .map(|(hash, entry)| (hash.into_object_key(), entry.blob.len() as u64))
                .chain(pages.map(|page| {
                    let key = PageHash::from(page).into();
                    (key, PAGE_SIZE as u64)
                })),
        ));
        let cache = Arc::new(ResidentObjects::new(resident_budget).with_blob_eviction(evict_blobs));
        let pages = Arc::new(PageManager::new(cache.clone(), page_pool, store.clone()));
        let blobs = Arc::new(BlobSet::from_blob_map(cache.clone(), store.clone(), blobs));

        Ok(Self {
            pages,
            blobs,
            cache,
            store,
            evict_pages,
        })
    }

    pub fn pages(&self) -> &Arc<PageManager> {
        &self.pages
    }

    pub fn page_eviction_enabled(&self) -> bool {
        self.evict_pages
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
