use super::metrics::{CacheTier, ObjectType};
use std::{
    collections::{btree_map, BTreeMap, BTreeSet},
    convert::Infallible,
    sync::Arc,
};

use spacetimedb_lib::ProductValue;
use spacetimedb_memory_usage::MemoryUsage;
use spacetimedb_sats::layout::Size;

use crate::{
    indexes::{PageIndex, RowPointer},
    page::{Page, PageCapacity},
    pages::Pages,
    table::{BlobNumBytes, PreparedInsert, Table, TableError},
    tiered::{
        blob_set::BlobHandle,
        page_manager::{PageFrameWriteGuard, PageHandle, PageManager, PageSlot, PageSlotHandle, ReservedPage},
        storage::DiskObject,
        BlobSet, BudgetPermit, EvictionError, EvictionPolicy, ObjectStoreError, PageError, PageRead, PageRef,
    },
    var_len::VarLenMembers,
};

/// The set of pages of a [Table].
#[derive(Debug)]
pub struct PageSet {
    pub eviction_policy: EvictionPolicy,
    pub(crate) memory_limit_exempt: bool,
    fixed_row_size: Size,
    pub(crate) manager: Arc<PageManager>,
    storage: PageStorage,
}

/// The original directly owned page representation, with allocation accounting.
/// A table borrow protects page access; no per-page shared ownership or locks are needed.
#[derive(Debug)]
struct ResidentPages {
    pages: Pages,
    permits: smallvec::SmallVec<[Option<BudgetPermit>; 1]>,
}

#[derive(Debug)]
enum PageStorage {
    Resident(ResidentPages),
    Tiered(TieredPages),
}

/// Evictable pages retain shared slots and frames so readers can pin their contents.
#[derive(Debug, Default)]
struct TieredPages {
    slots: Vec<PageSlotHandle>,
    free_page_slots: BTreeSet<PageIndex>,
    non_full_pages: BTreeSet<(usize, PageIndex)>,
}

impl PageStorage {
    #[inline]
    fn resident(&self) -> Option<&ResidentPages> {
        match self {
            Self::Resident(pages) => Some(pages),
            Self::Tiered(_) => None,
        }
    }

    #[inline]
    fn resident_mut(&mut self) -> Option<&mut ResidentPages> {
        match self {
            Self::Resident(pages) => Some(pages),
            Self::Tiered(_) => None,
        }
    }

    #[inline]
    fn tiered(&self) -> &TieredPages {
        match self {
            Self::Tiered(pages) => pages,
            Self::Resident(_) => unreachable!("resident branch must be handled first"),
        }
    }

    #[inline]
    fn tiered_mut(&mut self) -> &mut TieredPages {
        match self {
            Self::Tiered(pages) => pages,
            Self::Resident(_) => unreachable!("resident branch must be handled first"),
        }
    }
}

impl Drop for PageSet {
    fn drop(&mut self) {
        if let Some(resident) = self.storage.resident_mut() {
            let pages = std::mem::take(&mut resident.pages).into_page_slots();
            let permits = std::mem::take(&mut resident.permits);
            for (page, permit) in pages.zip(permits) {
                if let Some(page) = page {
                    self.manager
                        .recycle_resident(page, permit.expect("present page has a permit"));
                }
            }
        }
    }
}

impl ResidentPages {
    fn into_tiered(
        mut self,
        manager: &Arc<PageManager>,
        fixed_row_size: Size,
        eviction_policy: EvictionPolicy,
    ) -> TieredPages {
        assert_eq!(self.pages.len(), self.permits.len());
        let mut tiered = TieredPages {
            free_page_slots: std::mem::take(&mut self.pages.free_page_slots),
            non_full_pages: std::mem::take(&mut self.pages.non_full_pages),
            ..TieredPages::default()
        };
        // Move each allocation together with its existing memory charge. Keep
        // empty slots too: compacting the slots would invalidate row pointers.
        for (page, permit) in self.pages.into_page_slots().zip(self.permits) {
            assert_eq!(page.is_some(), permit.is_some());
            tiered
                .slots
                .push(manager.adopt_resident(page.zip(permit), fixed_row_size, eviction_policy));
        }
        tiered
    }

    #[inline(always)]
    fn with_page_to_insert_row<T>(
        &mut self,
        manager: &PageManager,
        fixed_row_size: Size,
        memory_limit_exempt: bool,
        num_var_len_granules: usize,
        reservation: Option<(PageIndex, ReservedPage)>,
        f: impl FnOnce(&mut Page) -> T,
    ) -> Result<(PageIndex, T), PageError> {
        let index = match reservation {
            Some((index, reservation)) => {
                self.install(index, reservation);
                index
            }
            None => match self.pages.take_non_full_page(fixed_row_size, num_var_len_granules) {
                Some(index) => index,
                None => {
                    let index = self.pages.next_page_index().map_err(|_| PageError::TooManyPages)?;
                    let reservation = manager.reserve_resident(fixed_row_size, memory_limit_exempt)?;
                    let (page, permit) = reservation;
                    self.install_owned(index, page, permit);
                    index
                }
            },
        };
        let result = f(self.pages.get_mut(index).unwrap());
        self.pages.record_page_non_full(index, fixed_row_size);
        Ok((index, result))
    }

    fn install(&mut self, index: PageIndex, reservation: ReservedPage) {
        let (page, permit) = reservation.into_resident();
        self.install_owned(index, page, permit);
    }

    #[inline]
    fn install_owned(&mut self, index: PageIndex, page: Box<Page>, permit: BudgetPermit) {
        self.pages.install_page(index, page);
        if index.idx() == self.permits.len() {
            self.permits.push(Some(permit));
        } else {
            self.permits[index.idx()] = Some(permit);
        }
    }

    #[inline]
    fn recycle_empty_page(&mut self, manager: &PageManager, index: PageIndex) {
        let page = self.pages.free_empty_page(index);
        let permit = self.permits[index.idx()].take().expect("present page has a permit");
        manager.recycle_resident(page, permit);
    }
}

impl TieredPages {
    fn into_resident(self, manager: &PageManager, fixed_row_size: Size) -> ResidentPages {
        let slot_count = self.slots.len();
        let mut pages = Vec::with_capacity(slot_count);
        let mut permits = smallvec::SmallVec::with_capacity(slot_count);
        for slot in self.slots {
            let (page, permit) = manager.detach_resident(slot).unzip();
            pages.push(page);
            permits.push(permit);
        }
        let mut resident = Pages::default();
        resident.set_contents(pages, fixed_row_size);
        ResidentPages {
            pages: resident,
            permits,
        }
    }

    fn apply_eviction_policy(&self, policy: EvictionPolicy) {
        assert!(matches!(policy, EvictionPolicy::Memory | EvictionPolicy::Disk));

        let disk_pins = if policy == EvictionPolicy::Memory { 1 } else { 0 };
        for slot_handle in &self.slots {
            let mut slot = slot_handle.lock_ref();
            match &mut *slot {
                PageSlot::Absent => {}
                PageSlot::Resident {
                    backing,
                    eviction_policy,
                    ..
                } => {
                    *eviction_policy = policy;
                    if let Some(backing) = backing {
                        backing.set_pins(disk_pins);
                    }
                }
                PageSlot::NonResident { backing, .. } => {
                    backing.set_pins(disk_pins);
                }
            }
        }
    }
}

pub enum PageInfo<'a> {
    Resident(&'a Page),
    Tiered(&'a PageSlotHandle),
}

impl PageInfo<'_> {
    #[cfg(test)]
    fn is_full(&self, size: Size) -> Option<bool> {
        match self {
            Self::Resident(p) => Some(p.is_full(size)),
            Self::Tiered(p) => p.is_full(size),
        }
    }
    fn available_var_len_granules(&self) -> Option<usize> {
        match self {
            Self::Resident(p) => Some(p.available_var_len_granules()),
            Self::Tiered(p) => p.available_var_len_granules(),
        }
    }
    fn has_space_for_row(&self, size: Size, granules: usize) -> Option<bool> {
        match self {
            Self::Resident(p) => Some(p.has_space_for_row(size, granules)),
            Self::Tiered(p) => p.has_space_for_row(size, granules),
        }
    }
    pub fn bytes_used_by_rows(&self, size: Size) -> usize {
        match self {
            Self::Resident(p) => p.bytes_used_by_rows(size),
            Self::Tiered(p) => p.bytes_used_by_rows(),
        }
    }
    #[cfg(test)]
    pub fn reconstruct_num_rows(&self) -> usize {
        match self {
            Self::Resident(p) => p.reconstruct_num_rows(),
            Self::Tiered(p) => p.reconstruct_num_rows(),
        }
    }
    #[cfg(test)]
    pub unsafe fn reconstruct_bytes_used_by_rows(&self, size: Size, visitor: &impl VarLenMembers) -> usize {
        unsafe {
            match self {
                Self::Resident(p) => p.reconstruct_bytes_used_by_rows(size, visitor),
                Self::Tiered(p) => p.reconstruct_bytes_used_by_rows(size, visitor),
            }
        }
    }
}

