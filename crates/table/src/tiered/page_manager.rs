use std::{
    fmt, io,
    mem::{self, ManuallyDrop},
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, Weak,
    },
};

use parking_lot::{ArcRwLockReadGuard, ArcRwLockWriteGuard, RawRwLock, RwLock};
use slab::Slab;
use spacetimedb_lib::bsatn::DecodeError;
use spacetimedb_memory_usage::MemoryUsage;
use spacetimedb_sats::layout::Size;

use crate::{
    indexes::{PageIndex, PAGE_SIZE},
    page::{self, Page, PageMetadata},
    page_pool::PagePool,
    tiered::{BudgetExceeded, BudgetPermit, ByteBudget, ByteBudgetConfig},
};

#[cfg(test)]
use crate::var_len::VarLenMembers;

/// RAII read lock guard that dereferences to [Page].
///
/// Created by [PageFrame::read()].
pub struct PageFrameReadGuard {
    guard: ArcRwLockReadGuard<RawRwLock, PageLease>,
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
    guard: ArcRwLockWriteGuard<RawRwLock, PageLease>,
    frame: Arc<PageFrame>,
    slot: Arc<Mutex<PageSlot>>,
    fixed_row_size: Size,
    eviction_policy: PageEvictionPolicy,
}

impl PageFrameWriteGuard {
    pub fn into_page_handle(self) -> PageHandle {
        PageHandle {
            frame: self.frame.clone(),
        }
    }
}

impl Deref for PageFrameWriteGuard {
    type Target = Page;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl DerefMut for PageFrameWriteGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl Drop for PageFrameWriteGuard {
    fn drop(&mut self) {
        let metadata = self.guard.metadata(self.fixed_row_size);
        let mut slot_guard = self.slot.lock().unwrap();
        match &mut *slot_guard {
            PageSlot::Resident {
                handle,
                state,
                metadata: cached_metadata,
                eviction_policy: cached_eviction_policy,
            } if Arc::ptr_eq(&handle.frame, &self.frame) => {
                *cached_metadata = metadata;
                *cached_eviction_policy = self.eviction_policy;
                state.mark_dirty(None);
            }
            _ => {
                *slot_guard = PageSlot::Resident {
                    handle: PageHandle {
                        frame: self.frame.clone(),
                    },
                    state: ResidentPageState::Dirty { hash: None },
                    metadata,
                    eviction_policy: self.eviction_policy,
                };
            }
        }
    }
}

/// Placeholder trait for disk / object storage.
pub trait PageBackingStore: fmt::Debug + Send + Sync + 'static {
    /// Load a [Page] by its content hash from backing storage .
    fn load_page(&self, hash: blake3::Hash) -> Result<Box<Page>, PageIoError>;
}

