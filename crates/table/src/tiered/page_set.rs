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

pub struct PageSet {
    eviction_policy: PageEvictionPolicy,
    slots: Vec<PageSlotHandle>,
    free_page_slots: BTreeSet<PageIndex>,
    non_full_pages: BTreeSet<(usize, PageIndex)>,
    manager: Arc<PageManager>,
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

    #[allow(unused)]
    pub(crate) fn register(&mut self, index: PageIndex, reservation: ReservedPage) -> PageHandle {
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
}