/// Created by [PageSet::ensure_disk], ownership of this handle guarantees that
/// a page is either pinned to memory, or present and pinned to the local
/// storage tier.
#[allow(unused)]
pub(crate) struct DiskPinnedPageHandle {
    page: Option<PageHandle>,
    backing: Option<DiskObject>,
}

impl PageSet {
    /// Load and pin all existing disk-table pages for a policy conversion.
    pub(crate) fn pin_all_pages(&self) -> Result<Vec<PageHandle>, PageError> {
        assert!(!self.is_resident());
        let mut pins = Vec::with_capacity(self.num_present_pages());
        for index in 0..self.num_pages() {
            if let Some(PageRef::Tiered(pin)) = self.get_page(PageIndex(index as u64))? {
                pins.push(pin);
            }
        }
        assert_eq!(pins.len(), self.num_present_pages());
        Ok(pins)
    }

    /// Load and pin all existing disk-table pages to the local storage tier.
    pub(crate) fn ensure_disk(&self) -> Result<Vec<DiskPinnedPageHandle>, ObjectStoreError> {
        assert!(!self.is_resident());
        let mut pins = Vec::with_capacity(self.num_present_pages());
        let tiered = self.storage.tiered();
        for slot_handle in &tiered.slots {
            let mut slot = slot_handle.lock_ref();
            let handle = match &mut *slot {
                PageSlot::Absent => None,
                PageSlot::Resident { handle, backing, .. } => {
                    let backing = backing
                        .clone()
                        .map(|mut backing| -> Result<DiskObject, ObjectStoreError> {
                            self.manager.storage.ensure_local(&mut backing)?;
                            Ok(backing)
                        })
                        .transpose()?;
                    Some(DiskPinnedPageHandle {
                        page: backing.is_none().then(|| handle.clone()),
                        backing,
                    })
                }
                PageSlot::NonResident { backing, .. } => {
                    let mut backing = backing.clone();
                    self.manager.storage.ensure_local(&mut backing)?;
                    Some(DiskPinnedPageHandle {
                        page: None,
                        backing: Some(backing),
                    })
                }
            };
            if let Some(handle) = handle {
                pins.push(handle);
            }
        }
        assert_eq!(pins.len(), self.num_present_pages());
        Ok(pins)
    }

    /// Change who owns the existing page allocations, without copying their contents.
    /// The caller must hold the database write lock to exclude concurrent eviction.
    /// All page handles must be released first. Disk-table pages must already be loaded.
    pub(crate) fn apply_eviction_policy(&mut self, policy: EvictionPolicy) {
        if policy == self.eviction_policy {
            return;
        }
        let old = std::mem::replace(&mut self.storage, PageStorage::Tiered(TieredPages::default()));
        self.storage = match old {
            PageStorage::Resident(resident) => {
                assert!(matches!(policy, EvictionPolicy::Memory | EvictionPolicy::Disk));
                PageStorage::Tiered(resident.into_tiered(&self.manager, self.fixed_row_size, policy))
            }
            PageStorage::Tiered(tiered) if policy == EvictionPolicy::Never => {
                PageStorage::Resident(tiered.into_resident(&self.manager, self.fixed_row_size))
            }
            PageStorage::Tiered(tiered) => {
                tiered.apply_eviction_policy(policy);
                PageStorage::Tiered(tiered)
            }
        };
        self.eviction_policy = policy;
    }

    #[inline]
    pub(crate) fn is_resident(&self) -> bool {
        matches!(self.storage, PageStorage::Resident(_))
    }

    /// Bypass general page access when only a resident row's presence bit is needed.
    #[inline]
    pub(crate) fn resident_row_present(&self, pointer: RowPointer, fixed_row_size: Size) -> Option<bool> {
        self.storage.resident().map(|resident| {
            resident
                .pages
                .get(pointer.page_index())
                .is_some_and(|page| page.has_row_offset(fixed_row_size, pointer.page_offset()))
        })
    }

    /// Delete a fixed-layout row directly, returning false for the tiered backend.
    ///
    /// # Safety
    /// The pointer must identify a live row, and this table's layout must contain
    /// no variable-length members or external blob references.
    #[inline]
    pub(crate) unsafe fn delete_fixed_resident_row(&mut self, pointer: RowPointer, fixed_row_size: Size) -> bool {
        let Some(resident) = self.storage.resident_mut() else {
            return false;
        };
        let index = pointer.page_index();
        let empty = resident.pages.with_page_mut(index, fixed_row_size, |page| {
            // SAFETY: Forward the caller's guarantees about the row and its layout.
            unsafe { page.delete_fixed_row(pointer.page_offset(), fixed_row_size) };
            page.num_rows() == 0
        });
        if empty {
            resident.recycle_empty_page(&self.manager, index);
        }
        true
    }

    /// Create a fresh, empty page set.
    pub fn new(manager: Arc<PageManager>, eviction_policy: EvictionPolicy, fixed_row_size: Size) -> Self {
        let storage = match eviction_policy {
            EvictionPolicy::Never => PageStorage::Resident(ResidentPages {
                pages: Pages::default(),
                permits: smallvec::SmallVec::new(),
            }),
            EvictionPolicy::Memory | EvictionPolicy::Disk => PageStorage::Tiered(TieredPages::default()),
        };
        Self {
            eviction_policy,
            memory_limit_exempt: false,
            fixed_row_size,
            manager,
            storage,
        }
    }

    /// Populate this [PageSet] with the `pages` acquired externally.
    ///
    /// This method is provided for compatibility. It is used when restoring
    /// from a snapshot, or a schema migration on an empty table.
    pub fn set_contents(&mut self, pages: impl IntoIterator<Item = Option<Box<Page>>>, fixed_row_size: Size) {
        if let Some(resident) = self.storage.resident_mut() {
            let pages: Vec<_> = pages.into_iter().collect();
            resident.permits = pages
                .iter()
                .map(|page| page.as_ref().map(|_| self.manager.restored_permit()))
                .collect();
            resident.pages.set_contents(pages, fixed_row_size);
            self.fixed_row_size = fixed_row_size;
            return;
        }
        assert!(self.storage.tiered().slots.is_empty());

        // This is ok, because the table is empty.
        self.fixed_row_size = fixed_row_size;
        self.manager.register_unchecked(
            self.eviction_policy,
            self.fixed_row_size,
            pages,
            &mut self.storage.tiered_mut().slots,
        );

        self.storage.tiered_mut().non_full_pages = self
            .storage
            .tiered()
            .slots
            .iter()
            .enumerate()
            .filter_map(|(idx, page)| {
                if !page.is_full(fixed_row_size)? {
                    page.available_var_len_granules()
                        .map(|granules| (granules, PageIndex(idx as _)))
                } else {
                    None
                }
            })
            .collect();
        self.storage.tiered_mut().free_page_slots = self
            .storage
            .tiered()
            .slots
            .iter()
            .enumerate()
            .filter_map(|(idx, page)| page.is_absent().then_some(PageIndex(idx as _)))
            .collect();
    }