impl PageBackingStore for () {
    fn load_page(&self, _: blake3::Hash) -> Result<Box<Page>, PageIoError> {
        unimplemented!("no page backing store configured")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PageError {
    #[error("maximum number of pages exceeded")]
    TooManyPages,
    #[error(transparent)]
    MemoryLimitExceeded(#[from] BudgetExceeded),
    #[error(transparent)]
    Page(page::Error),
    #[error("page at index {0:?} is missing")]
    MissingPage(PageIndex),
    #[error("object {0} is missing")]
    MissingObject(blake3::Hash),
    #[error(transparent)]
    Io(#[from] PageIoError),
    #[error("error decoding page from bsatn")]
    Deserialize(DecodeError),
}

#[derive(Debug, thiserror::Error)]
pub enum PageIoError {
    #[error("expected page hash {expected} doesn't match computed page hash {computed}")]
    HashMismatch {
        expected: blake3::Hash,
        computed: blake3::Hash,
    },
    #[error(transparent)]
    Io(#[from] io::Error),
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
    slot: Arc<Mutex<PageSlot>>,
}

impl PageSlotHandle {
    pub fn is_resident(&self) -> bool {
        self.slot.lock().unwrap().is_resident()
    }

    pub fn is_absent(&self) -> bool {
        self.slot.lock().unwrap().is_absent()
    }

    pub fn has_space_for_row(&self, fixed_row_size: Size, num_var_len_granules: usize) -> Option<bool> {
        self.slot
            .lock()
            .unwrap()
            .has_space_for_row(fixed_row_size, num_var_len_granules)
    }

    pub fn is_full(&self, fixed_row_size: Size) -> Option<bool> {
        self.slot.lock().unwrap().is_full(fixed_row_size)
    }

    pub fn available_var_len_granules(&self) -> Option<usize> {
        self.slot.lock().unwrap().available_var_len_granules()
    }

    pub fn bytes_used_by_rows(&self) -> usize {
        self.slot.lock().unwrap().bytes_used_by_rows()
    }

    pub fn metadata(&self) -> Option<PageMetadata> {
        self.slot.lock().unwrap().metadata()
    }

    pub fn page(&self) -> Option<PageHandle> {
        self.slot.lock().unwrap().page().cloned()
    }

    pub(super) fn free(&self) {
        let mut slot = self.slot.lock().unwrap();
        *slot = PageSlot::Absent;
    }
}

impl MemoryUsage for PageSlotHandle {
    fn heap_usage(&self) -> usize {
        let maybe_resident = {
            let slot = self.slot.lock().unwrap();
            slot.page().cloned()
        };
        let refcounts = mem::size_of::<usize>() * 2;
        refcounts + mem::size_of::<Mutex<PageSlot>>() + maybe_resident.as_ref().map_or(0, MemoryUsage::heap_usage)
    }
}

#[cfg(test)]
impl PageSlotHandle {
    pub unsafe fn reconstruct_bytes_used_by_rows(
        &self,
        fixed_row_size: Size,
        var_len_visitor: &impl VarLenMembers,
    ) -> usize {
        let slot = self.slot.lock().unwrap();
        unsafe { slot.reconstruct_bytes_used_by_rows(fixed_row_size, var_len_visitor) }
    }

    pub fn reconstruct_num_rows(&self) -> usize {
        self.slot.lock().unwrap().reconstruct_num_rows()
    }
}

/// State of a page.
#[derive(Debug)]
#[allow(unused)]
pub enum PageSlot {
    /// The page has been freed, so the slot is empty.
    /// This exists to maintain stable [PageIndex]ex.
    Absent,
    /// The page is resident in memory.
    Resident {
        handle: PageHandle,
        metadata: PageMetadata,
        #[allow(unused)]
        state: ResidentPageState,
        eviction_policy: PageEvictionPolicy,
    },
    /// The page is not resident in memory.
    #[allow(unused)]
    NonResident { hash: blake3::Hash, metadata: PageMetadata },
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
            PageSlot::Absent => None,
            PageSlot::Resident { metadata, .. } | PageSlot::NonResident { metadata, .. } => {
                Some(metadata.has_space_for_row(fixed_row_size, num_var_len_granules))
            }
        }
    }

    pub fn is_full(&self, fixed_row_size: Size) -> Option<bool> {
        match self {
            PageSlot::Absent => None,
            PageSlot::Resident { metadata, .. } | PageSlot::NonResident { metadata, .. } => {
                Some(metadata.is_full(fixed_row_size))
            }
        }
    }

    pub fn available_var_len_granules(&self) -> Option<usize> {
        match self {
            PageSlot::Absent => None,
            PageSlot::Resident { metadata, .. } | PageSlot::NonResident { metadata, .. } => {
                Some(metadata.available_var_len_granules())
            }
        }
    }

    pub fn bytes_used_by_rows(&self) -> usize {
        match self {
            PageSlot::Absent => 0,
            PageSlot::Resident { metadata, .. } | PageSlot::NonResident { metadata, .. } => {
                metadata.bytes_used_by_rows as _
            }
        }
    }

    pub fn metadata(&self) -> Option<PageMetadata> {
        match self {
            PageSlot::Absent => None,
            PageSlot::Resident { metadata, .. } | PageSlot::NonResident { metadata, .. } => Some(*metadata),
        }
    }

    fn page(&self) -> Option<&PageHandle> {
        match self {
            PageSlot::Absent | PageSlot::NonResident { .. } => None,
            PageSlot::Resident { handle, .. } => Some(handle),
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
    /// Lock the page for reading.
    pub fn read(&self) -> PageFrameReadGuard {
        self.frame.read()
    }
}

impl MemoryUsage for PageHandle {
    fn heap_usage(&self) -> usize {
        let page_heap_usage = self.read().heap_usage();
        // `Arc<PageFrame>` + `Arc<RwLock<PageLease>>` + `Arc<FrameAccess>`
        let refcounts = 3 * (mem::size_of::<usize>() * 2);

        refcounts
            + mem::size_of::<PageFrame>()
            + mem::size_of::<RwLock<PageLease>>()
            + mem::size_of::<FrameAccess>()
            + mem::size_of::<Page>()
            + page_heap_usage
    }
}

/// A page frame is basically an allocated page.
///
/// Eviction is controlled via weak references to the internals of this type.
#[derive(Debug)]
pub struct PageFrame {
    lease: Arc<RwLock<PageLease>>,
    access: Arc<FrameAccess>,
}

impl PageFrame {
    fn new(pool: PagePool, permit: BudgetPermit, page: Box<Page>, epoch: u64) -> Self {
        Self {
            lease: RwLock::new(PageLease::new(pool, permit, page)).into(),
            access: FrameAccess::new(epoch).into(),
        }
    }

    pub fn read(&self) -> PageFrameReadGuard {
        PageFrameReadGuard {
            guard: RwLock::read_arc(&self.lease),
        }
    }

    fn write(
        self: Arc<Self>,
        slot: Arc<Mutex<PageSlot>>,
        fixed_row_size: Size,
        eviction_policy: PageEvictionPolicy,
    ) -> PageFrameWriteGuard {
        PageFrameWriteGuard {
            guard: RwLock::write_arc(&self.lease),
            frame: self,
            slot,
            fixed_row_size,
            eviction_policy,
        }
    }

    fn touch(&self, epoch: u64) {
        self.access.touch(epoch);
    }
}

struct PageLease {
    page: ManuallyDrop<Box<Page>>,
    pool: PagePool,
    #[allow(unused)]
    permit: BudgetPermit,
}

impl PageLease {
    pub fn new(pool: PagePool, permit: BudgetPermit, page: Box<Page>) -> Self {
        Self {
            page: ManuallyDrop::new(page),
            pool,
            permit,
        }
    }
}

impl Drop for PageLease {
    fn drop(&mut self) {
        // SAFETY: we're in `drop`, so won't be using `ManuallyDrop` again.
        let page = unsafe { ManuallyDrop::take(&mut self.page) };
        self.pool.put(page);
    }
}

impl Deref for PageLease {
    type Target = Page;

    fn deref(&self) -> &Self::Target {
        &self.page
    }
}

impl DerefMut for PageLease {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.page
    }
}

impl fmt::Debug for PageLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageLease")
            .field("page", &self.page)
            .field("pool", &"<page pool>")
            .field("permit", &self.permit)
            .finish()
    }
}

#[derive(Debug)]
#[allow(unused)]
pub enum ResidentPageState {
    Clean { hash: Option<blake3::Hash> },
    Dirty { hash: Option<blake3::Hash> },
}

impl ResidentPageState {
    fn mark_dirty(&mut self, maybe_hash: Option<blake3::Hash>) {
        match self {
            ResidentPageState::Clean { .. } => *self = ResidentPageState::Dirty { hash: maybe_hash },
            ResidentPageState::Dirty { hash } => {
                maybe_hash.map(|new| hash.replace(new));
            }
        }
    }
}

/// A page allocation, along with its [BudgetPermit].
///
/// Created during commit planning ([crate::table::Table::prepare_commit()]),
/// respectively [PageManager::reserve()]. A registered page can be registered
/// with the page manager infallibly (see [PageManager::redeem()]).
pub struct ReservedPage {
    permit: BudgetPermit,
    page: Box<Page>,
    metadata: PageMetadata,
}

/// The page manager for a database.
///
/// Manages page allocations, eviction and faulting.
/// A page manager receives a [ByteBudget] that controls how many pages it may
/// allocate.
#[derive(Debug)]
pub struct PageManager {
    frames: RwLock<FrameRegistry>,
    pool: PagePool,
    store: Arc<dyn PageBackingStore>,
    memory: ByteBudget,
    access_epoch: AtomicU64,
}

impl PageManager {
    pub fn new(pool: PagePool, store: Arc<dyn PageBackingStore>, memory: ByteBudget) -> Self {
        Self {
            frames: <_>::default(),
            pool,
            store,
            memory,
            access_epoch: <_>::default(),
        }
    }

    pub fn new_for_test() -> Self {
        Self::new(
            PagePool::new_for_test(),
            Arc::new(()),
            ByteBudget::new(ByteBudgetConfig::unlimited()).unwrap(),
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
        Ok(self.may_fault(slot, eviction_policy)?.map(|frame| PageHandle { frame }))
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
        let frame = self
            .may_fault(slot, eviction_policy)?
            .expect("page requested for mutation to be present");
        Ok(frame.write(slot.slot.clone(), fixed_row_size, eviction_policy))
    }

    fn may_fault(
        &self,
        slot: &PageSlotHandle,
        eviction_policy: PageEvictionPolicy,
    ) -> Result<Option<Arc<PageFrame>>, PageError> {
        let mut slot_guard = slot.slot.lock().unwrap();
        match *slot_guard {
            PageSlot::Absent => Ok(None),
            PageSlot::Resident {
                ref handle,
                eviction_policy: cached_eviction_policy,
                ..
            } => {
                assert_eq!(
                    cached_eviction_policy, eviction_policy,
                    "eviction policy cannot be changed for resident pages"
                );
                let frame = handle.frame.clone();
                // Avoid atomics contention if the page isn't evictable.
                if matches!(eviction_policy, PageEvictionPolicy::Evictable) {
                    frame.touch(self.access_epoch.fetch_add(1, Ordering::Relaxed));
                }
                Ok(Some(frame))
            }
            PageSlot::NonResident { hash, metadata, .. } => {
                let permit = self.acquire_memory_budget_permit()?;
                let page = self.store.load_page(hash)?;
                let frame = self.frames.write().register(
                    self.pool.clone(),
                    permit,
                    page,
                    self.access_epoch.fetch_add(1, Ordering::Relaxed),
                    |frame| {
                        let registry_entry = FrameRegistryEntry::new(&frame, &slot.slot, eviction_policy);
                        *slot_guard = PageSlot::Resident {
                            handle: PageHandle { frame },
                            metadata,
                            state: ResidentPageState::Clean { hash: Some(hash) },
                            eviction_policy,
                        };
                        registry_entry
                    },
                );

                Ok(Some(frame))
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
        let permit = self.acquire_memory_budget_permit()?;
        let page = self.pool.take_with_fixed_row_size(fixed_row_size);
        let metadata = page.metadata(fixed_row_size);

        Ok(ReservedPage { permit, page, metadata })
    }

    /// Register a [ReservedPage] with the page manager.
    ///
    /// The page must have been created by [Self::reserve()] on the same page
    /// manager instance.
    pub fn redeem(
        &self,
        ReservedPage { permit, page, metadata }: ReservedPage,
        eviction_policy: PageEvictionPolicy,
    ) -> PageSlotHandle {
        let slot = Arc::new(Mutex::new(PageSlot::Absent));
        {
            let mut slot_guard = slot.lock().unwrap();
            self.frames.write().register(
                self.pool.clone(),
                permit,
                page,
                self.access_epoch.fetch_add(1, Ordering::Relaxed),
                |frame| {
                    let registry_entry = FrameRegistryEntry::new(&frame, &slot, eviction_policy);
                    *slot_guard = PageSlot::Resident {
                        handle: PageHandle { frame },
                        state: ResidentPageState::Clean { hash: None },
                        metadata,
                        eviction_policy,
                    };
                    registry_entry
                },
            );
        }

        PageSlotHandle { slot }
    }

    /// Register a number of pages with this manager.
    ///
    /// This is used when restoring from a snapshot, and **NOT** subject to the
    /// [ByteBudget] of this page manager. I.e., after this method returns, more
    /// bytes than the high watermark of the budget may be resident.
    pub(super) fn register(
        &self,
        eviction_policy: PageEvictionPolicy,
        fixed_row_size: Size,
        pages: impl IntoIterator<Item = Option<Box<Page>>>,
    ) -> Vec<PageSlotHandle> {
        let mut handles = Vec::new();
        for page in pages {
            let slot = Arc::new(Mutex::new(PageSlot::Absent));

            if let Some(page) = page {
                let metadata = page.metadata(fixed_row_size);
                // SAFETY: We are restoring from a snapshot, so are allowed to
                // go over budget.
                let permit = unsafe { self.unchecked_acquire_memory_buget_permit() };
                let mut slot_gard = slot.lock().unwrap();
                self.frames.write().register(
                    self.pool.clone(),
                    permit,
                    page,
                    self.access_epoch.fetch_add(1, Ordering::Relaxed),
                    |frame| {
                        let registry_entry = FrameRegistryEntry::new(&frame, &slot, eviction_policy);
                        *slot_gard = PageSlot::Resident {
                            handle: PageHandle { frame },
                            state: ResidentPageState::Clean { hash: None },
                            metadata,
                            eviction_policy,
                        };
                        registry_entry
                    },
                );
            }

            handles.push(PageSlotHandle { slot });
        }

        handles
    }

    unsafe fn unchecked_acquire_memory_buget_permit(&self) -> BudgetPermit {
        self.memory.force_acquire(PAGE_SIZE as _)
    }

    fn acquire_memory_budget_permit(&self) -> Result<BudgetPermit, BudgetExceeded> {
        // TODO: Try to evict pages if acquisition fails.
        self.memory.acquire(PAGE_SIZE as _)
    }
}

/// Essentially the LRU cache for pages.
///
/// This is currently a stub and doesn't ever evict pages.
#[derive(Debug, Default)]
struct FrameRegistry {
    frames: Slab<FrameRegistryEntry>,
}

impl FrameRegistry {
    pub fn register(
        &mut self,
        pool: PagePool,
        permit: BudgetPermit,
        page: Box<Page>,
        epoch: u64,
        mk_entry: impl FnOnce(Arc<PageFrame>) -> FrameRegistryEntry,
    ) -> Arc<PageFrame> {
        let entry = self.frames.vacant_entry();
        let frame = Arc::new(PageFrame::new(pool, permit, page, epoch));
        entry.insert(mk_entry(frame.clone()));
        frame
    }
}

/// Access counters for cache eviction.
///
/// Shared between [FrameRegistryEntry] and [PageFrame], to avoid lock
/// contention on the [FrameRegistry] for updates.
#[derive(Debug)]
struct FrameAccess {
    last_access_epoch: AtomicU64,
    last_access_count: AtomicU64,
}

impl FrameAccess {
    fn new(epoch: u64) -> Self {
        Self {
            last_access_epoch: AtomicU64::new(epoch),
            last_access_count: <_>::default(),
        }
    }

    fn touch(&self, epoch: u64) {
        self.last_access_epoch.store(epoch, Ordering::Relaxed);
        self.last_access_count.fetch_add(1, Ordering::Relaxed);
    }
}

impl MemoryUsage for FrameAccess {
    fn heap_usage(&self) -> usize {
        mem::size_of::<Self>()
    }
}

#[derive(Debug)]
#[allow(unused)]
pub struct FrameRegistryEntry {
    frame: Weak<PageFrame>,
    slot: Weak<Mutex<PageSlot>>,
    eviction_policy: PageEvictionPolicy,
    access: Arc<FrameAccess>,
}

impl FrameRegistryEntry {
    pub fn new(frame: &Arc<PageFrame>, slot: &Arc<Mutex<PageSlot>>, eviction_policy: PageEvictionPolicy) -> Self {
        Self {
            access: Arc::clone(&frame.access),
            frame: Arc::downgrade(frame),
            slot: Arc::downgrade(slot),
            eviction_policy,
        }
    }
}
