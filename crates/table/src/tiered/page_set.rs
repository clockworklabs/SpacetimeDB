use std::{
    collections::{btree_map, BTreeMap, BTreeSet},
    convert::Infallible,
    sync::Arc,
};

use spacetimedb_lib::ProductValue;
use spacetimedb_memory_usage::MemoryUsage;
use spacetimedb_sats::layout::Size;

use crate::{
    blob_store::BlobStore,
    indexes::{PageIndex, RowPointer},
    page::{Page, PageCapacity, PageMetadata},
    table::{BlobNumBytes, Table},
    tiered::page_manager::{PageEvictionPolicy, PageHandle, PageManager, PageSlotHandle, ReservedPage},
    var_len::VarLenMembers,
};

pub use crate::tiered::page_manager::PageError;

#[derive(Debug)]
pub struct PageSet {
    pub eviction_policy: PageEvictionPolicy,
    slots: Vec<PageSlotHandle>,
    free_page_slots: BTreeSet<PageIndex>,
    non_full_pages: BTreeSet<(usize, PageIndex)>,
    pub(crate) manager: Arc<PageManager>,
}

impl PageSet {
    /// Create a fresh, empty page set.
    pub fn new(manager: Arc<PageManager>, eviction_policy: PageEvictionPolicy) -> Self {
        Self {
            eviction_policy,
            slots: <_>::default(),
            free_page_slots: <_>::default(),
            non_full_pages: <_>::default(),
            manager,
        }
    }

    /// Populate this [PageSet] with the `pages` acquired externally.
    ///
    /// This method is provided for compatibility. It is used when restoring
    /// from a snapshot.
    pub fn set_contents(&mut self, pages: impl IntoIterator<Item = Option<Box<Page>>>, fixed_row_size: Size) {
        assert!(self.slots.is_empty());

        self.slots = self.manager.register(self.eviction_policy, pages);
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

    pub fn get_page(&self, index: PageIndex) -> Result<Option<PageHandle>, PageError> {
        let Some(slot) = self.slots.get(index.idx()) else {
            return Ok(None);
        };
        self.manager.get(slot, self.eviction_policy)
    }

    pub fn iter_pages_with_hashes(
        &self,
    ) -> impl Iterator<Item = Result<Option<(blake3::Hash, PageHandle)>, PageError>> {
        self.slots.iter().map(|slot| {
            if slot.is_absent() {
                Ok(None)
            } else {
                let hash = self
                    .manager
                    .with_page_mut(slot, self.eviction_policy, |page| page.save_or_get_content_hash())?;
                let page = self.manager.get(slot, self.eviction_policy)?.unwrap();

                Ok(Some((hash, page)))
            }
        })
    }

    pub fn with_page_mut<T>(
        &mut self,
        index: PageIndex,
        fixed_row_size: Size,
        f: impl FnOnce(&mut Page) -> T,
    ) -> Result<T, PageError> {
        let Some(slot) = self.slots.get(index.idx()) else {
            return Err(PageError::MissingPage(index));
        };
        let mut is_empty = false;
        let mut available_granules = None;
        let ret = self.manager.with_page_mut(slot, self.eviction_policy, |page| {
            self.non_full_pages.remove(&(page.available_var_len_granules(), index));
            let ret = f(page);
            is_empty = page.num_rows() == 0;
            if !is_empty && !page.is_full(fixed_row_size) {
                available_granules = Some(page.available_var_len_granules());
            }
            ret
        })?;

        if is_empty {
            slot.free();
            self.free_page_slots.insert(index);
        } else if let Some(available_granules) = available_granules {
            self.non_full_pages.insert((available_granules, index));
        }
        Ok(ret)
    }

    pub fn with_page_to_insert_row<T>(
        &mut self,
        fixed_row_size: Size,
        num_var_len_granules: usize,
        reservation: Option<(PageIndex, ReservedPage)>,
        f: impl FnOnce(&mut Page) -> T,
    ) -> Result<(PageIndex, T), PageError> {
        let index = match reservation {
            None => {
                //eprintln!("finding page for insert");
                self.find_page_with_space_for_row(fixed_row_size, num_var_len_granules)?
            }
            Some((index, page)) => {
                //eprintln!("registering reservation at {index:?} for insert");
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
        blob_store: &mut dyn BlobStore,
    ) -> Result<BlobNumBytes, PageError> {
        let page_index = row_ptr.page_index();
        self.with_page_mut(page_index, fixed_row_size, |page| {
            // SAFETY:
            // - `row_ptr.page_offset()` does point to a valid row in this page
            //   as the caller promised that `row_ptr` points to a valid row in `self`.
            //
            // - `fixed_row_size` is consistent with the size in bytes of the fixed part of the row.
            //   The size is also conistent with `var_len_visitor`.
            unsafe { page.delete_row(row_ptr.page_offset(), fixed_row_size, var_len_visitor, blob_store) }
        })
    }

    pub fn is_resident(&self, index: PageIndex) -> bool {
        self.slots[index.idx()].is_resident()
    }

    /// Find a page with sufficient available space to store a row of size `fixed_row_size`
    /// containing `num_var_len_granules` granules of var-len data.
    ///
    /// Retrieving a page in this way will remove it from the non-full set.
    /// After performing an insertion, the caller should use [`Self::record_page_non_full`]
    /// to restore the page to the non-full set.
    fn find_page_with_space_for_row(
        &mut self,
        fixed_row_size: Size,
        num_var_len_granules: usize,
    ) -> Result<PageIndex, PageError> {
        if let Some((page_num_free_granules, page_idx)) = self
            .non_full_pages
            .range((num_var_len_granules, PageIndex(0))..)
            .copied()
            .find(|(_, page_index)| {
                self.slots[page_index.idx()]
                    .has_space_for_row(fixed_row_size, num_var_len_granules)
                    .expect("page in `self.non_full_pages` to be present in `self.pages`")
            })
        {
            self.non_full_pages.remove(&(page_num_free_granules, page_idx));
            return Ok(page_idx);
        }

        self.allocate_new_page(fixed_row_size)
    }

    /// Allocates one additional page,
    /// returning an error if the new number of pages would overflow `PageIndex::MAX`.
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

    pub fn iter_present_pages(&self) -> impl Iterator<Item = &PageSlotHandle> {
        self.slots.iter().filter(|slot| !slot.is_absent())
    }

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

    pub fn prepare_commit(
        &self,
        fixed_row_size: Size,
        visitor: &impl VarLenMembers,
        deletes: impl IntoIterator<Item = RowPointer>,
        inserts: impl IntoIterator<Item = Result<(ProductValue, usize), PageError>>,
    ) -> Result<PreparedCommit, PageError> {
        let mut allocator = PageAllocator::new(fixed_row_size, &self.slots);
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
                    Ok(pinned[&page_index].read().capacity(fixed_row_size))
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
                            Ok(page.read().capacity(fixed_row_size))
                        }
                        btree_map::Entry::Occupied(entry) => Ok(entry.get().read().capacity(fixed_row_size)),
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

// TODO
impl MemoryUsage for PageSet {}

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
    pub fn apply(mut self, table: &mut Table, blob_store: &mut dyn BlobStore) -> AppliedCommit {
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
                .delete(blob_store, row_ptr, |row| row.to_product_value())
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
                    .insert_with_reservation(blob_store, &row, reservation)
                    .map(|(_, row_ref)| row_ref)
                    .expect("failed to insert during transaction commit");
                let (page, _) = row_ref.page_and_offset();
                let inserted_index = row_ref.pointer().page_index();
                pinned.entry(inserted_index).or_insert_with(|| page.clone());
                assert_eq!(inserted_index, page_index, "planned and actual placement differ");
            }

            row
        });

        drop(pinned);

        AppliedCommit { deletes, inserts }
    }
}

