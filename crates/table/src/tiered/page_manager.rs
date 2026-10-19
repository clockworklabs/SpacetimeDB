use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex},
};

use parking_lot::{ArcRwLockReadGuard, ArcRwLockWriteGuard, RawRwLock, RwLock, RwLockWriteGuard};
use spacetimedb_data_structures::object_pool::{Pool, PooledObject};
use spacetimedb_lib::bsatn;
use spacetimedb_memory_usage::MemoryUsage;
use spacetimedb_sats::layout::Size;

use crate::{
    indexes::{max_rows_in_page, PAGE_SIZE},
    page::{Page, PageMetadata},
    page_pool::PagePool,
    tiered::{
        error::PageError,
        map::{ObjectKey, PageHash},
        mem::{self, PageEvictionCandidate, PageFrame, PageId, PooledPage},
        storage::TieredStorage,
        BudgetExceeded, BudgetPermit, ByteBudget, ObjectStoreError, ResidentObjects,
    },
};

#[cfg(test)]
use crate::var_len::VarLenMembers;

/// RAII read lock guard that dereferences to [Page].
///
/// Created by [PageFrame::read()].
pub struct PageFrameReadGuard {
    guard: ArcRwLockReadGuard<RawRwLock, PooledPage>,
    #[allow(unused)]
    pin: Arc<PageFrame>,
}

impl Deref for PageFrameReadGuard {
    type Target = Page;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

/// RAII write lock guard that dereferences to [Page].
///
/// Dropping the guard marks the corresponding [PageSlot] dirty and recomputes
/// the [PageMetadata].
///
/// Created by [PageFrame::write].
pub struct PageFrameWriteGuard {
    // NOTE: declaration order ensures locks are released in reverse acquisition
    // order.
    /// Write guard for the page allocation.
    page_guard: ArcRwLockWriteGuard<RawRwLock, PooledPage>,
    /// Write guard for the page slot.
    /// When the guard is dropped, the slot needs to be marked dirty and the
    /// page metadata updated.
    slot_guard: ArcRwLockWriteGuard<RawRwLock, PageSlot>,

    /// The locks don't consume a strong count for the frame, so we keep one
    /// here to keep it pinned.
    pin: Arc<PageFrame>,

    /// Needed to compute the metadata.
    fixed_row_size: Size,
}

impl PageFrameWriteGuard {
    pub fn into_page_handle(self) -> PageHandle {
        PageHandle {
            frame: Arc::clone(&self.pin),
        }
    }
}

impl Deref for PageFrameWriteGuard {
    type Target = Page;

    fn deref(&self) -> &Self::Target {
        &self.page_guard
    }
}

impl DerefMut for PageFrameWriteGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.page_guard
    }
}

impl Drop for PageFrameWriteGuard {
    fn drop(&mut self) {
        let new_metadata = self.page_guard.metadata(self.fixed_row_size);
        let PageSlot::Resident {
            handle: _,
            metadata,
            backing,
            eviction_policy: _,
        } = &mut *self.slot_guard
        else {
            unreachable!("slot lock serves as proof that page is resident")
        };
        *metadata = new_metadata;
        backing.take();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageEvictionPolicy {
    Evictable,
    NeverEvict,
}

/// A handle to a [PageSlot], i.e. a page that can be absent, resident, or
/// non-resident.
///
/// The public API is read-only.
#[derive(Clone, Debug)]
pub struct PageSlotHandle {
    slot: Arc<RwLock<PageSlot>>,
}

impl PageSlotHandle {
    pub fn is_resident(&self) -> bool {
        self.slot.read().is_resident()
    }

    pub fn is_absent(&self) -> bool {
        self.slot.read().is_absent()
    }

    pub fn has_space_for_row(&self, fixed_row_size: Size, num_var_len_granules: usize) -> Option<bool> {
        self.slot.read().has_space_for_row(fixed_row_size, num_var_len_granules)
    }

    pub fn is_full(&self, fixed_row_size: Size) -> Option<bool> {
        self.slot.read().is_full(fixed_row_size)
    }

    pub fn available_var_len_granules(&self) -> Option<usize> {
        self.slot.read().available_var_len_granules()
    }

    pub fn bytes_used_by_rows(&self) -> usize {
        self.slot.read().bytes_used_by_rows()
    }

    pub fn metadata(&self) -> Option<PageMetadata> {
        self.slot.read().metadata()
    }

    pub fn page(&self) -> Option<PageHandle> {
        self.slot.read().page().cloned()
    }

    pub fn page_id(&self) -> Option<PageId> {
        self.slot.read().page_id()
    }

    pub(super) fn write_ref(&self) -> RwLockWriteGuard<'_, PageSlot> {
        self.slot.write()
    }

    fn write_owned(&self) -> ArcRwLockWriteGuard<RawRwLock, PageSlot> {
        self.slot.write_arc()
    }

    pub(super) fn free(&self) {
        let mut slot = self.slot.write();
        *slot = PageSlot::Absent;
    }
}

#[cfg(test)]
impl PageSlotHandle {
    pub unsafe fn reconstruct_bytes_used_by_rows(
        &self,
        fixed_row_size: Size,
        var_len_visitor: &impl VarLenMembers,
    ) -> usize {
        let slot = self.slot.read();
        unsafe { slot.reconstruct_bytes_used_by_rows(fixed_row_size, var_len_visitor) }
    }

