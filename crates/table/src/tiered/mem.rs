use super::metrics::Metrics;
use std::{
    cmp, fmt,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Weak,
    },
};

use parking_lot::{ArcRwLockReadGuard, ArcRwLockWriteGuard, Mutex, RawRwLock, RwLock, RwLockWriteGuard};
use spacetimedb_data_structures::map::{hash_map, IntMap, ValidAsIdentityHash};
use spacetimedb_memory_usage::MemoryUsage;

use crate::{
    blob_store::BlobHash,
    indexes::PAGE_SIZE,
    page::Page,
    page_pool::PagePool,
    tiered::{
        budget::ByteBudgetStateView, error::EvictionError, map::BlobHashWrapper, metrics::EvictionDirection,
        BudgetExceeded, BudgetPermit, ByteBudget, ByteBudgetUsage,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageId(u64);

impl std::hash::Hash for PageId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

impl ValidAsIdentityHash for PageId {}

pub struct ReservedBlob {
    frame: Arc<BlobFrame>,
}

#[derive(Debug)]
pub struct BlobFrame {
    hash: BlobHashWrapper,
    bytes: Box<[u8]>,
    access: Access,
    #[allow(unused)]
    permit: BudgetPermit,
}

impl BlobFrame {
    pub fn hash(&self) -> BlobHashWrapper {
        self.hash
    }

    pub fn size(&self) -> u64 {
        self.bytes.len() as _
    }

    fn access(&self) -> AccessSnapshot {
        self.access.snapshot()
    }

    fn touch(&self, epoch: u64) {
        self.access.touch(epoch);
    }
}

impl Deref for BlobFrame {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}

pub struct ReservedPage {
    id: PageId,
    frame: PageFrame,
}

impl ReservedPage {
    /// An unpublished reservation has no external pins or registry entry.
    pub(super) fn into_resident(self) -> (Box<Page>, BudgetPermit) {
        self.frame.into_owned()
    }
}

#[derive(Debug)]
pub struct PageFrame {
    pub(super) id: PageId,
    page: Arc<RwLock<PooledPage>>,
    access: Access,
    #[allow(unused)]
    permit: BudgetPermit,
}

impl PageFrame {
    /// Remove the frame's lock without copying the page or releasing its memory charge.
    fn into_owned(self) -> (Box<Page>, BudgetPermit) {
        let Self { page, permit, .. } = self;
        let page = Arc::try_unwrap(page)
            .expect("page ownership transfer requires all read and write guards to be dropped")
            .into_inner();
        (page.into_owned(), permit)
    }

    pub fn read(&self) -> ArcRwLockReadGuard<RawRwLock, PooledPage> {
        RwLock::read_arc(&self.page)
    }

    pub fn write(&self) -> ArcRwLockWriteGuard<RawRwLock, PooledPage> {
        RwLock::write_arc(&self.page)
    }

    pub fn write_ref(&self) -> RwLockWriteGuard<'_, PooledPage> {
        self.page.write()
    }

    fn access(&self) -> AccessSnapshot {
        self.access.snapshot()
    }

    fn touch(&self, epoch: u64) {
        self.access.touch(epoch);
    }
}

impl MemoryUsage for PageFrame {
    fn heap_usage(&self) -> usize {
        let refcount = std::mem::size_of::<usize>() * 2;

        refcount
            + std::mem::size_of::<RwLock<PooledPage>>()
            + std::mem::size_of::<Access>()
            + std::mem::size_of::<BudgetPermit>()
            + self.page.read().heap_usage()
    }
}

/// Tracks resident objects and controls the residency budget.
///
/// The objects, [PageFrame]s and [BlobFrame]s, are owned by [super::PageSet]
/// respectively [super::BlobSet]. The [ResidentObjects] only tracks [Weak]
/// references to them.
///
/// To make an object resident, a reservation must be acquired via
/// [ResidentObjects::reserve_blob] respectively [ResidentObjects::reserve_page].
/// This may fail because the available [ByteBudget] is already used.
///
/// Once a reservation has been successfully acquired, it may be exchanged for
/// its actual "frame" (allocation), see [ResidentObjects::redeem_blob],
/// [ResidentObjects::redeem_page].
///
/// NOTE: This two-step process is deliberate, as it reduces the surface for
/// lock-ordering bugs in the owning datastructures.
///
/// [PageFrame]s and [BlobFrame]s consume permits from the [ByteBudget] until
/// they're dropped. [ReservedPage]s and [ReservedBlob]s contain frames, so
/// count towards the budget even while not redeemed.
///
/// If a reservation attempt fails, callers may request eviction candidates
/// (see: [ResidentObjects::evictable_blobs], [ResidentObjects::evictable_pages]).
///
/// Eviction candidates are objects that have only a single strong reference,
/// and are sorted according to an LRU-2 algorithm. Note that, for the latter to
/// work, the owners must call [ResidentObjects::touch_blob] respectively
/// [ResidentObjects::touch_page] whenever they hand out an object reference.
#[derive(Debug)]
pub struct ResidentObjects {
    budget: ByteBudget,
    access_epoch: AtomicU64,
    page_id: AtomicU64,
    pages: Mutex<IntMap<PageId, Weak<PageFrame>>>,
    blobs: Mutex<IntMap<BlobHashWrapper, Weak<BlobFrame>>>,
    eviction: Arc<Eviction>,
}

impl ResidentObjects {
    pub(super) fn budget(&self) -> &ByteBudget {
        &self.budget
    }

    pub fn metrics(&self) -> &Metrics {
        self.budget.metrics()
    }

    pub(super) fn has_unlimited_budget(&self) -> bool {
        self.budget.is_unlimited()
    }

    fn acquire(&self, bytes: u64, memory_limit_exempt: bool) -> Result<BudgetPermit, BudgetExceeded> {
        if memory_limit_exempt {
            // Exempt allocations still count toward the database's resident usage.
            Ok(self.budget.force_acquire(bytes))
        } else {
            self.budget.acquire(bytes)
        }
    }

    pub(super) fn acquire_resident_page(&self, memory_limit_exempt: bool) -> Result<BudgetPermit, BudgetExceeded> {
        self.acquire(PAGE_SIZE as u64, memory_limit_exempt)
    }

    pub(super) fn restored_page_permit(&self) -> BudgetPermit {
        self.budget.force_acquire(PAGE_SIZE as u64)
    }

    pub fn new(budget: ByteBudget) -> Self {
        let eviction = Eviction {
            usage: budget.budget_view(),
            sources: <_>::default(),
        };
        Self {
            budget,
            access_epoch: AtomicU64::new(1),
            page_id: <_>::default(),
            pages: <_>::default(),
            blobs: <_>::default(),
            eviction: Arc::new(eviction),
        }
    }

    pub(super) fn reserve_blob(
        &self,
        size: u64,
        hash: BlobHashWrapper,
        memory_limit_exempt: bool,
        alloc: impl FnOnce() -> Box<[u8]>,
    ) -> Result<ReservedBlob, BudgetExceeded> {
        // Transaction and committed blob set share this registry.
        // Hand out an existing frame if there is one.
        if let Some(frame) = self.blobs.lock().get(&hash).and_then(Weak::upgrade) {
            return Ok(ReservedBlob { frame });
        }
        let permit = self.acquire(size, memory_limit_exempt)?;
        let bytes = alloc();
        assert_eq!(size, bytes.len() as u64);
        debug_assert_eq!(hash, BlobHashWrapper::hash_from_bytes(&bytes));
        let hash = BlobHash::hash_from_bytes(&bytes).into();
        let frame = BlobFrame {
            hash,
            bytes,
            access: Access::new(self.access_epoch.fetch_add(1, Ordering::Relaxed)),
            permit,
        }
        .into();

        Ok(ReservedBlob { frame })
    }

    pub(super) fn redeem_blob(&self, ReservedBlob { frame }: ReservedBlob) -> Arc<BlobFrame> {
        let key = frame.hash;
        let mut blobs = self.blobs.lock();
        // Transaction and committed blob sets share this registry. Reuse live
        // frames and replace stale weak entries when a hash becomes resident again.
        if let Some(existing) = blobs.get(&key).and_then(Weak::upgrade) {
            assert_eq!(existing.hash(), frame.hash());
            assert!(Arc::ptr_eq(&existing, &frame));

            return existing;
        }
        blobs.insert(key, Arc::downgrade(&frame));
        frame
    }

    pub(super) fn evictable_blobs(&self, candidates: &mut Vec<BlobEvictionCandidate>) {
        {
            // - one ref is from upgrading the weak reference below
            // - one ref is held by a blob set
            const EVICTABLE_REFCOUNT: usize = 2;

            let mut blobs = self.blobs.lock();
            blobs.retain(|&key, weak| {
                let Some(frame) = weak.upgrade() else {
                    return false;
                };

                if Arc::strong_count(&frame) <= EVICTABLE_REFCOUNT {
                    candidates.push(BlobEvictionCandidate {
                        key,
                        access: frame.access(),
                    });
                }

                true
            });
        }
        candidates.sort_unstable_by_key(|candidate| candidate.access);
    }

    pub(super) fn evict_blob(&self, victim: BlobEvictionCandidate) {
        let mut blobs = self.blobs.lock();
        if let hash_map::Entry::Occupied(entry) = blobs.entry(victim.key)
            && entry.get().strong_count() == 0
        {
            entry.remove();
        }
    }

    pub(super) fn touch_blob(&self, frame: &BlobFrame) {
        frame.touch(self.access_epoch.fetch_add(1, Ordering::Relaxed));
    }

    pub(super) fn reserve_page<E: From<BudgetExceeded>>(
        &self,
        memory_limit_exempt: bool,
        alloc: impl FnOnce() -> Result<PooledPage, E>,
    ) -> Result<ReservedPage, E> {
        let permit = self.acquire_resident_page(memory_limit_exempt)?;
        let page = alloc()?;
        Ok(self.reserve_page_with_permit(permit, page))
    }

    pub(super) fn redeem_page(&self, ReservedPage { id, frame }: ReservedPage) -> Arc<PageFrame> {
        let frame = Arc::new(frame);
        assert!(self.pages.lock().insert(id, Arc::downgrade(&frame)).is_none());
        frame
    }

    pub(super) fn evictable_pages(&self, candidates: &mut Vec<PageEvictionCandidate>) {
        {
            let mut pages = self.pages.lock();
            pages.retain(|&id, weak| {
                let Some(frame) = weak.upgrade() else {
                    return false;
                };

                if Arc::strong_count(&frame) <= 2 {
                    candidates.push(PageEvictionCandidate {
                        id,
                        access: frame.access(),
                    });
                }

                true
            });
        }

        candidates.sort_unstable_by_key(|candidate| candidate.access);
    }

    pub(super) fn evict_page(&self, victim: PageEvictionCandidate) {
        let mut pages = self.pages.lock();
        if let hash_map::Entry::Occupied(entry) = pages.entry(victim.id)
            && entry.get().strong_count() == 0
        {
            entry.remove();
        }
    }

    pub(super) fn touch_page(&self, frame: &PageFrame) {
        frame.touch(self.access_epoch.fetch_add(1, Ordering::Relaxed));
    }

    pub fn usage(&self) -> ByteBudgetUsage {
        self.budget.usage()
    }

    pub(super) fn register_page_unchecked(&self, pool: PagePool, page: Box<Page>) -> Arc<PageFrame> {
        let permit = self.budget.force_acquire(PAGE_SIZE as u64);
        let reservation = self.reserve_page_with_permit(permit, PooledPage::new(page, pool));
        self.redeem_page(reservation)
    }

    pub(super) fn detach_page(&self, frame: Arc<PageFrame>) -> (Box<Page>, BudgetPermit) {
        // The memory table will own the page directly. Remove the frame from
        // eviction tracking, but keep its allocation and memory charge alive.
        assert!(self.pages.lock().remove(&frame.id).is_some());
        Arc::try_unwrap(frame)
            .expect("page ownership transfer requires all eviction pins to be dropped")
            .into_owned()
    }

    pub(super) fn register_blob_unchecked(&self, blob: Box<[u8]>) -> Arc<BlobFrame> {
        let permit = self.budget.force_acquire(blob.len() as u64);
        let reservation = self.reserve_blob_with_permit(permit, blob);
        self.redeem_blob(reservation)
    }

    pub(super) fn reserve_page_with_permit(&self, permit: BudgetPermit, page: PooledPage) -> ReservedPage {
        let id = PageId(self.page_id.fetch_add(1, Ordering::Relaxed));
        let frame = PageFrame {
            id,
            page: Arc::new(RwLock::new(page)),
            access: Access::new(self.access_epoch.fetch_add(1, Ordering::Relaxed)),
            permit,
        };

        ReservedPage { id, frame }
    }

    fn reserve_blob_with_permit(&self, permit: BudgetPermit, bytes: Box<[u8]>) -> ReservedBlob {
        let hash = BlobHashWrapper::hash_from_bytes(&bytes);
        let frame = BlobFrame {
            hash,
            bytes,
            access: Access::new(self.access_epoch.fetch_add(1, Ordering::Relaxed)),
            permit,
        }
        .into();

        ReservedBlob { frame }
    }

    pub(super) fn trim(&self) -> Result<u64, EvictionError> {
        self.evict(self.budget.config().low_water_bytes)
    }

    pub(super) fn evict(&self, target_usage: u64) -> Result<u64, EvictionError> {
        let timer = self.metrics().timer();
        let result = self.eviction.evict(target_usage);
        self.metrics().eviction(EvictionDirection::MemoryToDisk, timer.nanos());
        result
    }

    pub(super) fn register_eviction_source(&self, source: &Arc<dyn EvictionSource>) {
        self.eviction.register(source);
    }
}

impl MemoryUsage for ResidentObjects {
    fn heap_usage(&self) -> usize {
        let Self {
            budget,
            access_epoch: _,
            page_id: _,
            pages,
            blobs,
            eviction,
        } = self;
        // `pages` and `blobs` contain only weak references, so report only the
        // size of the map allocations.
        pages.lock().allocation_size() + blobs.lock().allocation_size() + budget.heap_usage() + eviction.heap_usage()
    }
}

#[derive(Debug)]
pub struct BlobEvictionCandidate {
    pub key: BlobHashWrapper,
    access: AccessSnapshot,
}

#[derive(Debug)]
pub struct PageEvictionCandidate {
    pub id: PageId,
    access: AccessSnapshot,
}

#[derive(Debug)]
struct Access {
    last_access_epoch: AtomicU64,
    prev_access_epoch: AtomicU64,
}

impl Access {
    pub fn new(epoch: u64) -> Self {
        Self {
            last_access_epoch: AtomicU64::new(epoch),
            prev_access_epoch: <_>::default(),
        }
    }

    pub fn snapshot(&self) -> AccessSnapshot {
        AccessSnapshot {
            last_access_epoch: self.last_access_epoch.load(Ordering::Relaxed),
            prev_access_epoch: self.prev_access_epoch.load(Ordering::Relaxed),
        }
    }

    pub fn touch(&self, epoch: u64) {
        let prev = self.last_access_epoch.swap(epoch, Ordering::Relaxed);
        self.prev_access_epoch.store(prev, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Debug)]
struct AccessSnapshot {
    last_access_epoch: u64,
    prev_access_epoch: u64,
}

impl Ord for AccessSnapshot {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn key(
            &AccessSnapshot {
                last_access_epoch,
                prev_access_epoch,
            }: &AccessSnapshot,
        ) -> (bool, u64, u64) {
            (
                prev_access_epoch != 0,
                if prev_access_epoch == 0 {
                    last_access_epoch
                } else {
                    prev_access_epoch
                },
                last_access_epoch,
            )
        }

        key(self).cmp(&key(other))
    }
}

impl PartialOrd for AccessSnapshot {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for AccessSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}

impl Eq for AccessSnapshot {}

pub(super) struct PooledPage {
    page: ManuallyDrop<Box<Page>>,
    pool: PagePool,
}

impl PooledPage {
    fn into_owned(self) -> Box<Page> {
        let mut this = ManuallyDrop::new(self);
        // SAFETY: `this` suppresses PooledPage::drop, and both fields are moved
        // exactly once. The receiving resident page set assumes pool ownership.
        unsafe {
            let page = ManuallyDrop::take(&mut this.page);
            std::ptr::drop_in_place(&mut this.pool);
            page
        }
    }

    pub fn new(page: Box<Page>, pool: PagePool) -> Self {
        Self {
            page: ManuallyDrop::new(page),
            pool,
        }
    }
}

impl Drop for PooledPage {
    fn drop(&mut self) {
        // SAFETY: We're in `drop`, so won't be using `ManuallyDrop` again.
        let page = unsafe { ManuallyDrop::take(&mut self.page) };
        self.pool.put(page);
    }
}

impl Deref for PooledPage {
    type Target = Page;

    fn deref(&self) -> &Self::Target {
        &self.page
    }
}

impl DerefMut for PooledPage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.page
    }
}