pub struct AppliedCommit {
    deletes: Arc<[ProductValue]>,
    inserts: Arc<[ProductValue]>,
}

impl AppliedCommit {
    pub fn into_parts(self) -> (Arc<[ProductValue]>, Arc<[ProductValue]>) {
        (self.deletes, self.inserts)
    }
}

pub struct PlannedInsert {
    page_index: PageIndex,
    row: ProductValue,
}

enum PlannedPage {
    /// An existing page affected by a transaction.
    Existing(PageCapacity),
    /// A page that needs to be allocated.
    Allocate(PageCapacity),
}

impl PlannedPage {
    fn capacity(&self) -> &PageCapacity {
        match self {
            Self::Existing(capacity) | Self::Allocate(capacity) => capacity,
        }
    }

    fn capacity_mut(&mut self) -> &mut PageCapacity {
        match self {
            Self::Existing(capacity) | Self::Allocate(capacity) => capacity,
        }
    }
}

enum Slot {
    /// A page slot that has been allocated and then freed.
    Absent,
    /// A page slot that is present but not affected by the transaction.
    Present(PageMetadata),
    /// A page slot that is affected by the transaction.
    Planned(PlannedPage),
}

impl Slot {
    fn has_space_for_row(&self, fixed_row_size: Size, num_granules: usize) -> bool {
        match self {
            Slot::Absent => false,
            Slot::Present(metadata) => metadata.has_space_for_row(fixed_row_size, num_granules),
            Slot::Planned(planned) => planned.capacity().has_space_for_row(fixed_row_size, num_granules),
        }
    }

    fn is_full(&self, fixed_row_size: Size) -> bool {
        !self.has_space_for_row(fixed_row_size, 0)
    }

    fn available_var_len_granules(&self) -> Option<usize> {
        match self {
            Slot::Absent => None,
            Slot::Present(metadata) => Some(metadata.available_var_len_granules()),
            Slot::Planned(planned) => Some(planned.capacity().available_var_len_granules()),
        }
    }
}