    pub fn reconstruct_num_rows(&self) -> usize {
        self.slot.read().reconstruct_num_rows()
    }
}

impl MemoryUsage for PageSlotHandle {
    fn heap_usage(&self) -> usize {
        let maybe_resident = {
            let slot = self.slot.read();
            slot.page().cloned()
        };
        let refcounts = std::mem::size_of::<usize>() * 2;
        refcounts + std::mem::size_of::<Mutex<PageSlot>>() + maybe_resident.as_ref().map_or(0, MemoryUsage::heap_usage)
    }
}

/// State of a page.
#[derive(Debug)]
#[allow(unused)]
pub enum PageSlot {
    /// The page has been freed, so the slot is empty.
    /// This exists to maintain stable [PageIndex]es.
    Absent,
    /// The page is resident in memory.
    Resident {
        handle: PageHandle,
        metadata: PageMetadata,
        // `Some` if the page is clean and present in backing storage.
        backing: Option<PageHash>,
        eviction_policy: PageEvictionPolicy,
    },
    /// The page is not resident in memory.
    #[allow(unused)]
    NonResident { hash: PageHash, metadata: PageMetadata },
}

impl PageSlot {
    pub fn is_resident(&self) -> bool {
        matches!(self, Self::Resident { .. })
    }

    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    pub fn has_space_for_row(&self, fixed_row_size: Size, num_var_len_granules: usize) -> Option<bool> {
        match self {
            Self::Absent => None,
            Self::Resident { metadata, .. } | PageSlot::NonResident { metadata, .. } => {
                Some(metadata.has_space_for_row(fixed_row_size, num_var_len_granules))
            }
        }
    }

    pub fn is_full(&self, fixed_row_size: Size) -> Option<bool> {
        match self {
            Self::Absent => None,
            Self::Resident { metadata, .. } | PageSlot::NonResident { metadata, .. } => {
                Some(metadata.is_full(fixed_row_size))
            }
        }
    }

    pub fn available_var_len_granules(&self) -> Option<usize> {
        match self {
            Self::Absent => None,
            Self::Resident { metadata, .. } | PageSlot::NonResident { metadata, .. } => {
                Some(metadata.available_var_len_granules())
            }
        }
    }

    pub fn bytes_used_by_rows(&self) -> usize {
        match self {
            Self::Absent => 0,
            Self::Resident { metadata, .. } | PageSlot::NonResident { metadata, .. } => {
                metadata.bytes_used_by_rows as _
            }
        }
    }

    pub fn metadata(&self) -> Option<PageMetadata> {
        match self {
            Self::Absent => None,
            Self::Resident { metadata, .. } | Self::NonResident { metadata, .. } => Some(*metadata),
        }
    }

    pub fn page_id(&self) -> Option<PageId> {
        match self {
            Self::Resident { handle, .. } => Some(handle.page_id()),
            Self::NonResident { .. } | Self::Absent => None,
        }
    }