impl fmt::Debug for PooledPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PooledPage").field("page", &self.page).finish()
    }
}

pub(super) trait EvictionSource: fmt::Debug + Send + Sync {
    fn evict(&self, target: u64) -> Result<u64, EvictionError>;
}

#[derive(Debug)]
struct Eviction {
    usage: ByteBudgetStateView,
    sources: Mutex<Vec<Weak<dyn EvictionSource>>>,
}

impl Eviction {
    fn register(&self, source: &Arc<dyn EvictionSource>) {
        self.sources.lock().push(Arc::downgrade(source))
    }

    fn evict(&self, target: u64) -> Result<u64, EvictionError> {
        let mut bytes_freed = 0;
        let sources = {
            let mut sources = self.sources.lock();
            sources.retain(|s| s.strong_count() > 0);
            sources.iter().filter_map(Weak::upgrade).collect::<Vec<_>>()
        };

        for source in sources {
            if self.usage.usage().accounted_bytes <= target {
                break;
            }

            bytes_freed += source.evict(target)?;
        }

        Ok(bytes_freed)
    }
}

impl MemoryUsage for Eviction {
    fn heap_usage(&self) -> usize {
        // Don't count the budget, as it is already accounted for by
        // [ResidentObjects].
        self.sources.lock().capacity() + size_of::<Weak<dyn EvictionSource>>()
    }
}