    /// Get a read-only handle to the page at `index`.
    ///
    /// Returns `Ok(None)` if the index is out of bounds, or the page at that
    /// index was freed.
    ///
    /// If the page is not currently resident, it may be faulted in. If this
    /// fails, a [PageError] is returned.
    #[inline]
    pub fn get_page(&self, index: PageIndex) -> Result<Option<PageRef<'_>>, PageError> {
        if let Some(resident) = self.storage.resident() {
            let page = resident.pages.get(index);
            if page.is_some() {
                self.manager.metrics().lookup(ObjectType::Page, CacheTier::Memory, true);
            }
            return Ok(page.map(PageRef::Resident));
        }
        let tiered = self.storage.tiered();
        let Some(slot) = tiered.slots.get(index.idx()) else {
            return Ok(None);
        };
        self.manager
            .with_page_retry(|fault| self.manager.get_with_fault(slot, self.eviction_policy, fault))
            .map(|page| page.map(PageRef::Tiered))
    }

    #[inline]
    pub(crate) fn resident_page(&self, index: PageIndex) -> &Page {
        self.storage
            .resident()
            .expect("row without an eviction pin is resident")
            .pages
            .get(index)
            .expect("row reference points at a present page")
    }

    /// Iterate over all pages in this page set, ensuring that the page's hash
    /// is computed before yielding it.
    ///
    /// This will fault in each page if it is not resident.
    ///
    /// Used when capturing a snapshot.
    pub(crate) fn iter_pages_with_hashes(
        &mut self,
    ) -> impl Iterator<Item = Result<Option<(blake3::Hash, PageRef<'_>)>, PageError>> {
        match &mut self.storage {
            PageStorage::Resident(resident) => itertools::Either::Left(resident.pages.iter_mut().map(|page| {
                Ok(page.as_deref_mut().map(|page| {
                    let hash = page.save_or_get_content_hash();
                    (hash, PageRef::Resident(page))
                }))
            })),
            PageStorage::Tiered(tiered) => itertools::Either::Right(tiered.slots.iter().map(|slot| {
                if slot.is_absent() {
                    Ok(None)
                } else {
                    let mut page = self.manager.with_page_retry(|fault| {
                        self.manager
                            .get_mut_with_fault(slot, self.eviction_policy, self.fixed_row_size, fault)
                    })?;
                    let hash = page.save_or_get_content_hash();
                    Ok(Some((hash, PageRef::Tiered(page.into_page_handle()))))
                }
            })),
        }
    }

    /// Run a closure over a mutable [Page].
    ///
    /// The page is faulted in if it is not already resident.
    ///
    /// Note that the fullness status of the page is updated after `f` returns,
    /// but the page is not freed if it is empty at that point. Use [Self::delete_row]
    /// to free the page immediately if it is empty.
    #[inline]
    pub fn with_page_mut<T>(
        &mut self,
        index: PageIndex,
        fixed_row_size: Size,
        f: impl FnOnce(&mut Page) -> T,
    ) -> Result<T, PageError> {
        if let Some(resident) = self.storage.resident_mut() {
            if resident.pages.get(index).is_none() {
                return Err(PageError::MissingPage(index));
            }
            return Ok(resident.pages.with_page_mut(index, fixed_row_size, f));
        }

        self.with_tiered_page_mut(index, fixed_row_size, f)
            .map(|(result, _)| result)
    }

    // Retain the write guard so inserts can transfer its pin before unlocking.
    fn with_tiered_page_mut<T>(
        &mut self,
        index: PageIndex,
        fixed_row_size: Size,
        f: impl FnOnce(&mut Page) -> T,
    ) -> Result<(T, PageFrameWriteGuard), PageError> {
        let mut page = {
            let tiered = self.storage.tiered();
            let Some(slot) = tiered.slots.get(index.idx()) else {
                return Err(PageError::MissingPage(index));
            };
            self.manager.with_page_retry(|fault| {
                self.manager
                    .get_mut_with_fault(slot, self.eviction_policy, self.fixed_row_size, fault)
            })?
        };

        self.storage
            .tiered_mut()
            .non_full_pages
            .remove(&(page.available_var_len_granules(), index));
        let ret = f(&mut page);
        if !page.is_full(fixed_row_size) {
            self.storage
                .tiered_mut()
                .non_full_pages
                .insert((page.available_var_len_granules(), index));
        }

        Ok((ret, page))
    }

    /// Run a closure over a mutable [Page].
    ///
    /// This is meant to be only used to insert a single row.
    ///
    /// If `reservation` is `None`, `fixed_row_size` and `num_var_len_granules`
    /// are used to find a suitable non-full page, or allocate a new one. If a
    /// non-full page exists, it is faulted in not already resident.
    ///
    /// If `reservation` is `Some`, the supplied page is registered with this
    /// [PageSet], and the closure is run against exactly this page.
    ///
    /// The fullness status of the page is updated, but the page is not freed if
    /// it ends up being empty after `f` returns. Keep the returned pin until the
    /// inserted row has acquired its own page handle.
    #[inline(always)]
    pub fn with_page_to_insert_row<T>(
        &mut self,
        fixed_row_size: Size,
        num_var_len_granules: usize,
        reservation: Option<(PageIndex, ReservedPage)>,
        f: impl FnOnce(&mut Page) -> T,
    ) -> Result<(PageIndex, T, Option<PageHandle>), PageError> {
        if let Some(resident) = self.storage.resident_mut() {
            let (index, result) = resident.with_page_to_insert_row(
                &self.manager,
                fixed_row_size,
                self.memory_limit_exempt,
                num_var_len_granules,
                reservation,
                f,
            )?;

            return Ok((index, result, None));
        }
        self.with_tiered_page_to_insert_row(fixed_row_size, num_var_len_granules, reservation, f)
    }

    fn with_tiered_page_to_insert_row<T>(
        &mut self,
        fixed_row_size: Size,
        num_var_len_granules: usize,
        reservation: Option<(PageIndex, ReservedPage)>,
        f: impl FnOnce(&mut Page) -> T,
    ) -> Result<(PageIndex, T, Option<PageHandle>), PageError> {
        // Keep a newly published reservation pinned through the mutation. Blob
        // allocations inside `f` can trigger the shared eviction coordinator.
        let (index, _allocation_pin) = match reservation {
            None => self.find_page_with_space_for_row(fixed_row_size, num_var_len_granules)?,
            Some((index, page)) => (index, Some(self.register(index, page))),
        };
        let (result, page) = self.with_tiered_page_mut(index, fixed_row_size, f)?;
        // Transfer the write guard's pin before releasing either write lock.

        Ok((index, result, Some(page.into_page_handle())))
    }

    /// Free the row that is pointed to by `row_ptr`,
    /// marking its fixed-len storage
    /// and var-len storage granules as available for re-use.
    ///
    /// If the page ends up empty after deleting the row, it is freed
    /// immediately.
    ///
    /// # Safety
    ///
    /// The `row_ptr` must point to a valid row in this page manager,
    /// of `fixed_row_size` bytes for the fixed part.
    ///
    /// The `fixed_row_size` must be consistent
    /// with what has been passed to the manager in all other operations
    /// and must be consistent with the `var_len_visitor` the manager was made with.
    pub unsafe fn delete_row(
        &mut self,
        var_len_visitor: &impl VarLenMembers,
        fixed_row_size: Size,
        row_ptr: RowPointer,
        blobs: &BlobSet,
    ) -> Result<BlobNumBytes, PageError> {
        let page_index = row_ptr.page_index();
        let eviction_policy = self.eviction_policy;
        let (blob_bytes_deleted, is_empty, available_granules) =
            self.with_page_mut(page_index, fixed_row_size, |page| {
                // SAFETY:
                // - `row_ptr.page_offset()` does point to a valid row in this page
                //   as the caller promised that `row_ptr` points to a valid row in `self`.
                //
                // - `fixed_row_size` is consistent with the size in bytes of the fixed part of the row.
                //   The size is also conistent with `var_len_visitor`.
                let blob_bytes_deleted = unsafe {
                    page.delete_row(
                        row_ptr.page_offset(),
                        fixed_row_size,
                        var_len_visitor,
                        blobs,
                        eviction_policy,
                    )
                };

                (
                    blob_bytes_deleted,
                    page.num_rows() == 0,
                    page.available_var_len_granules(),
                )
            })?;

        if is_empty {
            if let Some(resident) = self.storage.resident_mut() {
                resident.recycle_empty_page(&self.manager, page_index);
            } else {
                self.storage.tiered().slots[page_index.idx()].free();
                self.storage.tiered_mut().free_page_slots.insert(page_index);
                self.storage
                    .tiered_mut()
                    .non_full_pages
                    .remove(&(available_granules, page_index));
            }
        }

        Ok(blob_bytes_deleted)
    }

    /// Find a page with sufficient available space to store a row of size `fixed_row_size`
    /// containing `num_var_len_granules` granules of var-len data.
    ///
    /// If no such page exists, a fresh one is allocated.
    fn find_page_with_space_for_row(
        &mut self,
        fixed_row_size: Size,
        num_var_len_granules: usize,
    ) -> Result<(PageIndex, Option<PageHandle>), PageError> {
        if let Some((_, page_idx)) = self
            .storage
            .tiered()
            .non_full_pages
            .range((num_var_len_granules, PageIndex(0))..)
            .copied()
            .find(|(_, page_index)| {
                self.storage.tiered().slots[page_index.idx()]
                    .has_space_for_row(fixed_row_size, num_var_len_granules)
                    .expect("page in `self.storage.tiered().non_full_pages` to be present in `self.pages`")
            })
        {
            return Ok((page_idx, None));
        }

        self.allocate_new_page_pinned(fixed_row_size)
            .map(|(index, pin)| (index, Some(pin)))
    }

    /// Allocates one additional page,
    /// returning an error if the new number of pages would overflow `PageIndex::MAX`.
    ///
    /// Allocation may also fail if the page manager has insufficient
    /// [crate::tiered::budget::ByteBudget] to reserve another page.
    ///
    /// The new page is initially empty, but is not added to the non-full set.
    /// Callers should call [`Pages::record_page_non_full`] after operating on the new page.
    fn allocate_new_page_pinned(&mut self, fixed_row_size: Size) -> Result<(PageIndex, PageHandle), PageError> {
        let free_page = self.storage.tiered_mut().free_page_slots.pop_first();
        let page_index = free_page.map(Ok).unwrap_or_else(|| self.can_allocate_new_page())?;
        let allocation = self.manager.with_eviction_retry(|| {
            let reservation = self.manager.reserve(fixed_row_size)?;

            Ok(self.manager.redeem_pinned(reservation, self.eviction_policy))
        });
        let (slot, pin) = allocation.inspect_err(|_| {
            if free_page.is_some() {
                self.storage.tiered_mut().free_page_slots.insert(page_index);
            }
        })?;

        // SAFETY: The page is resident and the index is from free slots or
        // fresh.
        let pin = unsafe { self.install_page_slot(page_index, slot, pin) };

        Ok((page_index, pin))
    }

    #[cfg(test)]
    fn allocate_new_page(&mut self, fixed_row_size: Size) -> Result<PageIndex, PageError> {
        self.allocate_new_page_pinned(fixed_row_size).map(|(index, _)| index)
    }

    /// Register a [ReservedPage] with this [PageSet].
    ///
    /// Pages are reserved during [Self::prepare_commit].
    fn register(&mut self, index: PageIndex, reservation: ReservedPage) -> PageHandle {
        let (slot, pin) = self.manager.redeem_pinned(reservation, self.eviction_policy);
        // SAFETY: The page is resident. The index was obtained during commit
        // planning.
        unsafe { self.install_page_slot(index, slot, pin) }
    }

    /// SAFETY:
    /// - The [PageSlotHandle] must contain the resident page held by `page`.
    /// - If there already exists an entry at `index`, the existing slot must be
    ///   absent.
    unsafe fn install_page_slot(&mut self, index: PageIndex, slot: PageSlotHandle, page: PageHandle) -> PageHandle {
        assert_eq!(slot.page_id(), Some(page.page_id()));
        let storage = self.storage.tiered_mut();
        let idx = index.idx();
        if idx == storage.slots.len() {
            storage.slots.push(slot);
        } else {
            assert!(storage.slots[idx].is_absent());
            storage.free_page_slots.remove(&index);
            storage.slots[idx] = slot;
        }

        self.storage
            .tiered_mut()
            .non_full_pages
            .insert((page.read().available_var_len_granules(), index));

        page
    }

    /// Is there space to allocate another page?
    pub fn can_allocate_new_page(&self) -> Result<PageIndex, PageError> {
        let new_idx = self.num_pages();
        if new_idx <= PageIndex::MAX.idx() {
            Ok(PageIndex(new_idx as _))
        } else {
            Err(PageError::TooManyPages)
        }
    }

    /// Iterate over all pages that have not been freed.
    ///
    /// Yields borrowed page metadata without faulting in tiered pages.
    pub fn iter_present_pages(&self) -> impl Iterator<Item = PageInfo<'_>> {
        self.iter_present_pages_with_page_index().map(|(_, page)| page)
    }

    /// Iterate over all pages that have not been freed, along with their
    /// [PageIndex].
    ///
    /// Because absent pages are skipped, the [PageIndex] sequence may contain
    /// gaps.
    ///
    /// Yields borrowed page metadata without faulting in tiered pages.
    pub fn iter_present_pages_with_page_index(&self) -> impl Iterator<Item = (PageIndex, PageInfo<'_>)> {
        if let Some(resident) = self.storage.resident() {
            return itertools::Either::Left(
                resident
                    .pages
                    .iter_present_pages_with_page_index()
                    .map(|(index, page)| (index, PageInfo::Resident(page))),
            );
        }
        itertools::Either::Right(
            self.storage
                .tiered()
                .slots
                .iter()
                .enumerate()
                .filter_map(|(idx, slot)| (!slot.is_absent()).then_some((PageIndex(idx as _), PageInfo::Tiered(slot)))),
        )
    }

    /// Free all pages, retaining their indices as absent, reusable slots.
    ///
    /// Does not read pages or release blob references: the caller must release
    /// references belonging to the discarded rows before calling this method.
    pub(crate) fn clear(&mut self) {
        if let Some(resident) = self.storage.resident_mut() {
            resident.pages.clear(|index, page| {
                let permit = resident.permits[index.idx()].take().expect("present page has a permit");
                self.manager.recycle_resident(page, permit);
            });
        } else {
            let tiered = self.storage.tiered_mut();
            tiered.non_full_pages.clear();
            for (index, slot) in tiered.slots.iter().enumerate() {
                slot.free();
                tiered.free_page_slots.insert(PageIndex(index as _));
            }
            assert_eq!(tiered.free_page_slots.len(), tiered.slots.len());
        }
        assert_eq!(self.num_present_pages(), 0);
    }

    /// The number of present pages in `self`.
    ///
    /// Includes resident as well as non-resident pages, but not freed ones.
    pub fn num_present_pages(&self) -> usize {
        if let Some(resident) = self.storage.resident() {
            return resident.pages.num_present_pages();
        }
        self.storage
            .tiered()
            .slots
            .len()
            .checked_sub(self.storage.tiered().free_page_slots.len())
            .expect("pages len to be greater than number of free slots")
    }

    /// The total number of pages in `self`.
    ///
    /// Includes resident, non-resident and absent (i.e. freed) pages.
    pub fn num_pages(&self) -> usize {
        if let Some(resident) = self.storage.resident() {
            return resident.pages.len();
        }
        self.storage.tiered().slots.len()
    }

    // Resident pages cannot fault, and an unlimited memory budget cannot reject a page.
    // For fixed-layout callers, one new page per insert bounds the worst case so
    // page-index exhaustion cannot interrupt a commit after mutations have started.
    pub(crate) fn can_commit_resident(&self, insert_count: u64) -> bool {
        self.storage.resident().is_some()
            && self.manager.has_unlimited_budget()
            && u64::try_from(self.num_pages())
                .ok()
                .and_then(|pages| pages.checked_add(insert_count))
                .is_some_and(|pages| pages <= PageIndex::MAX.0 + 1)
    }

    fn non_full_pages(&self) -> &BTreeSet<(usize, PageIndex)> {
        match &self.storage {
            PageStorage::Resident(pages) => &pages.pages.non_full_pages,
            PageStorage::Tiered(pages) => &pages.non_full_pages,
        }
    }

    fn free_page_slots(&self) -> &BTreeSet<PageIndex> {
        match &self.storage {
            PageStorage::Resident(pages) => &pages.pages.free_page_slots,
            PageStorage::Tiered(pages) => &pages.free_page_slots,
        }
    }

    fn page_info(&self, index: PageIndex) -> Option<PageInfo<'_>> {
        if let Some(resident) = self.storage.resident() {
            return resident.pages.get(index).map(PageInfo::Resident);
        }
        self.storage
            .tiered()
            .slots
            .get(index.idx())
            .filter(|slot| !slot.is_absent())
            .map(PageInfo::Tiered)
    }

    /// Plan a transaction commit.
    ///
    /// Planning proceeds by computing the placement of each row in both
    /// `deletes` and `inserts`, faulting in the affected pages and collecting
    /// them in a pinned set, such that they can't be evicted.
    ///
    /// If new pages need to be allocated in order to accomodate for the
    /// `inserts`, they are obtained from the [PageManager] as [ReservedPage]s
    /// (if there is sufficient budget).
    ///
    /// May fail with fault or budget errors.
    ///
    /// To infallibly apply the mutations, call [PreparedCommit::apply].
    pub fn prepare_commit(
        &self,
        fixed_row_size: Size,
        visitor: &impl VarLenMembers,
        deletes: impl IntoIterator<Item = RowPointer>,
        inserts: impl IntoIterator<Item = Result<PreparedInsert, TableError>>,
    ) -> Result<PreparedCommit, TableError> {
        let mut allocator = PageAllocator::new(self, fixed_row_size);
        let mut pinned = BTreeMap::new();

        let deletes = deletes
            .into_iter()
            .map(|row_ptr| {
                let page_index = row_ptr.page_index();
                let page = match pinned.entry(page_index) {
                    btree_map::Entry::Vacant(entry) => {
                        match self.get_page(page_index)?.expect("delete from absent page") {
                            PageRef::Resident(page) => PageRead::Resident(page),
                            PageRef::Tiered(pin) => PageRead::Tiered(entry.insert(pin).read()),
                        }
                    }
                    btree_map::Entry::Occupied(entry) => PageRead::Tiered(entry.get().read()),
                };
                let granules = unsafe { page.row_total_granules(row_ptr.page_offset(), fixed_row_size, visitor) };
                allocator
                    .prepare_delete::<Infallible>(page_index, granules, |_| Ok(page.metadata(fixed_row_size).into()));

                Ok(row_ptr)
            })
            .collect::<Result<_, PageError>>()?;

        let inserts = inserts
            .into_iter()
            .map(|res| {
                let row = res?;
                // Comparison pages share the mutation pin set, which deduplicates
                // them and keeps them resident until apply has finished.
                for &page_index in &row.extra_pages_to_pin {
                    if let btree_map::Entry::Vacant(entry) = pinned.entry(page_index)
                        && let PageRef::Tiered(pin) = self
                            .get_page(page_index)?
                            .expect("pointer-map candidate belongs to a live page")
                    {
                        entry.insert(pin);
                    }
                }
                let page_index =
                    allocator.prepare_insert::<PageError>(row.required_var_len_granules as usize, |page_index| {
                        match pinned.entry(page_index) {
                            btree_map::Entry::Vacant(entry) => {
                                let page = self.get_page(page_index)?.expect("delete from absent page");
                                match page {
                                    PageRef::Resident(page) => Ok(page.metadata(fixed_row_size).into()),
                                    PageRef::Tiered(pin) => {
                                        Ok(entry.insert(pin).read().metadata(fixed_row_size).into())
                                    }
                                }
                            }
                            btree_map::Entry::Occupied(entry) => Ok(entry.get().read().metadata(fixed_row_size).into()),
                        }
                    })?;
                Ok(PlannedInsert { page_index, row })
            })
            .collect::<Result<_, TableError>>()?;

        let reserved = allocator
            .pages_to_allocate()
            .map(|page_index| {
                self.reserve_page(fixed_row_size)
                    .map(|reservation| (page_index, reservation))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        Ok(PreparedCommit {
            pinned,
            reserved,
            deletes,
            inserts,
            pinned_blobs: Vec::new(),
        })
    }

    /// Evict pages and blobs sharing this manager until the budget's low watermark
    /// is met, or no eligible objects remain. Returns the total number of bytes freed.
    pub fn trim(&self) -> Result<u64, EvictionError> {
        self.manager.trim_all()
    }

    /// Evict pages and blobs sharing this manager until `target_usage` is met,
    /// or no eligible objects remain. Returns the total number of bytes freed.
    pub fn evict(&self, target_usage: u64) -> Result<u64, EvictionError> {
        self.manager.evict_all(target_usage)
    }

    fn reserve_page(&self, fixed_row_size: Size) -> Result<ReservedPage, PageError> {
        let reserve = || {
            self.manager
                .reserve_with_memory_limit_exemption(fixed_row_size, self.memory_limit_exempt)
                .map_err(PageError::from)
        };
        match &self.storage {
            PageStorage::Resident(_) => reserve(),
            PageStorage::Tiered(_) => self.manager.with_eviction_retry(reserve),
        }
    }
}

#[cfg(test)]
impl PageSet {
    pub(crate) fn assert_non_full_pages_consistent(&self, fixed_row_size: Size) {
        let mut page_granules = BTreeMap::new();
        for &(avail, page_index) in self.non_full_pages() {
            assert!(
                page_granules.insert(page_index, avail).is_none(),
                "page {:?} appears multiple times in non_full_pages",
                page_index,
            );
        }
        for index in 0..self.num_pages() {
            let page_index = PageIndex(index as _);
            let entry = page_granules.get(&page_index).copied();
            if let Some(page) = self.page_info(page_index) {
                if page.is_full(fixed_row_size).unwrap() {
                    assert!(entry.is_none(), "full page appears in non_full_pages");
                } else {
                    assert_eq!(entry, page.available_var_len_granules());
                }
            } else {
                assert!(entry.is_none(), "absent page appears in non_full_pages");
            }
        }
    }

    pub(crate) fn iter_present_page_indexes(&self) -> impl Iterator<Item = PageIndex> {
        self.iter_present_pages_with_page_index().map(|(idx, _)| idx)
    }
}

impl MemoryUsage for PageSet {
    fn heap_usage(&self) -> usize {
        // The manager is shared by all page sets of a database.
        match &self.storage {
            PageStorage::Resident(pages) => {
                pages.pages.heap_usage()
                    + if pages.permits.spilled() {
                        pages.permits.capacity() * std::mem::size_of::<Option<BudgetPermit>>()
                    } else {
                        0
                    }
            }
            PageStorage::Tiered(pages) => {
                pages.slots.heap_usage() + pages.free_page_slots.heap_usage() + pages.non_full_pages.heap_usage()
            }
        }
    }
}

#[inline]
fn collect_arc_slice<T, U>(len: u64, iter: impl Iterator<Item = T>, mut f: impl FnMut(T) -> U) -> Arc<[U]> {
    let mut output = Arc::new_uninit_slice(usize::try_from(len).expect("row count fits the address space"));
    let slots = Arc::get_mut(&mut output).unwrap();
    let mut written: u64 = 0;
    for item in iter {
        slots[usize::try_from(written).expect("row index fits the address space")].write(f(item));
        written += 1;
    }
    assert_eq!(written, len, "row count must match iterator length");
    // SAFETY: Every slot was initialized, as verified above.
    unsafe { output.assume_init() }
}

/// A planned transaction commit created by [PageSet::prepare_commit].
///
/// Pins all existing pages affected by the transaction, such that they can't be
/// evicted, as well as any newly alocated pages the transaction requires.
pub struct PreparedCommit {
    /// The pinned or reserved pages to operate on.
    pinned: BTreeMap<PageIndex, PageHandle>,
    /// Reserved page allocations.
    reserved: BTreeMap<PageIndex, ReservedPage>,
    /// The rows to be deleted in this transaction.
    deletes: Vec<RowPointer>,
    /// The rows to be inserted in this transaction.
    inserts: Vec<PlannedInsert>,
    /// Blobs referenced by rows which will be deleted. Holding these handles
    /// makes blob reads during apply infallible.
    pinned_blobs: Vec<BlobHandle>,
}

impl PreparedCommit {
    /// Apply a transaction directly when neither source nor destination can fault
    /// and the destination has no allocation budget or page-index failure ahead.
    pub fn apply_resident(
        table: &mut Table,
        blobs: &BlobSet,
        delete_count: u64,
        deletes: Option<impl Iterator<Item = RowPointer>>,
        source_table: Option<&Table>,
        source_blobs: &BlobSet,
    ) -> AppliedCommit {
        let insert_count = source_table.map_or(0, Table::num_rows);
        assert!(table.can_commit_resident(insert_count));
        assert!(source_table.is_none_or(|source| source.can_commit_resident(0)));
        let deletes = match deletes {
            Some(deletes) => collect_arc_slice(delete_count, deletes, |pointer| {
                table
                    .delete(blobs, pointer, |row| row.to_product_value())
                    .expect("resident page access cannot fail")
                    .expect("deleted row must be present")
            }),
            None => Arc::from([]),
        };
        let is_event = table.get_schema().is_event;
        let inserts = match source_table {
            Some(source) => collect_arc_slice(insert_count, source.scan_rows(source_blobs), |row| {
                let row = row.expect("resident page access cannot fail").to_product_value();
                if !is_event {
                    table.insert(blobs, &row).expect("resident commit was preflighted");
                }
                row
            }),
            None => Arc::from([]),
        };
        AppliedCommit { deletes, inserts }
    }

    /// Apply this transaction to [Table].
    ///
    /// The table's [PageSet] must be the same as the one the [PreparedCommit]
    /// was created from.
    pub fn apply(mut self, table: &mut Table, blobs: &BlobSet) -> AppliedCommit {
        let mut pinned = self.pinned;

        let deletes = collect_arc_slice(self.deletes.len() as u64, self.deletes.into_iter(), |row_ptr| {
            table
                .delete(blobs, row_ptr, |row| row.to_product_value())
                .expect("no page faults")
                .expect("`Table::delete` never returns `None`")
        });
        let inserts = collect_arc_slice(
            self.inserts.len() as u64,
            self.inserts.into_iter(),
            |PlannedInsert { page_index, row }| {
                let schema = table.get_schema();
                // For event tables, we don't insert into the committed state. The
                // row is collected regardless, as we include it in subscriptions
                // and the commitlog.
                if !schema.is_event {
                    let reservation = self.reserved.remove(&page_index).map(|page| (page_index, page));
                    let allocated = reservation.is_some();
                    let row_ref = table
                        .insert_prepared(blobs, &row, reservation)
                        .map(|(_, row_ref)| row_ref)
                        .expect("failed to insert during transaction commit");
                    let (page, _) = row_ref.page_and_offset();
                    let inserted_index = row_ref.pointer().page_index();
                    // A deleted page's index can now name a different frame.
                    // Replace its old pin so subsequent inserts cannot evict the new page.
                    if (allocated || !pinned.contains_key(&inserted_index))
                        && let Some(pin) = page.pin()
                    {
                        pinned.insert(inserted_index, pin);
                    }
                    assert_eq!(inserted_index, page_index, "planned and actual placement differ");

                    row.into_product_value(Some(&row_ref))
                } else {
                    row.into_product_value(None)
                }
            },
        );

        drop(pinned);
        drop(self.pinned_blobs);

        AppliedCommit { deletes, inserts }
    }

    pub(crate) fn pin_blobs(&mut self, blobs: impl IntoIterator<Item = BlobHandle>) {
        self.pinned_blobs.extend(blobs);
    }
}

/// The deleted and inserted rows of a [PreparedCommit::apply] operation, as
/// [ProductValue]s.
pub struct AppliedCommit {
    deletes: Arc<[ProductValue]>,
    inserts: Arc<[ProductValue]>,
}

impl AppliedCommit {
    /// Deconstruct `self` into `(deletes, inserts)`.
    pub fn into_parts(self) -> (Arc<[ProductValue]>, Arc<[ProductValue]>) {
        (self.deletes, self.inserts)
    }
}

/// A [PreparedInsert], along with the [PageIndex] it applies to.
///
/// Constructed during [PageSet::prepare_commit].
pub struct PlannedInsert {
    page_index: PageIndex,
    row: PreparedInsert,
}

enum PlannedPage {
    Absent,
    Existing(PageCapacity),
    Allocate(PageCapacity),
}

/// Simulates placement of deletes and inserts and keeps track of the required
/// pages for a given transaction.
///
/// Pages may either be already existing or need allocation. The latter can be
/// obtained via [PageAllocator::pages_to_allocate] after commit planning is
/// complete.
struct PageAllocator<'a> {
    pages: &'a PageSet,
    fixed_row_size: Size,

    planned: BTreeMap<PageIndex, PlannedPage>,
    non_full: BTreeSet<(usize, PageIndex)>,
    next_new_page: usize,
}