    fn page(&self) -> Option<&PageHandle> {
        match self {
            Self::Absent | PageSlot::NonResident { .. } => None,
            Self::Resident { handle, .. } => Some(handle),
        }
    }
}

#[cfg(test)]
impl PageSlot {
    pub unsafe fn reconstruct_bytes_used_by_rows(
        &self,
        fixed_row_size: Size,
        var_len_visitor: &impl VarLenMembers,
    ) -> usize {
        match self {
            PageSlot::Absent => 0,
            PageSlot::Resident { handle, .. } => {
                let page = handle.read();
                unsafe { page.reconstruct_bytes_used_by_rows(fixed_row_size, var_len_visitor) }
            }
            PageSlot::NonResident { metadata, .. } => {
                use crate::var_len::VarLenGranule;

                let fixed_row_bytes = metadata.num_rows as usize * fixed_row_size.len();
                let var_len_bytes = metadata.available_var_len_granules() * VarLenGranule::SIZE.len();
                fixed_row_bytes + var_len_bytes
            }
        }
    }

    pub fn reconstruct_num_rows(&self) -> usize {
        match self {
            PageSlot::Absent => 0,
            PageSlot::Resident { handle, .. } => handle.read().reconstruct_num_rows(),
            PageSlot::NonResident { metadata, .. } => metadata.num_rows as _,
        }
    }
}

/// A handle to a page.
///
/// Guarantees that the page is resident for as long as the handle is live.
#[derive(Clone, Debug)]
pub struct PageHandle {
    frame: Arc<PageFrame>,
}

impl PageHandle {
    pub fn page_id(&self) -> PageId {
        self.frame.id
    }

    pub fn read(&self) -> PageFrameReadGuard {
        PageFrameReadGuard {
            guard: self.frame.read(),
            pin: self.frame.clone(),
        }
    }

    pub const fn size(&self) -> u64 {
        PAGE_SIZE as u64
    }

    pub(super) fn serialize_for_storage(&self) -> (PageHash, Vec<u8>) {
        let mut guard = self.frame.write_ref();
        let hash = PageHash::from(&mut **guard);
        let guard = RwLockWriteGuard::downgrade(guard);
        let bytes = bsatn::to_vec(&**guard).unwrap();

        (hash, bytes)
    }

    /// Obtain a write lock for the page allocation, while holding a write lock
    /// on the page slot.
    ///
    /// The method is marked unsafe to enforce stating the below safety
    /// invariants in a comment at call sites.
    ///
    /// # Safety
    ///
    /// The caller guarantees that the page is faulted, i.e. the slot is
    /// `PageSlot::Resident`. The slot must contain this handle's [PageFrame].
    unsafe fn write(
        &self,
        slot_guard: ArcRwLockWriteGuard<RawRwLock, PageSlot>,
        fixed_row_size: Size,
    ) -> PageFrameWriteGuard {
        assert!(matches!(
            &*slot_guard,
            PageSlot::Resident { handle, .. }
                if Arc::ptr_eq(&handle.frame, &self.frame)
        ));
        PageFrameWriteGuard {
            page_guard: self.frame.write(),
            slot_guard,

            pin: self.frame.clone(),

            fixed_row_size,
        }
    }
}

impl MemoryUsage for PageHandle {
    fn heap_usage(&self) -> usize {
        let refcount = std::mem::size_of::<usize>() * 2;
        let buffer_heap_usage = self.frame.heap_usage();

        refcount + buffer_heap_usage
    }
}

/// A page allocation, along with its [PageMetadata].
///
/// Created during commit planning ([crate::table::Table::prepare_commit()]),
/// respectively [PageManager::reserve()]. A registered page can be registered
/// with the page manager infallibly (see [PageManager::redeem()]).
pub struct ReservedPage {
    inner: mem::ReservedPage,
    metadata: PageMetadata,
}

impl ReservedPage {
    pub(super) fn into_resident(self) -> (Box<Page>, BudgetPermit) {
        self.inner.into_resident()
    }
}

/// A cached page remains allocated and charged to its original memory budget.
/// Moving its permit with it avoids locking the accounting counter on every reuse.
/// Dropping this pair when the cache is full or destroyed releases both resources.
#[derive(Debug)]
struct CachedResidentPage {
    page: Box<Page>,
    permit: BudgetPermit,
}

impl MemoryUsage for CachedResidentPage {
    fn heap_usage(&self) -> usize {
        self.page.heap_usage()
    }
}

impl PooledObject for CachedResidentPage {
    type ResidentBytesStorage = ();
    fn resident_object_bytes(_: &(), count: usize) -> usize {
        count * std::mem::size_of::<Page>()
    }
    fn add_to_resident_object_bytes(_: &(), _: usize) {}
    fn sub_from_resident_object_bytes(_: &(), _: usize) {}
}

/// The page manager for a database.
///
/// Manages page allocations, eviction and faulting.
/// A page manager receives a [ByteBudget] that controls how many pages it may
/// allocate.
#[derive(Debug)]
pub struct PageManager {
    pool: PagePool,
    resident_cache: Pool<CachedResidentPage>,
    resident: Arc<ResidentObjects>,
    storage: Arc<TieredStorage>,
}

impl PageManager {
    pub(super) fn has_unlimited_budget(&self) -> bool {
        self.resident.has_unlimited_budget()
    }

