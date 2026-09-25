use core::fmt;
use std::{
    io,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, Weak,
    },
};

use parking_lot::{ArcRwLockReadGuard, RawRwLock, RwLock, RwLockWriteGuard};
use slab::Slab;
use spacetimedb_lib::bsatn::DecodeError;
use spacetimedb_sats::layout::Size;

use crate::{
    indexes::{PageIndex, PAGE_SIZE},
    page::{self, Page, PageMetadata},
    page_pool::PagePool,
    tiered::{BudgetExceeded, BudgetPermit, ByteBudget, ByteBudgetConfig},
};

#[cfg(test)]
use crate::var_len::VarLenMembers;

pub type PageFrameReadGuard = ArcRwLockReadGuard<RawRwLock, Box<Page>>;

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

#[derive(Clone, Copy, Debug)]
pub enum PageEvictionPolicy {
    Evictable,
    NeverEvict,
}

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

    pub fn bytes_used_by_rows(&self, fixed_row_size: Size) -> usize {
        self.slot.lock().unwrap().bytes_used_by_rows(fixed_row_size)
    }

    pub fn metadata(&self, fixed_row_size: Size) -> Option<PageMetadata> {
        self.slot.lock().unwrap().metadata(fixed_row_size)
    }

    pub fn page(&self) -> Option<PageHandle> {
        self.slot.lock().unwrap().page().cloned()
    }

    pub(super) fn free(&self) {
        let mut slot = self.slot.lock().unwrap();
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
        let slot = self.slot.lock().unwrap();
        unsafe { slot.reconstruct_bytes_used_by_rows(fixed_row_size, var_len_visitor) }
    }

    pub fn reconstruct_num_rows(&self) -> usize {
        self.slot.lock().unwrap().reconstruct_num_rows()
    }
}

#[derive(Debug)]
#[allow(unused)]
pub enum PageSlot {
    Absent,
    Resident {
        handle: PageHandle,
        #[allow(unused)]
        state: ResidentPageState,
    },
    #[allow(unused)]
    NonResident {
        hash: blake3::Hash,
        metadata: PageMetadata,
    },
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
            PageSlot::Resident { handle, .. } => {
                Some(handle.read().has_space_for_row(fixed_row_size, num_var_len_granules))
            }
            PageSlot::NonResident { metadata, .. } => {
                Some(metadata.has_space_for_row(fixed_row_size, num_var_len_granules))
            }
        }
    }

    pub fn is_full(&self, fixed_row_size: Size) -> Option<bool> {
        match self {
            PageSlot::Absent => None,
            PageSlot::Resident { handle, .. } => Some(handle.read().is_full(fixed_row_size)),
            PageSlot::NonResident { metadata, .. } => Some(metadata.is_full(fixed_row_size)),
        }
    }

    pub fn available_var_len_granules(&self) -> Option<usize> {
        match self {
            PageSlot::Absent => None,
            PageSlot::Resident { handle, .. } => Some(handle.read().available_var_len_granules()),
            PageSlot::NonResident { metadata, .. } => Some(metadata.available_var_len_granules()),
        }
    }

    pub fn bytes_used_by_rows(&self, fixed_row_size: Size) -> usize {
        match self {
            PageSlot::Absent => 0,
            PageSlot::Resident { handle, .. } => handle.read().bytes_used_by_rows(fixed_row_size),
            PageSlot::NonResident { metadata, .. } => metadata.bytes_used_by_rows as _,
        }
    }

    pub fn metadata(&self, fixed_row_size: Size) -> Option<PageMetadata> {
        match self {
            PageSlot::Absent => None,
            PageSlot::Resident { handle, .. } => Some(handle.read().metadata(fixed_row_size)),
            PageSlot::NonResident { metadata, .. } => Some(*metadata),
        }
    }

    pub fn page(&self) -> Option<&PageHandle> {
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

                let fixed_row_bytes = metadata.num_rows as usize + fixed_row_size.len();
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

#[derive(Clone, Debug)]
pub struct PageHandle {
    frame: Arc<PageFrame>,
}

impl PageHandle {
    pub fn read(&self) -> PageFrameReadGuard {
        self.frame.read()
    }

    pub fn with_page_mut<T>(&mut self, f: impl FnOnce(&mut Page) -> T) -> T {
        let mut guard = self.frame.write();
        f(&mut guard)
    }
}

#[derive(Debug)]
pub struct PageFrame {
    page: Arc<RwLock<Box<Page>>>,
    #[allow(unused)]
    permit: BudgetPermit,
    access: Arc<FrameAccess>,
}

impl PageFrame {
    fn new(permit: BudgetPermit, page: Box<Page>, epoch: u64) -> Self {
        Self {
            page: RwLock::new(page).into(),
            permit,
            access: FrameAccess::new(epoch).into(),
        }
    }

    pub fn read(&self) -> PageFrameReadGuard {
        RwLock::read_arc(&self.page)
    }

    fn write(&self) -> RwLockWriteGuard<'_, Box<Page>> {
        self.page.write()
    }

    fn touch(&self, epoch: u64) {
        self.access.touch(epoch);
    }
}

#[derive(Debug)]
#[allow(unused)]
pub enum ResidentPageState {
    Clean { hash: Option<blake3::Hash> },
    Dirty { hash: Option<blake3::Hash> },
}