impl<'a> PageAllocator<'a> {
    pub fn new(pages: &'a PageSet, fixed_row_size: Size) -> Self {
        Self {
            pages,
            fixed_row_size,

            planned: <_>::default(),
            non_full: <_>::default(),
            next_new_page: pages.num_pages(),
        }
    }

    pub fn prepare_delete<E>(
        &mut self,
        page_index: PageIndex,
        row_granules: usize,
        fault: impl FnOnce(PageIndex) -> Result<PageCapacity, E>,
    ) -> Result<(), E> {
        self.remove_planned_non_full(page_index);

        let capacity = self.ensure_planned(page_index, fault)?;
        capacity.release_row(row_granules);

        if capacity.num_rows == 0 {
            self.planned.insert(page_index, PlannedPage::Absent);
        } else {
            self.add_planned_non_full(page_index);
        }

        Ok(())
    }

    pub fn prepare_insert<E>(
        &mut self,
        row_granules: usize,
        fault: impl FnOnce(PageIndex) -> Result<PageCapacity, E>,
    ) -> Result<PageIndex, E> {
        if let Some(page_index) = self.find_page(row_granules) {
            self.remove_planned_non_full(page_index);

            let fixed_row_size = self.fixed_row_size;
            self.ensure_planned(page_index, fault)?
                .reserve_row(fixed_row_size, row_granules);

            self.add_planned_non_full(page_index);
            return Ok(page_index);
        }

        let page_index = self.allocate_slot();

        let mut capacity = PageCapacity::empty(self.fixed_row_size);
        capacity.reserve_row(self.fixed_row_size, row_granules);

        self.planned.insert(page_index, PlannedPage::Allocate(capacity));
        self.add_planned_non_full(page_index);

        Ok(page_index)
    }