    pub(super) fn reserve_resident(&self, fixed_row_size: Size) -> Result<(Box<Page>, BudgetPermit), BudgetExceeded> {
        if !self.has_unlimited_budget() {
            let permit = self.resident.acquire_resident_page()?;
            return Ok((self.pool.take_with_fixed_row_size(fixed_row_size), permit));
        }
        let CachedResidentPage { page, permit } = self.resident_cache.take(
            |cached| cached.page.reset_for(max_rows_in_page(fixed_row_size)),
            || CachedResidentPage {
                permit: self
                    .resident
                    .acquire_resident_page()
                    .expect("an unlimited budget cannot reject a resident page"),
                page: self.pool.take_with_fixed_row_size(fixed_row_size),
            },
        );
        Ok((page, permit))
    }

    pub(super) fn recycle_resident(&self, page: Box<Page>, permit: BudgetPermit) {
        if self.has_unlimited_budget() {
            self.resident_cache.put(CachedResidentPage { page, permit });
        } else {
            // Idle pages must not keep consuming a finite memory allowance.
            self.pool.put(page);
            drop(permit);
        }
    }

    pub(super) fn restored_permit(&self) -> BudgetPermit {
        self.resident.restored_page_permit()
    }

    pub fn new(resident: Arc<ResidentObjects>, pool: PagePool, storage: Arc<TieredStorage>) -> Self {
        // Bound this cache by the configured page-pool capacity.
        let resident_cache =
            Pool::new(usize::try_from(pool.capacity()).expect("existing pool capacity fits the address space"));
        Self {
            pool,
            resident_cache,
            resident,
            storage,
        }
    }

    pub fn new_for_test() -> Self {
        Self::new(
            ResidentObjects::new(ByteBudget::unlimited()).into(),
            PagePool::new_for_test(),
            TieredStorage::new_for_test().into(),
        )
    }

    /// Get the [PageHandle] at the given `slot`, or fault it in.
    ///
    /// Returns `Ok(None)` if the page is absent.
    pub fn get(
        &self,
        slot: &PageSlotHandle,
        eviction_policy: PageEvictionPolicy,
    ) -> Result<Option<PageHandle>, PageError> {
        self.may_fault(&mut slot.write_ref(), eviction_policy)
    }

    /// Get a [PageFrameWriteGuard] for the page at the given `slot`.
    ///
    /// The page is faulted in if necessary.
    ///
    /// # Panics
    ///
    /// The caller must ensure that the page is present (i.e. the slot is not
    /// [PageSlot::Absent]). The method panics if the page is absent.
    pub fn get_mut(
        &self,
        slot: &PageSlotHandle,
        eviction_policy: PageEvictionPolicy,
        fixed_row_size: Size,
    ) -> Result<PageFrameWriteGuard, PageError> {
        let mut slot_guard = slot.write_owned();
        let handle = self
            .may_fault(&mut slot_guard, eviction_policy)?
            .expect("page requested for mutation to be present");
        // SAFETY: We just faulted the page, so the handle contains the resident
        // frame we want to lock.
        Ok(unsafe { handle.write(slot_guard, fixed_row_size) })
    }

