use std::{collections::BTreeSet, sync::Arc};

use spacetimedb_sats::layout::Size;

use crate::{
    blob_store::BlobStore,
    indexes::{PageIndex, RowPointer},
    page::Page,
    table::BlobNumBytes,
    tiered::page_manager::{PageEvictionPolicy, PageHandle, PageManager, PageSlotHandle, ReservedPage},
    var_len::VarLenMembers,
};

pub use crate::tiered::page_manager::PageError;

/// The set of pages of a [Table].
#[derive(Debug)]
pub struct PageSet {
    pub eviction_policy: PageEvictionPolicy,
    fixed_row_size: Size,
    slots: Vec<PageSlotHandle>,
    free_page_slots: BTreeSet<PageIndex>,
    non_full_pages: BTreeSet<(usize, PageIndex)>,
    manager: Arc<PageManager>,
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
    #[allow(unused)]
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
        blob_store: &mut dyn BlobStore,
    ) -> Result<BlobNumBytes, PageError> {
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
                    unsafe { page.delete_row(row_ptr.page_offset(), fixed_row_size, var_len_visitor, blob_store) };

                (
                    blob_bytes_deleted,
                    page.num_rows() == 0,
                    page.available_var_len_granules(),
                )
            })?;

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
}
