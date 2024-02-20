//! Provides [`Pages`], a page manager dealing with [`Page`]s as a collection.

use super::blob_store::BlobStore;
use super::indexes::{Bytes, PageIndex, PageOffset, RowPointer, Size};
use super::page::Page;
use super::var_len::VarLenMembers;
use core::ops::{ControlFlow, Deref, Index, IndexMut};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Attempt to allocate more than {} pages.", PageIndex::MAX.idx())]
    TooManyPages,
    #[error(transparent)]
    Page(#[from] super::page::Error),
}

impl Index<PageIndex> for Pages {
    type Output = Page;

    fn index(&self, pi: PageIndex) -> &Self::Output {
        &self.pages[pi.idx()]
    }
}

impl IndexMut<PageIndex> for Pages {
    fn index_mut(&mut self, pi: PageIndex) -> &mut Self::Output {
        &mut self.pages[pi.idx()]
    }
}

/// A manager of [`Page`]s.
#[derive(Default)]
pub struct Pages {
    /// The collection of pages under management.
    pages: Vec<Box<Page>>,
    /// The set of pages that aren't yet full.
    non_full_pages: Vec<PageIndex>,
}

impl Pages {
    /// Is there space to allocate another page?
    pub fn can_allocate_new_page(&self) -> Result<PageIndex, Error> {
        let new_idx = self.len();
        if new_idx <= PageIndex::MAX.idx() {
            Ok(PageIndex(new_idx as _))
        } else {
            Err(Error::TooManyPages)
        }
    }

    /// Get a mutable reference to a `Page`.
    ///
    /// Used in benchmarks. Internal operators will prefer directly indexing into `self.pages`,
    /// as that allows split borrows.
    #[doc(hidden)] // Used in benchmarks.
    pub fn get_page_mut(&mut self, page: PageIndex) -> &mut Page {
        &mut self.pages[page.idx()]
    }

    /// Make all pages within `self` clear,
    /// deallocating all rows.
    #[doc(hidden)] // Used in benchmarks.
    pub fn clear(&mut self) {
        // Clear every page.
        for page in &mut self.pages {
            page.clear();
        }
        // Mark every page non-full.
        self.non_full_pages = (0..self.pages.len()).map(|idx| PageIndex(idx as u64)).collect();
    }

    /// Get a reference to fixed-len row data.
    ///
    /// Used in benchmarks.
    /// Higher-level code paths are expected to go through [`super::de::read_row_from_pages`].
    #[doc(hidden)] // Used in benchmarks.
    pub fn get_fixed_len_row(&self, row: RowPointer, fixed_row_size: Size) -> &Bytes {
        self[row.page_index()].get_row_data(row.page_offset(), fixed_row_size)
    }

    /// Allocates one additional page,
    /// returning an error if the new number of pages would overflow `PageIndex::MAX`.
    ///
    /// The new page is initially empty, but is not added to the non-full set.
    /// Callers should call [`Pages::maybe_mark_page_non_full`] after operating on the new page.
    fn allocate_new_page(&mut self, fixed_row_size: Size) -> Result<PageIndex, Error> {
        let new_idx = self.can_allocate_new_page()?;

        self.pages.push(Page::new(fixed_row_size));

        Ok(new_idx)
    }

    /// Reserve a new, initially empty page.
    pub fn reserve_empty_page(&mut self, fixed_row_size: Size) -> Result<PageIndex, Error> {
        let idx = self.allocate_new_page(fixed_row_size)?;
        self.mark_page_non_full(idx);
        Ok(idx)
    }

    /// Mark the page at `idx` as non-full.
    pub fn mark_page_non_full(&mut self, idx: PageIndex) {
        self.non_full_pages.push(idx);
    }

    /// If the page at `page_index` is not full,
    /// add it to the non-full set so that later insertions can access it.
    pub fn maybe_mark_page_non_full(&mut self, page_index: PageIndex, fixed_row_size: Size) {
        if !self[page_index].is_full(fixed_row_size) {
            self.non_full_pages.push(page_index);
        }
    }