    fn may_fault(
        &self,
        slot_guard: &mut PageSlot,
        eviction_policy: PageEvictionPolicy,
    ) -> Result<Option<PageHandle>, PageError> {
        match &*slot_guard {
            PageSlot::Absent => Ok(None),
            PageSlot::Resident {
                handle,
                eviction_policy: cached_eviction_policy,
                ..
            } => {
                assert_eq!(
                    cached_eviction_policy, &eviction_policy,
                    "eviction policy cannot be changed for resident pages"
                );
                // Avoid atomics contention if the page isn't evictable.
                if matches!(eviction_policy, PageEvictionPolicy::Evictable) {
                    self.resident.touch_page(&handle.frame);
                }
                Ok(Some(handle.clone()))
            }
            PageSlot::NonResident { hash, metadata, .. } => {
                let reservation = self.resident.reserve_page(|| {
                    let bytes = self.storage.read(ObjectKey::from(*hash))?;
                    let page = self
                        .pool
                        .take_deserialize_from(&bytes)
                        .map(|page| PooledPage::new(page, self.pool.clone()))?;

                    Ok::<_, PageError>(page)
                })?;
                let frame = self.resident.redeem_page(reservation);
                let handle = PageHandle { frame };
                *slot_guard = PageSlot::Resident {
                    handle: handle.clone(),
                    metadata: *metadata,
                    backing: Some(*hash),
                    eviction_policy,
                };

                Ok(Some(handle))
            }
        }
    }

    /// Allocate a new page with the proper layout for `fixed_row_size`.
    ///
    /// Subject to the [ByteBudget] of this page manager.
    pub fn allocate(
        &self,
        fixed_row_size: Size,
        eviction_policy: PageEvictionPolicy,
    ) -> Result<PageSlotHandle, PageError> {
        let reservation = self.reserve(fixed_row_size)?;
        Ok(self.redeem(reservation, eviction_policy))
    }

    /// Reserve a a page with the proper layout for `fixed_row_size`.
    ///
    /// Subject to the [ByteBudget] of this page manager.
    /// The returned [ReservedPage] is not registered with the manager. To do
    /// so, call [Self::redeem()].
    pub fn reserve(&self, fixed_row_size: Size) -> Result<ReservedPage, BudgetExceeded> {
        let mut metadata = None;
        let reservation = self.resident.reserve_page(|| {
            let page = self.pool.take_with_fixed_row_size(fixed_row_size);
            metadata = Some(page.metadata(fixed_row_size));
            let page = PooledPage::new(page, self.pool.clone());

            Ok::<_, BudgetExceeded>(page)
        })?;

        Ok(ReservedPage {
            inner: reservation,
            metadata: metadata.unwrap(),
        })
    }

    /// Register a [ReservedPage] with the page manager.
    ///
    /// The page must have been created by [Self::reserve()] on the same page
    /// manager instance.
    pub fn redeem(
        &self,
        ReservedPage { inner, metadata }: ReservedPage,
        eviction_policy: PageEvictionPolicy,
    ) -> PageSlotHandle {
        let slot = Arc::new(RwLock::new(PageSlot::Absent));
        {
            let mut slot_guard = slot.write();
            let frame = self.resident.redeem_page(inner);
            *slot_guard = PageSlot::Resident {
                handle: PageHandle { frame },
                metadata,
                backing: None,
                eviction_policy,
            };
        }

        PageSlotHandle { slot }
    }

    /// Register a number of pages with this manager.
    ///
    /// This is used when restoring from a snapshot, and **NOT** subject to the
    /// [ByteBudget] of this page manager. I.e., after this method returns, more
    /// bytes than the high watermark of the budget may be resident.
    pub(super) fn register_unchecked(
        &self,
        eviction_policy: PageEvictionPolicy,
        fixed_row_size: Size,
        pages: impl IntoIterator<Item = Option<Box<Page>>>,
        slots: &mut Vec<PageSlotHandle>,
    ) {
        for page in pages {
            let slot = Arc::new(RwLock::new(PageSlot::Absent));

            if let Some(page) = page {
                let metadata = page.metadata(fixed_row_size);
                let frame = self.resident.register_page_unchecked(self.pool.clone(), page);
                let mut slot_guard = slot.write();
                *slot_guard = PageSlot::Resident {
                    handle: PageHandle { frame },
                    metadata,
                    // NOTE: The page is in snapshot storage, but not
                    // necessarily in the local tier.
                    backing: None,
                    eviction_policy,
                };
            }

            slots.push(PageSlotHandle { slot });
        }
    }

    pub(super) fn evictable(&self, candidates: &mut Vec<PageEvictionCandidate>) {
        self.resident.evictable_pages(candidates);
    }

    pub(super) fn evicted(&self, victim: PageEvictionCandidate) {
        self.resident.evict_page(victim);
    }

    pub(super) fn ensure_lower_tier_stored(&self, page: &PageHandle) -> Result<PageHash, ObjectStoreError> {
        self.storage
            .write(page.into())
            .map(|key| key.as_page_hash().cloned().expect("object key to be a page hash"))
    }
}