    fn find_page(&self, row_granules: usize) -> Option<PageIndex> {
        let planned = self
            .non_full
            .range((row_granules, PageIndex(0))..)
            .find_map(|&(_, index)| self.has_space(index, row_granules).then_some(index));

        let committed = self
            .pages
            .non_full_pages()
            .range((row_granules, PageIndex(0))..)
            // A planned page shadows committed state.
            .filter(|(_, index)| !self.planned.contains_key(index))
            .find_map(|&(_, index)| {
                self.pages
                    .page_info(index)
                    .unwrap()
                    .has_space_for_row(self.fixed_row_size, row_granules)
                    .unwrap()
                    .then_some(index)
            });

        match (planned, committed) {
            (Some(a), Some(b)) => {
                let a_free = self.available_granules(a).unwrap();
                let b_free = self.pages.page_info(b).unwrap().available_var_len_granules().unwrap();
                Some(if (a_free, a) <= (b_free, b) { a } else { b })
            }
            (a, b) => a.or(b),
        }
    }

    fn allocate_slot(&mut self) -> PageIndex {
        if let Some(index) = self
            .pages
            .free_page_slots()
            .iter()
            .copied()
            .find(|index| !self.planned.contains_key(index))
        {
            return index;
        }

        let index = PageIndex(self.next_new_page as _);
        self.next_new_page += 1;
        index
    }