    /// Call `f` with a reference to a page which satisfies
    /// `page.has_space_for_row(fixed_row_size, num_var_len_granules)`.
    pub fn with_page_to_insert_row<Res>(
        &mut self,
        fixed_row_size: Size,
        num_var_len_granules: usize,
        f: impl FnOnce(&mut Page) -> Res,
    ) -> Result<(PageIndex, Res), Error> {
        let page_index = self.find_page_with_space_for_row(fixed_row_size, num_var_len_granules)?;
        let res = f(&mut self[page_index]);
        self.maybe_mark_page_non_full(page_index, fixed_row_size);
        Ok((page_index, res))
    }

    /// Find a page with sufficient available space to store a row of size `fixed_row_size`
    /// containing `num_var_len_granules` granules of var-len data.
    ///
    /// Retrieving a page in this way will remove it from the non-full set.
    /// After performing an insertion, the caller should use [`Pages::maybe_mark_page_non_full`]
    /// to restore the page to the non-full set.
    fn find_page_with_space_for_row(
        &mut self,
        fixed_row_size: Size,
        num_var_len_granules: usize,
    ) -> Result<PageIndex, Error> {
        if let Some((page_idx_idx, page_idx)) = self
            .non_full_pages
            .iter()
            .copied()
            .enumerate()
            .find(|(_, page_idx)| self[*page_idx].has_space_for_row(fixed_row_size, num_var_len_granules))
        {
            self.non_full_pages.swap_remove(page_idx_idx);
            return Ok(page_idx);
        }

        self.allocate_new_page(fixed_row_size)
    }

    /// Superseded by `write_av_to_pages`, but exposed for benchmarking
    /// when we want to avoid the overhead of traversing `AlgebraicType`.
    ///
    /// Inserts a row with fixed parts in `fixed_len` and variable parts in `var_len`.
    /// The `fixed_len.len()` is equal to `fixed_row_size`.
    ///
    /// # Safety
    ///
    /// - `var_len_visitor` must be suitable for visiting var-len refs in `fixed_row`.
    /// - `fixed_row.len()` matches the row type size exactly.
    /// - `fixed_row.len()` is consistent
    ///    with what has been passed to the manager in all other ops
    ///    and must be consistent with the `var_len_visitor` the manager was made with.
    // TODO(bikeshedding): rename to make purpose as bench interface clear?
    pub unsafe fn insert_row(
        &mut self,
        var_len_visitor: &impl VarLenMembers,
        fixed_row_size: Size,
        fixed_len: &Bytes,
        var_len: &[&[u8]],
        blob_store: &mut dyn BlobStore,
    ) -> Result<(PageIndex, PageOffset), Error> {
        debug_assert!(fixed_len.len() == fixed_row_size.len());

        match self.with_page_to_insert_row(
            fixed_row_size,
            Page::total_granules_required_for_objects(var_len),
            |page| {
                // This insertion can never fail, as we know that the page has sufficient space from `find_page_with_space_for_row`.
                //
                // SAFETY:
                // - Caller promised that `var_len_visitor`
                //   is suitable for visiting var-len refs in `fixed_row`
                //   and that `fixed_row.len()` matches the row type size exactly.
                //
                // - Caller promised that `fixed_row.len()` is consistent
                //   with what has been passed to the manager in all other ops.
                //   This entails that `fixed_row.len()` is consistent with `page`.
                unsafe { page.insert_row(fixed_len, var_len, var_len_visitor, blob_store) }
            },
        )? {
            (page, Ok(offset)) => Ok((page, offset)),
            (_, Err(e)) => Err(e.into()),
        }
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
    ) {
        let page = &mut self[row_ptr.page_index()];
        let full_before = page.is_full(fixed_row_size);
        // SAFETY:
        // - `row_ptr.page_offset()` does point to a valid row in this page
        //   as the caller promised that `row_ptr` points to a valid row in `self`.
        //
        // - `fixed_row_size` is consistent with the size in bytes of the fixed part of the row.
        //   The size is also conistent with `var_len_visitor`.
        unsafe {
            page.delete_row(row_ptr.page_offset(), fixed_row_size, var_len_visitor, blob_store);
        }

        // If the page was previously full, mark it as non-full now,
        // since we just opened a space in it.
        if full_before {
            self.mark_page_non_full(row_ptr.page_index());
        }
    }