pub struct ReservedPage {
    permit: BudgetPermit,
    page: Box<Page>,
}

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

    pub fn get(
        &self,
        slot: &PageSlotHandle,
        eviction_policy: PageEvictionPolicy,
    ) -> Result<Option<PageHandle>, PageError> {
        Ok(self.may_fault(slot, eviction_policy)?.map(|frame| PageHandle { frame }))
    }

    pub fn with_page_mut<T>(
        &self,
        slot: &PageSlotHandle,
        eviction_policy: PageEvictionPolicy,
        f: impl FnOnce(&mut Page) -> T,
    ) -> Result<T, PageError> {
        let frame = self
            .may_fault(slot, eviction_policy)?
            .expect("page requested for mutation to be present");
        let res = {
            let mut page = frame.write();
            f(&mut page)
        };
        let mut slot = slot.slot.lock().unwrap();
        *slot = PageSlot::Resident {
            handle: PageHandle { frame },
            state: ResidentPageState::Dirty { hash: None },
        };
        Ok(res)
    }

    fn may_fault(
        &self,
        slot: &PageSlotHandle,
        eviction_policy: PageEvictionPolicy,
    ) -> Result<Option<Arc<PageFrame>>, PageError> {
        let mut slot_guard = slot.slot.lock().unwrap();
        match *slot_guard {
            PageSlot::Absent => Ok(None),
            PageSlot::Resident { ref handle, .. } => {
                let frame = handle.frame.clone();
                frame.touch(self.access_epoch.fetch_add(1, Ordering::Relaxed));
                Ok(Some(frame))
            }
            PageSlot::NonResident { hash, .. } => {
                let permit = self.acquire_memory_budget_permit()?;
                let page = self.store.load_page(hash)?;
                let frame = self.frames.write().register(
                    permit,
                    page,
                    self.access_epoch.fetch_add(1, Ordering::Relaxed),
                    |frame| {
                        let registry_entry = FrameRegistryEntry::new(&frame, &slot.slot, eviction_policy);
                        let resident = PageSlot::Resident {
                            handle: PageHandle { frame },
                            state: ResidentPageState::Clean { hash: Some(hash) },
                        };
                        *slot_guard = resident;
                        registry_entry
                    },
                );

                Ok(Some(frame))
            }
        }
    }

    pub fn allocate(
        &self,
        fixed_row_size: Size,
        eviction_policy: PageEvictionPolicy,
    ) -> Result<PageSlotHandle, PageError> {
        let reservation = self.reserve(fixed_row_size)?;
        Ok(self.redeem(reservation, eviction_policy))
    }

    pub fn reserve(&self, fixed_row_size: Size) -> Result<ReservedPage, BudgetExceeded> {
        let permit = self.acquire_memory_budget_permit()?;
        let page = self.pool.take_with_fixed_row_size(fixed_row_size);

        Ok(ReservedPage { permit, page })
    }

    pub fn redeem(
        &self,
        ReservedPage { permit, page }: ReservedPage,
        eviction_policy: PageEvictionPolicy,
    ) -> PageSlotHandle {
        let slot = Arc::new(Mutex::new(PageSlot::Absent));
        {
            let mut slot_guard = slot.lock().unwrap();
            self.frames.write().register(
                permit,
                page,
                self.access_epoch.fetch_add(1, Ordering::Relaxed),
                |frame| {
                    let registry_entry = FrameRegistryEntry::new(&frame, &slot, eviction_policy);
                    *slot_guard = PageSlot::Resident {
                        handle: PageHandle { frame },
                        state: ResidentPageState::Clean { hash: None },
                    };
                    registry_entry
                },
            );
        }

        PageSlotHandle { slot }
    }

    pub(super) fn register(
        &self,
        eviction_policy: PageEvictionPolicy,
        pages: impl IntoIterator<Item = Option<Box<Page>>>,
    ) -> Vec<PageSlotHandle> {
        let mut handles = Vec::new();
        for page in pages {
            let slot = Arc::new(Mutex::new(PageSlot::Absent));
            if let Some(page) = page {
                let permit = self.force_acquire_memory_budget_limit();
                let mut slot_gard = slot.lock().unwrap();
                self.frames.write().register(
                    permit,
                    page,
                    self.access_epoch.fetch_add(1, Ordering::Relaxed),
                    |frame| {
                        let registry_entry = FrameRegistryEntry::new(&frame, &slot, eviction_policy);
                        *slot_gard = PageSlot::Resident {
                            handle: PageHandle { frame },
                            state: ResidentPageState::Clean { hash: None },
                        };
                        registry_entry
                    },
                );
            }

            handles.push(PageSlotHandle { slot });
        }

        handles
    }

    fn force_acquire_memory_budget_limit(&self) -> BudgetPermit {
        self.memory.force_acquire(PAGE_SIZE as _)
    }

    fn acquire_memory_budget_permit(&self) -> Result<BudgetPermit, BudgetExceeded> {
        // TODO: Try to evict pages if acquisition fails.
        self.memory.acquire(PAGE_SIZE as _)
    }
}

#[derive(Debug, Default)]
struct FrameRegistry {
    frames: Slab<FrameRegistryEntry>,
}

impl FrameRegistry {
    pub fn register(
        &mut self,
        permit: BudgetPermit,
        page: Box<Page>,
        epoch: u64,
        mk_entry: impl FnOnce(Arc<PageFrame>) -> FrameRegistryEntry,
    ) -> Arc<PageFrame> {
        let entry = self.frames.vacant_entry();
        let frame = Arc::new(PageFrame::new(permit, page, epoch));
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