    fn ensure_planned<E>(
        &mut self,
        page_index: PageIndex,
        fault: impl FnOnce(PageIndex) -> Result<PageCapacity, E>,
    ) -> Result<&mut PageCapacity, E> {
        use btree_map::Entry;

        match self.planned.entry(page_index) {
            Entry::Vacant(entry) => {
                let capacity = fault(page_index)?;
                Ok(match entry.insert(PlannedPage::Existing(capacity)) {
                    PlannedPage::Existing(capacity) => capacity,
                    _ => unreachable!(),
                })
            }

            Entry::Occupied(entry) => match entry.into_mut() {
                PlannedPage::Existing(capacity) | PlannedPage::Allocate(capacity) => Ok(capacity),

                PlannedPage::Absent => unreachable!("page is absent"),
            },
        }
    }

    fn available_granules(&self, index: PageIndex) -> Option<usize> {
        match self.planned.get(&index)? {
            PlannedPage::Existing(c) | PlannedPage::Allocate(c) => Some(c.available_var_len_granules()),
            PlannedPage::Absent => None,
        }
    }

    fn has_space(&self, index: PageIndex, granules: usize) -> bool {
        match self.planned.get(&index).unwrap() {
            PlannedPage::Existing(c) | PlannedPage::Allocate(c) => c.has_space_for_row(self.fixed_row_size, granules),
            PlannedPage::Absent => false,
        }
    }

    fn remove_planned_non_full(&mut self, index: PageIndex) {
        if let Some(granules) = self.available_granules(index) {
            self.non_full.remove(&(granules, index));
        }
    }

    fn add_planned_non_full(&mut self, index: PageIndex) {
        let Some(granules) = self.available_granules(index) else {
            return;
        };

        if self.has_space(index, 0) {
            self.non_full.insert((granules, index));
        }
    }

    fn pages_to_allocate(&self) -> impl Iterator<Item = PageIndex> + '_ {
        self.planned
            .iter()
            .filter_map(|(&index, page)| matches!(page, PlannedPage::Allocate(_)).then_some(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        indexes::{PageOffset, PAGE_SIZE},
        page_pool::PagePool,
        tiered::{metrics::CacheTier, BlobError, ByteBudget, ByteBudgetConfig, ResidentObjects, TieredStorage},
        var_len::NullVarLenVisitor,
    };

    fn shared_page_and_blob_cache(page_count: u64) -> (Arc<PageManager>, BlobSet) {
        // Bound the shared resident cache while leaving backing storage unlimited.
        let limit = page_count * PAGE_SIZE as u64;
        let budget = ByteBudget::new(ByteBudgetConfig {
            tier: CacheTier::Memory,
            low_water_bytes: 0,
            soft_limit_bytes: limit,
            hard_limit_bytes: limit,
        })
        .unwrap();
        let resident = Arc::new(ResidentObjects::new(budget));
        let storage = Arc::new(TieredStorage::new_for_test());
        let blobs = BlobSet::new(resident.clone(), storage.clone());
        let manager = Arc::new(PageManager::new(resident, PagePool::new_for_test(), storage));

        (manager, blobs)
    }

    fn write_fixed_row(pages: &mut PageSet, blobs: &mut BlobSet, index: PageIndex, value: u64) -> PageOffset {
        pages
            .with_page_mut(index, Size(8), |page| {
                // SAFETY: These pages contain only eight-byte integers, with no
                // variable-length fields for the visitor to inspect.
                unsafe {
                    page.insert_row(
                        &value.to_le_bytes(),
                        &[] as &[&[u8]],
                        &NullVarLenVisitor,
                        blobs,
                        EvictionPolicy::Memory,
                    )
                }
                .unwrap()
            })
            .unwrap()
    }

    fn assert_fixed_row(pages: &PageSet, index: PageIndex, offset: PageOffset, value: u64) {
        let pin = pages.get_page(index).unwrap().unwrap();
        assert_eq!(pin.read().get_row_data(offset, Size(8)), &value.to_le_bytes());
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(64))]

