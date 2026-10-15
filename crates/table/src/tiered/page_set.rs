use std::{
    collections::{btree_map, BTreeMap, BTreeSet},
    convert::Infallible,
    sync::{Arc, Mutex},
};

use spacetimedb_data_structures::map::IntMap;
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
        mem::{PageEvictionCandidate, PageId},
        page_manager::{PageEvictionPolicy, PageHandle, PageManager, PageSlot, PageSlotHandle, ReservedPage},
        BlobError, BlobSet, BudgetPermit, PageError, PageRead, PageRef,
    },
    var_len::VarLenMembers,
};

#[derive(Debug, thiserror::Error)]
pub enum DeleteRowError {
    #[error(transparent)]
    Page(#[from] PageError),
    #[error(transparent)]
    Blob(#[from] BlobError),
}

/// The set of pages of a [Table].
#[derive(Debug)]
pub struct PageSet {
    pub eviction_policy: PageEvictionPolicy,
    fixed_row_size: Size,
    pub(crate) manager: Arc<PageManager>,
    storage: PageStorage,
}

/// The original directly owned page representation, with allocation accounting.
/// A table borrow protects page access; no per-page ownership or locks are needed.
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

#[derive(Debug, Default)]
struct TieredPages {
    slots: Vec<PageSlotHandle>,
    free_page_slots: BTreeSet<PageIndex>,
    non_full_pages: BTreeSet<(usize, PageIndex)>,
    evictable: Mutex<EvictionScratch>,
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
    #[inline(always)]
    fn with_page_to_insert_row<T>(
        &mut self,
        manager: &PageManager,
        fixed_row_size: Size,
        num_var_len_granules: usize,
        reservation: Option<(PageIndex, ReservedPage)>,
        f: impl FnOnce(&mut Page) -> T,
    ) -> Result<(PageIndex, T), PageError> {
        let index = match reservation {
            Some((index, reservation)) => {
                self.install(index, reservation);
                index
            }
            None => match self
                .pages
                .take_non_full_page(fixed_row_size, num_var_len_granules)
            {
                Some(index) => index,
                None => {
                    let index = self
                        .pages
                        .next_page_index()
                        .map_err(|_| PageError::TooManyPages)?;
                    let reservation = manager.reserve_resident(fixed_row_size)?;
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
    pub unsafe fn reconstruct_bytes_used_by_rows(
        &self,
        size: Size,
        visitor: &impl VarLenMembers,
    ) -> usize {
        unsafe {
            match self {
                Self::Resident(p) => p.reconstruct_bytes_used_by_rows(size, visitor),
                Self::Tiered(p) => p.reconstruct_bytes_used_by_rows(size, visitor),
            }
        }
    }
}
type EvictionScratch = (Box<Vec<PageEvictionCandidate>>, Box<IntMap<PageId, PageIndex>>);

impl PageSet {
    #[inline]
    pub(crate) fn is_resident(&self) -> bool {
        matches!(self.storage, PageStorage::Resident(_))
    }

    #[inline]
    pub(crate) fn resident_row_present(
        &self,
        pointer: RowPointer,
        fixed_row_size: Size,
    ) -> Option<bool> {
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
    pub(crate) unsafe fn delete_fixed_resident_row(
        &mut self,
        pointer: RowPointer,
        fixed_row_size: Size,
    ) -> bool {
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
            let page = resident.pages.free_empty_page(index);
            let permit = resident.permits[index.idx()]
                .take()
                .expect("present page has a permit");
            self.manager.recycle_resident(page, permit);
        }
        true
    }

    /// Create a fresh, empty page set.
    pub fn new(manager: Arc<PageManager>, eviction_policy: PageEvictionPolicy, fixed_row_size: Size) -> Self {
        let storage = match eviction_policy {
            PageEvictionPolicy::NeverEvict => PageStorage::Resident(ResidentPages {
                pages: Pages::default(),
                permits: smallvec::SmallVec::new(),
            }),
            PageEvictionPolicy::Evictable => PageStorage::Tiered(TieredPages::default()),
        };
        Self {
            eviction_policy,
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
            return Ok(resident.pages.get(index).map(PageRef::Resident));
        }
        let Some(slot) = self.storage.tiered().slots.get(index.idx()) else {
            return Ok(None);
        };
        self.manager
            .get(slot, self.eviction_policy)
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
            PageStorage::Resident(resident) => {
                itertools::Either::Left(resident.pages.iter_mut().map(|page| {
                    Ok(page.as_deref_mut().map(|page| {
                        let hash = page.save_or_get_content_hash();
                        (hash, PageRef::Resident(page))
                    }))
                }))
            }
            PageStorage::Tiered(tiered) => {
                itertools::Either::Right(tiered.slots.iter().map(|slot| {
                    if slot.is_absent() {
                        Ok(None)
                    } else {
                        let mut page = self.manager.get_mut(
                            slot,
                            self.eviction_policy,
                            self.fixed_row_size,
                        )?;
                        let hash = page.save_or_get_content_hash();
                        Ok(Some((hash, PageRef::Tiered(page.into_page_handle()))))
                    }
                }))
            }
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
        let Some(slot) = self.storage.tiered().slots.get(index.idx()) else {
            return Err(PageError::MissingPage(index));
        };
        let mut page = self
            .manager
            .get_mut(slot, self.eviction_policy, self.fixed_row_size)?;
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
        Ok(ret)
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
    /// it ends up being empty after `f` returns.
    #[inline(always)]
    pub fn with_page_to_insert_row<T>(
        &mut self,
        fixed_row_size: Size,
        num_var_len_granules: usize,
        reservation: Option<(PageIndex, ReservedPage)>,
        f: impl FnOnce(&mut Page) -> T,
    ) -> Result<(PageIndex, T), PageError> {
        if let Some(resident) = self.storage.resident_mut() {
            return resident.with_page_to_insert_row(
                &self.manager,
                fixed_row_size,
                num_var_len_granules,
                reservation,
                f,
            );
        }
        self.with_tiered_page_to_insert_row(fixed_row_size, num_var_len_granules, reservation, f)
    }

    fn with_tiered_page_to_insert_row<T>(
        &mut self,
        fixed_row_size: Size,
        num_var_len_granules: usize,
        reservation: Option<(PageIndex, ReservedPage)>,
        f: impl FnOnce(&mut Page) -> T,
    ) -> Result<(PageIndex, T), PageError> {
        let index = match reservation {
            None => self.find_page_with_space_for_row(fixed_row_size, num_var_len_granules)?,
            Some((index, page)) => {
                self.register(index, page);
                index
            }
        };
        self.with_page_mut(index, fixed_row_size, f).map(|res| (index, res))
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
    ) -> Result<BlobNumBytes, DeleteRowError> {
        let page_index = row_ptr.page_index();
        let (blob_bytes_deleted, is_empty, available_granules) =
            self.with_page_mut(page_index, fixed_row_size, |page| {
                // SAFETY:
                // - `row_ptr.page_offset()` does point to a valid row in this page
                //   as the caller promised that `row_ptr` points to a valid row in `self`.
                //
                // - `fixed_row_size` is consistent with the size in bytes of the fixed part of the row.
                //   The size is also conistent with `var_len_visitor`.
                let blob_bytes_deleted =
                    unsafe { page.delete_row(row_ptr.page_offset(), fixed_row_size, var_len_visitor, blobs) }?;

                Ok::<_, BlobError>((
                    blob_bytes_deleted,
                    page.num_rows() == 0,
                    page.available_var_len_granules(),
                ))
            })??;

        if is_empty {
            if let Some(resident) = self.storage.resident_mut() {
                let page = resident.pages.free_empty_page(page_index);
                let permit = resident.permits[page_index.idx()]
                    .take()
                    .expect("present page has a permit");
                self.manager.recycle_resident(page, permit);
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
    ) -> Result<PageIndex, PageError> {
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
            return Ok(page_idx);
        }

        self.allocate_new_page(fixed_row_size)
    }

    /// Allocates one additional page,
    /// returning an error if the new number of pages would overflow `PageIndex::MAX`.
    ///
    /// Allocation may also fail if the page manager has insufficient
    /// [crate::tiered::budget::ByteBudget] to reserve another page.
    ///
    /// The new page is initially empty, but is not added to the non-full set.
    /// Callers should call [`Pages::record_page_non_full`] after operating on the new page.
    fn allocate_new_page(&mut self, fixed_row_size: Size) -> Result<PageIndex, PageError> {
        let page_index = self
            .storage
            .tiered_mut()
            .free_page_slots
            .pop_first()
            .map(Ok)
            .unwrap_or_else(|| self.can_allocate_new_page())?;
        let slot = self.manager.allocate(fixed_row_size, self.eviction_policy)?;
        // SAFETY: The page is resident and the index is from free slots or
        // fresh.
        unsafe { self.install_page_slot(page_index, slot) };

        Ok(page_index)
    }

    /// Register a [ReservedPage] with this [PageSet].
    ///
    /// Pages are reserved during [Self::prepare_commit].
    fn register(&mut self, index: PageIndex, reservation: ReservedPage) {
        let slot = self.manager.redeem(reservation, self.eviction_policy);
        // SAFETY: The page is resident. The index was obtained during commit
        // planning.
        unsafe { self.install_page_slot(index, slot) };
    }

    /// SAFETY:
    /// - The [PageSlotHandle] must contain a resident page.
    /// - If there already exists an entry at `index`, the existing slot must be
    ///   absent.
    unsafe fn install_page_slot(&mut self, index: PageIndex, slot: PageSlotHandle) -> PageHandle {
        let page = slot.page().expect("page must be resident");

        let idx = index.idx();
        if idx == self.storage.tiered().slots.len() {
            self.storage.tiered_mut().slots.push(slot);
        } else {
            assert!(self.storage.tiered().slots[idx].is_absent());
            self.storage.tiered_mut().free_page_slots.remove(&index);
            self.storage.tiered_mut().slots[idx] = slot;
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
        self.iter_present_pages_with_page_index()
            .map(|(_, page)| page)
    }

    /// Iterate over all pages that have not been freed, along with their
    /// [PageIndex].
    ///
    /// Because absent pages are skipped, the [PageIndex] sequence may contain
    /// gaps.
    ///
    /// Yields borrowed page metadata without faulting in tiered pages.
    pub fn iter_present_pages_with_page_index(
        &self,
    ) -> impl Iterator<Item = (PageIndex, PageInfo<'_>)> {
        if let Some(resident) = self.storage.resident() {
            return itertools::Either::Left(
                resident
                    .pages
                    .iter_present_pages_with_page_index()
                    .map(|(index, page)| (index, PageInfo::Resident(page))),
            );
        }
        itertools::Either::Right(self.storage.tiered().slots.iter().enumerate().filter_map(
            |(idx, slot)| {
                (!slot.is_absent()).then_some((PageIndex(idx as _), PageInfo::Tiered(slot)))
            },
        ))
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
        inserts: impl IntoIterator<Item = Result<(PreparedInsert, usize), TableError>>,
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
                let granules = unsafe {
                    page.row_total_granules(row_ptr.page_offset(), fixed_row_size, visitor)
                };
                allocator.prepare_delete::<Infallible>(page_index, granules, |_| {
                    Ok(page.metadata(fixed_row_size).into())
                });

                Ok(row_ptr)
            })
            .collect::<Result<_, PageError>>()?;

        let inserts = inserts
            .into_iter()
            .map(|res| {
                let (row, num_granules) = res?;
                let page_index = allocator.prepare_insert::<PageError>(num_granules, |page_index| {
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
                self.manager
                    .reserve(fixed_row_size)
                    .map(|reservation| (page_index, reservation))
                    .map_err(PageError::from)
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

    pub fn evict(&self) -> Result<(), PageError> {
        if self.storage.resident().is_some() {
            return Ok(());
        }
        let tiered = self.storage.tiered();
        let mut _guard = tiered.evictable.lock().unwrap();
        let (candidates, slots) = &mut *_guard;

        // Collect eviction candidates, returning early if there aren't any.
        self.manager.evictable(candidates);
        if candidates.is_empty() {
            return Ok(());
        }

        // Only resident slots have a page id, so collect them now.
        slots.clear();
        slots.extend(
            tiered
                .slots
                .iter()
                .enumerate()
                .filter_map(|(idx, slot_handle)| {
                    let page_id = slot_handle.page_id()?;
                    Some((page_id, PageIndex(idx as _)))
                }),
        );

        for candidate in candidates.drain(..) {
            let Some(page_index) = slots.remove(&candidate.id) else {
                continue;
            };
            let mut slot = tiered.slots[page_index.idx()].lock();
            match &*slot {
                PageSlot::Resident {
                    handle: page_handle,
                    metadata,
                    backing,
                    eviction_policy: PageEvictionPolicy::Evictable,
                } => {
                    let hash = backing
                        .map(Ok)
                        .unwrap_or_else(|| self.manager.ensure_lower_tier_stored(page_handle))?;
                    *slot = PageSlot::NonResident {
                        hash,
                        metadata: *metadata,
                    };
                    self.manager.evicted(candidate);
                }
                _ => continue,
            }
        }

        Ok(())
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
                pages.slots.heap_usage()
                    + pages.free_page_slots.heap_usage()
                    + pages.non_full_pages.heap_usage()
            }
        }
    }
}

#[inline]
fn collect_arc_slice<T, U>(
    len: u64,
    iter: impl Iterator<Item = T>,
    mut f: impl FnMut(T) -> U,
) -> Arc<[U]> {
    let mut output =
        Arc::new_uninit_slice(usize::try_from(len).expect("row count fits the address space"));
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
            Some(source) => {
                collect_arc_slice(insert_count, source.scan_rows(source_blobs), |row| {
                    let row = row
                        .expect("resident page access cannot fail")
                        .to_product_value();
                    if !is_event {
                        table
                            .insert(blobs, &row)
                            .expect("resident commit was preflighted");
                    }
                    row
                })
            }
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

        let deletes = collect_arc_slice(
            self.deletes.len() as u64,
            self.deletes.into_iter(),
            |row_ptr| {
                table
                    .delete(blobs, row_ptr, |row| row.to_product_value())
                    .expect("no page faults")
                    .expect("`Table::delete` never returns `None`")
            },
        );
        let inserts = collect_arc_slice(
            self.inserts.len() as u64,
            self.inserts.into_iter(),
            |PlannedInsert { page_index, row }| {
                let schema = table.get_schema();
                // For event tables, we don't insert into the committed state. The
                // row is collected regardless, as we include it in subscriptions
                // and the commitlog.
                if !schema.is_event {
                    let reservation = self
                        .reserved
                        .remove(&page_index)
                        .map(|page| (page_index, page));
                    let row_ref = table
                        .insert_prepared(blobs, &row, reservation)
                        .map(|(_, row_ref)| row_ref)
                        .expect("failed to insert during transaction commit");
                    let (page, _) = row_ref.page_and_offset();
                    let inserted_index = row_ref.pointer().page_index();
                    if let btree_map::Entry::Vacant(entry) = pinned.entry(inserted_index)
                        && let Some(pin) = page.pin()
                    {
                        entry.insert(pin);
                    }
                    assert_eq!(
                        inserted_index, page_index,
                        "planned and actual placement differ"
                    );

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
                let b_free = self
                    .pages
                    .page_info(b)
                    .unwrap()
                    .available_var_len_granules()
                    .unwrap();
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
