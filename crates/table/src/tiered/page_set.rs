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
    table::{BlobNumBytes, PreparedInsert, Table},
    tiered::{
        page_manager::{PageEvictionPolicy, PageHandle, PageManager, PageSlotHandle, ReservedPage},
        BlobError, BlobReadSet, BlobSet, PageError,
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
    slots: Vec<PageSlotHandle>,
    free_page_slots: BTreeSet<PageIndex>,
    non_full_pages: BTreeSet<(usize, PageIndex)>,
    pub(crate) manager: Arc<PageManager>,
}

impl PageSet {
    /// Create a fresh, empty page set.
    pub fn new(manager: Arc<PageManager>, eviction_policy: PageEvictionPolicy, fixed_row_size: Size) -> Self {
        Self {
            eviction_policy,
            fixed_row_size,
            slots: <_>::default(),
            free_page_slots: <_>::default(),
            non_full_pages: <_>::default(),
            manager,
        }
    }

    /// Populate this [PageSet] with the `pages` acquired externally.
    ///
    /// This method is provided for compatibility. It is used when restoring
    /// from a snapshot, or a schema migration on an empty table.
    pub fn set_contents(&mut self, pages: impl IntoIterator<Item = Option<Box<Page>>>, fixed_row_size: Size) {
        assert!(self.slots.is_empty());

        // This is ok, because the table is empty.
        self.fixed_row_size = fixed_row_size;
        self.slots = self.manager.register(self.eviction_policy, self.fixed_row_size, pages);
        self.non_full_pages = self
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
        self.free_page_slots = self
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
    pub fn get_page(&self, index: PageIndex) -> Result<Option<PageHandle>, PageError> {
        let Some(slot) = self.slots.get(index.idx()) else {
            return Ok(None);
        };
        self.manager.get(slot, self.eviction_policy)
    }

    /// Iterate over all pages in this page set, ensuring that the page's hash
    /// is computed before yielding it.
    ///
    /// This will fault in each page if it is not resident.
    ///
    /// Used when capturing a snapshot.
    pub(crate) fn iter_pages_with_hashes(
        &self,
    ) -> impl Iterator<Item = Result<Option<(blake3::Hash, PageHandle)>, PageError>> {
        self.slots.iter().map(|slot| {
            if slot.is_absent() {
                Ok(None)
            } else {
                let mut page = self.manager.get_mut(slot, self.eviction_policy, self.fixed_row_size)?;
                let hash = page.save_or_get_content_hash();

                Ok(Some((hash, page.into_page_handle())))
            }
        })
    }

    /// Run a closure over a mutable [Page].
    ///
    /// The page is faulted in if it is not already resident.
    ///
    /// Note that the fullness status of the page is updated after `f` returns,
    /// but the page is not freed if it is empty at that point. Use [Self::delete_row]
    /// to free the page immediately if it is empty.
    pub fn with_page_mut<T>(
        &mut self,
        index: PageIndex,
        fixed_row_size: Size,
        f: impl FnOnce(&mut Page) -> T,
    ) -> Result<T, PageError> {
        let Some(slot) = self.slots.get(index.idx()) else {
            return Err(PageError::MissingPage(index));
        };
        let mut page = self.manager.get_mut(slot, self.eviction_policy, self.fixed_row_size)?;
        self.non_full_pages.remove(&(page.available_var_len_granules(), index));
        let ret = f(&mut page);
        if !page.is_full(fixed_row_size) {
            self.non_full_pages.insert((page.available_var_len_granules(), index));
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
    pub fn with_page_to_insert_row<T>(
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
        blobs: &mut BlobSet,
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
            self.slots[page_index.idx()].free();
            self.free_page_slots.insert(page_index);
            self.non_full_pages.remove(&(available_granules, page_index));
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
            .non_full_pages
            .range((num_var_len_granules, PageIndex(0))..)
            .copied()
            .find(|(_, page_index)| {
                self.slots[page_index.idx()]
                    .has_space_for_row(fixed_row_size, num_var_len_granules)
                    .expect("page in `self.non_full_pages` to be present in `self.pages`")
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
    fn register(&mut self, index: PageIndex, reservation: ReservedPage) -> PageHandle {
        let slot = self.manager.redeem(reservation, self.eviction_policy);
        // SAFETY: The page is resident. The index was obtained during commit
        // planning.
        unsafe { self.install_page_slot(index, slot) }
    }

    /// SAFETY:
    /// - The [PageSlotHandle] must contain a resident page.
    /// - If there already exists an entry at `index`, the existing slot must be
    ///   absent.
    unsafe fn install_page_slot(&mut self, index: PageIndex, slot: PageSlotHandle) -> PageHandle {
        let page = slot.page().expect("page must be resident");

        let idx = index.idx();
        if idx == self.slots.len() {
            self.slots.push(slot);
        } else {
            assert!(self.slots[idx].is_absent());
            self.free_page_slots.remove(&index);
            self.slots[idx] = slot;
        }

        self.non_full_pages
            .insert((page.read().available_var_len_granules(), index));

        page
    }

    /// Is there space to allocate another page?
    pub fn can_allocate_new_page(&self) -> Result<PageIndex, PageError> {
        let new_idx = self.slots.len();
        if new_idx <= PageIndex::MAX.idx() {
            Ok(PageIndex(new_idx as _))
        } else {
            Err(PageError::TooManyPages)
        }
    }

    /// Iterate over all pages that have not been freed.
    ///
    /// This yields [PageSlotHandle]s and doesn't fault in pages.
    pub fn iter_present_pages(&self) -> impl Iterator<Item = &PageSlotHandle> {
        self.slots.iter().filter(|slot| !slot.is_absent())
    }

    /// Iterate over all pagess that have not been freed, along with their
    /// [PageIndex].
    ///
    /// Because absent pages are skipped, the [PageIndex] sequence may contain
    /// gaps.
    ///
    /// Yields [PageSlotHandle]s and doesn't fault in pages.
    pub fn iter_present_pages_with_page_index(&self) -> impl Iterator<Item = (PageIndex, &PageSlotHandle)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(idx, slot)| (!slot.is_absent()).then_some((PageIndex(idx as _), slot)))
    }

    /// The number of present pages in `self`.
    ///
    /// Includes resident as well as non-resident pages, but not freed ones.
    pub fn num_present_pages(&self) -> usize {
        self.slots
            .len()
            .checked_sub(self.free_page_slots.len())
            .expect("pages len to be greater than number of free slots")
    }

    /// The total number of pages in `self`.
    ///
    /// Includes resident, non-resident and absent (i.e. freed) pages.
    pub fn num_pages(&self) -> usize {
        self.slots.len()
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
        inserts: impl IntoIterator<Item = Result<(PreparedInsert, usize), PageError>>,
    ) -> Result<PreparedCommit, PageError> {
        let mut allocator = PageAllocator::new(self, fixed_row_size);
        let mut pinned = BTreeMap::new();

        let deletes = deletes
            .into_iter()
            .map(|row_ptr| {
                let page_index = row_ptr.page_index();
                if let btree_map::Entry::Vacant(entry) = pinned.entry(page_index) {
                    let page = self.get_page(page_index)?.expect("delete from absent page");
                    entry.insert(page);
                }
                let granules = unsafe {
                    pinned[&page_index]
                        .read()
                        .row_total_granules(row_ptr.page_offset(), fixed_row_size, visitor)
                };
                allocator.prepare_delete::<Infallible>(page_index, granules, |page_index| {
                    Ok(pinned[&page_index].read().metadata(fixed_row_size).into())
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
                            let page = entry.insert(page);
                            Ok(page.read().metadata(fixed_row_size).into())
                        }
                        btree_map::Entry::Occupied(entry) => Ok(entry.get().read().metadata(fixed_row_size).into()),
                    }
                })?;
                Ok(PlannedInsert { page_index, row })
            })
            .collect::<Result<_, PageError>>()?;

        let reserved = allocator
            .pages_to_allocate()
            .map(|page_index| {
                self.manager
                    .reserve(fixed_row_size)
                    .map(|reservation| (page_index, reservation))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        Ok(PreparedCommit {
            pinned,
            reserved,
            deletes,
            inserts,
        })
    }
}

#[cfg(test)]
impl PageSet {
    pub(crate) fn assert_non_full_pages_consistent(&self, fixed_row_size: Size) {
        let mut page_granules = BTreeMap::new();
        for &(avail, page_index) in &self.non_full_pages {
            assert!(
                page_granules.insert(page_index, avail).is_none(),
                "page {:?} appears multiple times in non_full_pages",
                page_index,
            );
        }
        for (idx, slot) in self.slots.iter().enumerate() {
            let page_index = PageIndex(idx as _);
            let entry = page_granules.get(&page_index).copied();
            if !slot.is_absent() {
                let is_full = slot.is_full(fixed_row_size).unwrap();
                let available_granules = slot.available_var_len_granules().unwrap();

                if is_full {
                    assert!(
                        entry.is_none(),
                        "page {:?} has 0 available var-len granules but appears in non_full_pages as {:?}",
                        page_index,
                        entry
                    );
                } else {
                    assert_eq!(
                        entry,
                        Some(available_granules),
                        "page {:?} has {} available var-len granules but non_full_pages has {:?}",
                        page_index,
                        available_granules,
                        entry
                    );
                }
            } else {
                assert!(
                    entry.is_none(),
                    "page slot {:?} is is absent, but appears in non_full_pages as {:?}",
                    page_index,
                    entry,
                );
            }
        }
    }

    pub(crate) fn iter_present_page_indexes(&self) -> impl Iterator<Item = PageIndex> {
        self.iter_present_pages_with_page_index().map(|(idx, _)| idx)
    }
}

impl MemoryUsage for PageSet {
    fn heap_usage(&self) -> usize {
        let Self {
            eviction_policy: _,
            fixed_row_size: _,
            slots,
            free_page_slots,
            non_full_pages,
            // TODO: The manager is shared by all page sets of a database, how
            // should we account for it?
            manager: _,
        } = self;

        slots.heap_usage() + free_page_slots.heap_usage() + non_full_pages.heap_usage()
    }
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
}

impl PreparedCommit {
    /// Apply this transaction to [Table].
    ///
    /// The table's [PageSet] must be the same as the one the [PreparedCommit]
    /// was created from.
    pub fn apply(mut self, table: &mut Table, blobs: &BlobReadSet) -> AppliedCommit {
        let mut pinned = self.pinned;

        fn collect_arc_slice<T, U>(iter: impl ExactSizeIterator<Item = T>, mut f: impl FnMut(T) -> U) -> Arc<[U]> {
            let mut arc_slice = Arc::new_uninit_slice(iter.len());
            let arc_slice_mut = Arc::get_mut(&mut arc_slice).expect("`Arc` must be unique as it was just created");

            for (x, slot) in iter.into_iter().zip(arc_slice_mut) {
                slot.write(f(x));
            }

            // SAFETY: We wrote to every slot in `arc_slice`, so it is now fully
            // initialized.
            unsafe { arc_slice.assume_init() }
        }

        let deletes = collect_arc_slice(self.deletes.into_iter(), |row_ptr| {
            table
                .delete(blobs, row_ptr, |row| row.to_product_value())
                .expect("no page faults")
                .expect("`Table::delete` never returns `None`")
        });
        let inserts = collect_arc_slice(self.inserts.into_iter(), |PlannedInsert { page_index, row }| {
            let schema = table.get_schema();
            // For event tables, we don't insert into the committed state. The
            // row is collected regardless, as we include it in subscriptions
            // and the commitlog.
            if !schema.is_event {
                let reservation = self.reserved.remove(&page_index).map(|page| (page_index, page));
                let row_ref = table
                    .insert_prepared(blobs, &row, reservation)
                    .map(|(_, row_ref)| row_ref)
                    .expect("failed to insert during transaction commit");
                let (page, _) = row_ref.page_and_offset();
                let inserted_index = row_ref.pointer().page_index();
                pinned.entry(inserted_index).or_insert_with(|| page.clone());
                assert_eq!(inserted_index, page_index, "planned and actual placement differ");

                row.into_product_value(Some(&row_ref))
            } else {
                row.into_product_value(None)
            }
        });

        drop(pinned);

        AppliedCommit { deletes, inserts }
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
            next_new_page: pages.slots.len(),
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
            .non_full_pages
            .range((row_granules, PageIndex(0))..)
            // A planned page shadows committed state.
            .filter(|(_, index)| !self.planned.contains_key(index))
            .find_map(|&(_, index)| {
                self.pages.slots[index.idx()]
                    .has_space_for_row(self.fixed_row_size, row_granules)
                    .unwrap()
                    .then_some(index)
            });

        match (planned, committed) {
            (Some(a), Some(b)) => {
                let a_free = self.available_granules(a).unwrap();
                let b_free = self.pages.slots[b.idx()].available_var_len_granules().unwrap();
                Some(if (a_free, a) <= (b_free, b) { a } else { b })
            }
            (a, b) => a.or(b),
        }
    }

    fn allocate_slot(&mut self) -> PageIndex {
        if let Some(index) = self
            .pages
            .free_page_slots
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