        #[test]
        fn global_page_eviction_matches_lru_two_across_tables(
            accesses in proptest::collection::vec(0u8..4, 0..48),
            restored in proptest::prelude::any::<bool>(),
            disk_policy in proptest::prelude::any::<bool>(),
            empty_slots in 0u8..3,
        ) {
            let (manager, _blobs) = shared_page_and_blob_cache(4);
            let policy = if disk_policy { EvictionPolicy::Disk } else { EvictionPolicy::Memory };
            let mut tables = Vec::new();
            let mut history = [(0u64, 0u64); 4];

            // Allocation and snapshot recovery must both publish owning slots.
            // Holes in restored tables must not affect victim lookup.
            for table_index in 0u8..4 {
                let mut pages = PageSet::new(manager.clone(), policy, Size(8));
                let index = if restored {
                    pages.set_contents(
                        (0..empty_slots).map(|_| None).chain([Some(Page::new(Size(8)))]),
                        Size(8),
                    );
                    PageIndex(u64::from(empty_slots))
                } else {
                    pages.allocate_new_page(Size(8)).unwrap()
                };
                history[usize::from(table_index)].1 = u64::from(table_index) + 1;
                tables.push((pages, index));
            }

            // Model access history independently of the resident registry.
            let mut epoch = 5u64;
            for table_index in accesses {
                let index = usize::from(table_index);
                let (pages, page_index) = &tables[index];
                drop(pages.get_page(*page_index).unwrap().unwrap());
                history[index] = (history[index].1, epoch);
                epoch += 1;
            }
            let victim = (0u8..4).min_by_key(|&index| {
                let (previous, last) = history[usize::from(index)];
                (previous != 0, if previous == 0 { last } else { previous }, last)
            }).unwrap();

            // The coordinator must choose one global victim, regardless of
            // the order in which tables would have registered their sources.
            let freed = manager.evict_all(3 * PAGE_SIZE as u64).unwrap();
            proptest::prop_assert_eq!(freed, PAGE_SIZE as u64);
            for (index, (pages, page_index)) in tables.iter().enumerate() {
                proptest::prop_assert_eq!(
                    pages.storage.tiered().slots[page_index.idx()].is_resident(),
                    index != usize::from(victim),
                );
            }
        }
    }

    #[test]
    fn allocation_evicts_the_colder_page_from_a_later_table() {
        let limit = 2 * PAGE_SIZE as u64;
        let budget = ByteBudget::new(ByteBudgetConfig {
            tier: CacheTier::Memory,
            low_water_bytes: PAGE_SIZE as u64,
            soft_limit_bytes: limit,
            hard_limit_bytes: limit,
        })
        .unwrap();
        let manager = Arc::new(PageManager::new(
            Arc::new(ResidentObjects::new(budget)),
            PagePool::new_for_test(),
            Arc::new(TieredStorage::new_for_test()),
        ));
        let mut hot = PageSet::new(manager.clone(), EvictionPolicy::Memory, Size(8));
        let mut cold = PageSet::new(manager.clone(), EvictionPolicy::Memory, Size(8));
        let mut writer = PageSet::new(manager, EvictionPolicy::Memory, Size(8));
        let hot_index = hot.allocate_new_page(Size(8)).unwrap();
        let cold_index = cold.allocate_new_page(Size(8)).unwrap();

        // The earlier table has the hotter page. A table-ordered sweep would
        // reclaim it before considering the unread page in the later table.
        drop(hot.get_page(hot_index).unwrap().unwrap());
        let writer_index = writer.allocate_new_page(Size(8)).unwrap();
        assert!(hot.storage.tiered().slots[hot_index.idx()].is_resident());
        assert!(!cold.storage.tiered().slots[cold_index.idx()].is_resident());
        assert!(writer.storage.tiered().slots[writer_index.idx()].is_resident());
    }

    #[test]
    fn insertion_pin_protects_the_row_after_releasing_the_write_lock() {
        for reserved in [false, true] {
            let (manager, mut blobs) = shared_page_and_blob_cache(1);
            let mut pages = PageSet::new(manager.clone(), EvictionPolicy::Memory, Size(8));
            let reservation = reserved.then(|| (PageIndex(0), manager.reserve(Size(8)).unwrap()));
            let (index, offset, pin) = pages
                .with_page_to_insert_row(Size(8), 0, reservation, |page| {
                    // SAFETY: This page and visitor both describe eight-byte integers.
                    unsafe {
                        page.insert_row(
                            &73u64.to_le_bytes(),
                            &[] as &[&[u8]],
                            &NullVarLenVisitor,
                            &mut blobs,
                            EvictionPolicy::Memory,
                        )
                    }
                    .unwrap()
                })
                .unwrap();

            // Simulate another set trimming between the write and creation of
            // the RowRef. The returned pin must bridge that unlocked interval.
            assert!(pin.is_some());
            assert_eq!(manager.evict_all(0).unwrap(), 0);
            let row_pin = pages.get_page(index).unwrap().unwrap();
            drop(pin);
            assert_eq!(manager.evict_all(0).unwrap(), 0);
            drop(row_pin);

            assert_eq!(manager.evict_all(0).unwrap(), PAGE_SIZE as u64);
            assert_fixed_row(&pages, index, offset, 73);
        }
    }

    #[test]
    fn allocation_pin_protects_a_page_before_its_slot_is_installed() {
        let (manager, _blobs) = shared_page_and_blob_cache(1);
        let mut pages = PageSet::new(manager.clone(), EvictionPolicy::Memory, Size(8));
        let reservation = manager.reserve(Size(8)).unwrap();
        let (slot, pin) = manager.redeem_pinned(reservation, EvictionPolicy::Memory);

        // Simulate another set requesting eviction after publication but before
        // installation. The allocation pin must keep the page resident.
        assert_eq!(manager.evict_all(0).unwrap(), 0);
        assert!(slot.is_resident());
        // SAFETY: The pin identifies this slot's resident frame, and the index is fresh.
        let pin = unsafe { pages.install_page_slot(PageIndex(0), slot, pin) };
        assert_eq!(manager.evict_all(0).unwrap(), 0);

        // Installation must not leave behind permanent protection.
        drop(pin);
        assert_eq!(manager.evict_all(0).unwrap(), PAGE_SIZE as u64);
        assert!(!pages.storage.tiered().slots[0].is_resident());
        assert!(pages.get_page(PageIndex(0)).unwrap().is_some());
    }

    #[test]
    fn global_eviction_reclaims_pages_from_another_page_set() {
        let (manager, mut blobs) = shared_page_and_blob_cache(1);
        let mut committed = PageSet::new(manager.clone(), EvictionPolicy::Memory, Size(8));
        let mut transaction = PageSet::new(manager, EvictionPolicy::Memory, Size(8));
        let first = committed.allocate_new_page(Size(8)).unwrap();
        let first_offset = write_fixed_row(&mut committed, &mut blobs, first, 17);

        // Distinct sets share a manager but own their slots independently. An
        // empty transaction must be able to evict a committed set's page.
        let second = transaction.allocate_new_page(Size(8)).unwrap();
        let second_offset = write_fixed_row(&mut transaction, &mut blobs, second, 29);
        assert!(!committed.storage.tiered().slots[first.idx()].is_resident());
        assert!(transaction.storage.tiered().slots[second.idx()].is_resident());

        // Read faults must reclaim the other set too, preserving both rows and
        // each set's page indices across repeated changes of residency.
        assert_fixed_row(&committed, first, first_offset, 17);
        assert!(!transaction.storage.tiered().slots[second.idx()].is_resident());
        assert_fixed_row(&transaction, second, second_offset, 29);
        assert!(!committed.storage.tiered().slots[first.idx()].is_resident());

        drop(transaction);
        assert_fixed_row(&committed, first, first_offset, 17);
    }

    #[test]
    fn global_eviction_allows_pages_and_blobs_to_reclaim_each_other() {
        let (manager, blobs) = shared_page_and_blob_cache(1);
        let mut pages = PageSet::new(manager, EvictionPolicy::Memory, Size(8));
        let bytes = vec![b'x'; PAGE_SIZE];
        let hash = blobs.insert(EvictionPolicy::Memory, &bytes).unwrap().hash();

        // The blob fills the entire budget. Allocating a page can only succeed
        // by evicting that blob, even though the requesting page set is empty.
        let index = pages.allocate_new_page(Size(8)).unwrap();
        assert_eq!(blobs.resident_bytes_used_by_blobs(), 0);
        assert!(pages.storage.tiered().slots[index.idx()].is_resident());

        // Faulting the blob back in must perform the opposite eviction. Then
        // fault the page again to cover both allocation and read-fault retries.
        assert_eq!(&*blobs.get(hash).unwrap(), bytes.as_slice());
        assert!(!pages.storage.tiered().slots[index.idx()].is_resident());
        drop(pages.get_page(index).unwrap().unwrap());
        assert!(pages.storage.tiered().slots[index.idx()].is_resident());
        assert_eq!(blobs.resident_bytes_used_by_blobs(), 0);
        assert_eq!(blobs.resident_usage().accounted_bytes(), PAGE_SIZE as u64);
    }

    #[test]
    fn global_eviction_preserves_a_pinned_page_when_a_blob_cannot_fit() {
        let (manager, blobs) = shared_page_and_blob_cache(1);
        let mut pages = PageSet::new(manager, EvictionPolicy::Memory, Size(8));
        let index = pages.allocate_new_page(Size(8)).unwrap();
        let pin = pages.get_page(index).unwrap().unwrap();

        // A live page handle must block eviction triggered by a different type
        // of object. Failed allocation must not publish a blob catalog entry.
        assert!(matches!(
            blobs.insert(EvictionPolicy::Memory, b"new blob"),
            Err(BlobError::MemoryLimitExceeded(_))
        ));
        assert_eq!(blobs.num_blobs(), 0);
        assert!(pages.storage.tiered().slots[index.idx()].is_resident());
        assert_eq!(blobs.resident_usage().accounted_bytes(), PAGE_SIZE as u64);

        drop(pin);
        drop(blobs.insert(EvictionPolicy::Memory, b"new blob").unwrap());
        assert!(!pages.storage.tiered().slots[index.idx()].is_resident());
    }

    #[test]
    fn global_eviction_registers_pages_created_by_policy_conversion() {
        let (manager, blobs) = shared_page_and_blob_cache(1);
        let mut pages = PageSet::new(manager, EvictionPolicy::Never, Size(8));
        pages.set_contents([Some(Page::new(Size(8)))], Size(8));

        // Never pages cannot spill. Conversion must register their slots even
        // though the page set was originally created with directly owned pages.
        assert!(matches!(
            blobs.insert(EvictionPolicy::Memory, b"new blob"),
            Err(BlobError::MemoryLimitExceeded(_))
        ));
        pages.apply_eviction_policy(EvictionPolicy::Memory);
        drop(blobs.insert(EvictionPolicy::Memory, b"new blob").unwrap());
        assert!(!pages.storage.tiered().slots[0].is_resident());

        drop(pages.get_page(PageIndex(0)).unwrap().unwrap());
        assert!(pages.storage.tiered().slots[0].is_resident());
        assert_eq!(blobs.resident_bytes_used_by_blobs(), 0);
    }

    #[test]
    fn global_eviction_registers_restored_pages_without_changing_their_indices() {
        let (manager, blobs) = shared_page_and_blob_cache(1);
        let mut pages = PageSet::new(manager, EvictionPolicy::Memory, Size(8));

        // Restore bypasses ordinary allocation and can contain absent slots.
        // Its eviction registry must include the live page at its original index.
        pages.set_contents([None, Some(Page::new(Size(8)))], Size(8));
        let bytes = vec![b'x'; PAGE_SIZE];
        drop(blobs.insert(EvictionPolicy::Memory, &bytes).unwrap());
        assert!(!pages.storage.tiered().slots[1].is_resident());
        assert!(pages.get_page(PageIndex(0)).unwrap().is_none());

        drop(pages.get_page(PageIndex(1)).unwrap().unwrap());
        assert!(pages.storage.tiered().slots[1].is_resident());
        assert_eq!(blobs.resident_bytes_used_by_blobs(), 0);
        assert!(pages.get_page(PageIndex(0)).unwrap().is_none());
    }

    #[test]
    fn global_eviction_skips_a_write_locked_page_during_blob_allocation() {
        use std::{sync::mpsc, thread, time::Duration};

        // Run the allocation on a worker so regressing to a blocking slot lock
        // fails with a timeout instead of hanging the whole test suite.
        let (completed, completion) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let (manager, blobs) = shared_page_and_blob_cache(2);
            let mut pages = PageSet::new(manager, EvictionPolicy::Memory, Size(8));
            let locked = pages.allocate_new_page(Size(8)).unwrap();
            let victim = pages.allocate_new_page(Size(8)).unwrap();

            // Blob writes normally happen while mutating a page. The sweep must
            // skip that locked slot, find the other page, and make forward progress.
            pages
                .with_page_mut(locked, Size(8), |_| {
                    drop(blobs.insert(EvictionPolicy::Memory, b"new blob").unwrap());
                })
                .unwrap();
            assert!(pages.storage.tiered().slots[locked.idx()].is_resident());
            assert!(!pages.storage.tiered().slots[victim.idx()].is_resident());
            completed.send(()).unwrap();
        });

        completion
            .recv_timeout(Duration::from_secs(10))
            .expect("global eviction blocked on the allocating thread's page lock");
        worker.join().unwrap();
    }

    #[test]
    fn global_eviction_does_not_keep_dropped_page_sets_alive() {
        let (manager, blobs) = shared_page_and_blob_cache(1);
        let weak_manager = Arc::downgrade(&manager);

        // Repeatedly discard transaction-like sets. Their weak registrations
        // must not retain resident allocations or prevent subsequent eviction.
        for _ in 0..3 {
            let mut temporary = PageSet::new(manager.clone(), EvictionPolicy::Memory, Size(8));
            temporary.allocate_new_page(Size(8)).unwrap();
            drop(temporary);
            assert_eq!(blobs.resident_usage().accounted_bytes(), 0);
        }
        let mut live = PageSet::new(manager.clone(), EvictionPolicy::Memory, Size(8));
        let index = live.allocate_new_page(Size(8)).unwrap();
        drop(blobs.insert(EvictionPolicy::Memory, b"new blob").unwrap());
        assert!(!live.storage.tiered().slots[index.idx()].is_resident());

        drop(live);
        drop(manager);
        drop(blobs);
        assert!(
            weak_manager.upgrade().is_none(),
            "eviction sources formed an ownership cycle"
        );
    }

    fn bounded_page_set() -> PageSet {
        let budget = ByteBudget::new(ByteBudgetConfig {
            tier: CacheTier::Memory,
            low_water_bytes: 0,
            soft_limit_bytes: PAGE_SIZE as u64,
            hard_limit_bytes: PAGE_SIZE as u64,
        })
        .unwrap();
        let manager = PageManager::new(
            Arc::new(ResidentObjects::new(budget)),
            PagePool::new_for_test(),
            Arc::new(TieredStorage::new_for_test()),
        );
        PageSet::new(Arc::new(manager), EvictionPolicy::Memory, Size(8))
    }

    #[test]
    fn allocation_evicts_an_unpinned_page_and_faults_it_back() {
        let mut pages = bounded_page_set();
        let first = pages.allocate_new_page(Size(8)).unwrap();

        let second = pages.allocate_new_page(Size(8)).unwrap();
        assert!(!pages.storage.tiered().slots[first.idx()].is_resident());
        assert!(pages.storage.tiered().slots[second.idx()].is_resident());

        pages.get_page(first).unwrap().unwrap();
        assert!(pages.storage.tiered().slots[first.idx()].is_resident());
        assert!(!pages.storage.tiered().slots[second.idx()].is_resident());
    }

    #[test]
    fn pinned_page_blocks_eviction_until_the_pin_is_dropped() {
        let mut pages = bounded_page_set();
        let first = pages.allocate_new_page(Size(8)).unwrap();
        let pin = pages.storage.tiered().slots[first.idx()].page().unwrap();

        assert!(matches!(
            pages.allocate_new_page(Size(8)),
            Err(PageError::MemoryLimitExceeded(_))
        ));
        assert!(pages.storage.tiered().slots[first.idx()].is_resident());

        drop(pin);
        pages.allocate_new_page(Size(8)).unwrap();
        assert!(!pages.storage.tiered().slots[first.idx()].is_resident());
    }

    #[test]
    fn dirtying_or_freeing_a_page_releases_its_old_backing() {
        let storage = Arc::new(TieredStorage::new_for_test());
        let manager = Arc::new(PageManager::new(
            Arc::new(ResidentObjects::new(ByteBudget::unlimited())),
            PagePool::new_for_test(),
            storage.clone(),
        ));
        let mut pages = PageSet::new(manager, EvictionPolicy::Memory, Size(8));

        // Persist a clean version and remember its serialized disk footprint.
        let index = pages.allocate_new_page(Size(8)).unwrap();
        pages.evict(0).unwrap();
        let key = match &*pages.storage.tiered().slots[index.idx()].lock_ref() {
            PageSlot::NonResident { backing, .. } => backing.key(),
            _ => panic!("page should be evicted"),
        };
        let stored_bytes = storage.stat(key).unwrap().size;

        // A write guard dirties the page, releasing ownership of the previous version.
        pages.with_page_mut(index, Size(8), |_| ()).unwrap();
        assert_eq!(storage.reclaim().unwrap(), stored_bytes);
        assert!(!storage.contains(key));

        // Freeing a nonresident page must release the newly stored version too.
        pages.evict(0).unwrap();
        pages.storage.tiered().slots[index.idx()].free();
        assert_eq!(storage.reclaim().unwrap(), stored_bytes);
        assert!(!storage.contains(key));
    }

    #[test]
    fn reads_refresh_page_recency() {
        let budget = ByteBudget::new(ByteBudgetConfig {
            tier: CacheTier::Memory,
            low_water_bytes: 0,
            soft_limit_bytes: 2 * PAGE_SIZE as u64,
            hard_limit_bytes: 2 * PAGE_SIZE as u64,
        })
        .unwrap();
        let manager = Arc::new(PageManager::new(
            Arc::new(ResidentObjects::new(budget)),
            PagePool::new_for_test(),
            Arc::new(TieredStorage::new_for_test()),
        ));
        let mut pages = PageSet::new(manager, EvictionPolicy::Disk, Size(8));
        let hot = pages.allocate_new_page(Size(8)).unwrap();
        let cold = pages.allocate_new_page(Size(8)).unwrap();
        for _ in 0..2 {
            drop(pages.get_page(hot).unwrap().unwrap());
        }

        pages.evict(PAGE_SIZE as u64).unwrap();
        assert!(
            pages.storage.tiered().slots[hot.idx()].is_resident(),
            "a repeatedly read Disk-policy page should outlive the unread page"
        );
        assert!(!pages.storage.tiered().slots[cold.idx()].is_resident());
    }
}