struct PageAllocator {
    fixed_row_size: Size,
    slots: Vec<Slot>,
    non_full_pages: BTreeSet<(usize, PageIndex)>,
    free_page_slots: BTreeSet<PageIndex>,
}

impl PageAllocator {
    pub fn new(fixed_row_size: Size, pages: &[PageSlotHandle]) -> Self {
        let mut slots = Vec::with_capacity(pages.len());
        let mut non_full_pages = BTreeSet::new();
        let mut free_page_slots = BTreeSet::new();

        for (page_index, handle) in pages.iter().enumerate() {
            let page_index = PageIndex(page_index as _);
            match handle.metadata(fixed_row_size) {
                None => {
                    slots.push(Slot::Absent);
                    free_page_slots.insert(page_index);
                }
                Some(metadata) => {
                    slots.push(Slot::Present(metadata));
                    let page = &slots[page_index.idx()];
                    if !page.is_full(fixed_row_size)
                        && let Some(available_granules) = page.available_var_len_granules()
                    {
                        non_full_pages.insert((available_granules, page_index));
                    }
                }
            }
        }

        Self {
            fixed_row_size,
            slots,
            non_full_pages,
            free_page_slots,
        }
    }

    pub fn prepare_delete<E>(
        &mut self,
        page_index: PageIndex,
        row_granules: usize,
        fault: impl FnMut(PageIndex) -> Result<PageCapacity, E>,
    ) -> Result<(), E> {
        self.remove_from_non_full(page_index);

        let capacity = self.ensure_planned(page_index, fault)?;
        capacity.release_row(row_granules);

        if capacity.num_rows == 0 {
            self.slots[page_index.idx()] = Slot::Absent;
            self.free_page_slots.insert(page_index);
        } else {
            self.add_to_non_full(page_index);
        }

        Ok(())
    }

    pub fn prepare_insert<E>(
        &mut self,
        row_granules: usize,
        fault: impl FnMut(PageIndex) -> Result<PageCapacity, E>,
    ) -> Result<PageIndex, E> {
        let existing = self
            .non_full_pages
            .range((row_granules, PageIndex(0))..)
            .copied()
            .find(|&(_, index)| self.slots[index.idx()].has_space_for_row(self.fixed_row_size, row_granules));

        match existing {
            Some((available_granules, page_index)) => {
                assert!(self.non_full_pages.remove(&(available_granules, page_index)));

                let fixed_row_size = self.fixed_row_size;
                let capacity = self.ensure_planned(page_index, fault)?;
                capacity.reserve_row(fixed_row_size, row_granules);
                self.add_to_non_full(page_index);

                Ok(page_index)
            }
            None => {
                // No existing page fits. Allocate at a free slot or a new slot.
                let page_index = if let Some(page_index) = self.free_page_slots.pop_first() {
                    page_index
                } else {
                    let page_index = PageIndex(self.slots.len() as _);
                    self.slots.push(Slot::Absent);
                    page_index
                };
                assert!(matches!(self.slots[page_index.idx()], Slot::Absent));

                let mut capacity = PageCapacity::empty(self.fixed_row_size);
                capacity.reserve_row(self.fixed_row_size, row_granules);

                self.slots[page_index.idx()] = Slot::Planned(PlannedPage::Allocate(capacity));
                self.add_to_non_full(page_index);

                Ok(page_index)
            }
        }
    }

    pub fn pages_to_allocate(&self) -> impl Iterator<Item = PageIndex> {
        self.slots.iter().enumerate().filter_map(|(idx, slot)| match slot {
            Slot::Absent | Slot::Present(_) | Slot::Planned(PlannedPage::Existing(_)) => None,
            Slot::Planned(PlannedPage::Allocate(_)) => Some(PageIndex(idx as _)),
        })
    }

    fn ensure_planned<E>(
        &mut self,
        page_index: PageIndex,
        mut fault: impl FnMut(PageIndex) -> Result<PageCapacity, E>,
    ) -> Result<&mut PageCapacity, E> {
        let slot = &mut self.slots[page_index.idx()];
        match slot {
            Slot::Absent => unreachable!("attempt to fault an absent page"),
            Slot::Present(_metadata) => {
                let page = fault(page_index).map(PlannedPage::Existing)?;
                *slot = Slot::Planned(page);

                let Slot::Planned(planned) = slot else { unreachable!() };
                Ok(planned.capacity_mut())
            }
            Slot::Planned(planned) => Ok(planned.capacity_mut()),
        }
    }

    fn remove_from_non_full(&mut self, page_index: PageIndex) {
        if let Some(available_granules) = self.slots[page_index.idx()].available_var_len_granules() {
            self.non_full_pages.remove(&(available_granules, page_index));
        }
    }

    fn add_to_non_full(&mut self, page_index: PageIndex) {
        let page = &self.slots[page_index.idx()];
        if !page.is_full(self.fixed_row_size)
            && let Some(available_granules) = page.available_var_len_granules()
        {
            self.non_full_pages.insert((available_granules, page_index));
        }
    }
}