    /// Materialize a view of rows in `self` for which the  `filter` returns `true`.
    ///
    /// # Safety
    ///
    /// - The `var_len_visitor` will visit the same set of `VarLenRef`s in the row
    ///   as the visitor provided to all other methods on `self`.
    ///
    /// - The `fixed_row_size` is consistent with the `var_len_visitor`
    ///   and is equal to the value provided to all other methods on `self`.
    pub unsafe fn copy_filter(
        &self,
        var_len_visitor: &impl VarLenMembers,
        fixed_row_size: Size,
        blob_store: &mut dyn BlobStore,
        mut filter: impl FnMut(&Page, PageOffset) -> bool,
    ) -> Self {
        // Build a new container to hold the materialized view.
        // Push pages into it later.
        let mut partial_copied_pages = Self::default();

        // A destination page that was not filled entirely,
        // or `None` if it's time to allocate a new destination page.
        let mut partial_page = None;

        // Copy each page.
        for from_page in &self.pages {
            // You may require multiple calls to `Page::copy_starting_from`
            // if `partial_page` fills up;
            // the first call starts from 0.
            let mut copy_starting_from = Some(PageOffset(0));

            // While there are unprocessed rows in `from_page`,
            while let Some(next_offset) = copy_starting_from.take() {
                // Grab the `partial_page` or allocate a new one.
                let mut to_page = partial_page.take().unwrap_or_else(|| Page::new(fixed_row_size));

                // Copy as many rows as will fit in `to_page`.
                //
                // SAFETY:
                //
                // - The `var_len_visitor` will visit the same set of `VarLenRef`s in the row
                //   as the visitor provided to all other methods on `self`.
                //   The `to_page` uses the same visitor as the `from_page`.
                //
                // - The `fixed_row_size` is consistent with the `var_len_visitor`
                //   and is equal to the value provided to all other methods on `self`,
                //   as promised by the caller.
                //   The newly made `to_page` uses the same `fixed_row_size` as the `from_page`.
                //
                // - The `next_offset` is either 0,
                //   which is always a valid starting offset for any row size,
                //   or it came from `copy_filter_into` in a previous iteration,
                //   which, given that `fixed_row_size` was valid,
                //   always returns a valid starting offset in case of `Continue(_)`.
                let cfi_ret = unsafe {
                    from_page.copy_filter_into(
                        next_offset,
                        &mut to_page,
                        fixed_row_size,
                        var_len_visitor,
                        blob_store,
                        &mut filter,
                    )
                };
                copy_starting_from = if let ControlFlow::Continue(continue_point) = cfi_ret {
                    // If `to_page` couldn't fit all of `from_page`,
                    // repeat the `while_let` loop to copy the rest.
                    Some(continue_point)
                } else {
                    // If `to_page` fit all of `from_page`, we can move on.
                    None
                };

                // If `from_page` finished copying into `to_page`, then `to_page` may have extra room.
                //
                // If `copy_filtered_into` returns `Some`,
                // that means at least one row didn't have space in `to_page`,
                // so we must consider `to_page` full.
                //
                // Note that this is distinct from `Page::is_full`,
                // as that method considers the optimistic case of a row with no var-len members.
                if copy_starting_from.is_none() {
                    partial_page = Some(to_page);
                } else {
                    partial_copied_pages.pages.push(to_page);
                }
            }
        }

        partial_copied_pages
    }
}

impl Deref for Pages {
    type Target = [Box<Page>];

    fn deref(&self) -> &Self::Target {
        &self.pages
    }
}
