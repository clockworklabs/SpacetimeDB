//! Provides [`Pages`], a page manager dealing with [`Page`]s as a collection.

use super::blob_store::{BlobHash, BlobStore};
use super::indexes::{Bytes, PageIndex, PageOffset, RowPointer};
use super::page::Page;
use super::page_pool::PagePool;
use super::table::BlobNumBytes;
use super::var_len::VarLenMembers;
use core::ops::{ControlFlow, Deref};
use spacetimedb_sats::layout::Size;
use spacetimedb_sats::memory_usage::MemoryUsage;
use std::collections::BTreeSet;
use std::ops::DerefMut;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum Error {
    #[error("Attempt to allocate more than {} pages.", PageIndex::MAX.idx())]
    TooManyPages,
    #[error(transparent)]
    Page(#[from] super::page::Error),
}

/// A manager of [`Page`]s.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Pages {
    /// The collection of pages under management.
    ///
    /// A `None` here is a page that was previously allocated and then became empty during [`Self::delete_row`].
    pages: Vec<Option<Box<Page>>>,
    /// Indexes into [`self.pages`](Self::pages) which hold `None`, where newly-allocated pages can be inserted.
    ///
    /// When freeing a page other than the last one, resulting in a `None` in [`self.pages`](Self::pages),
    /// we'll insert the freed page's [`PageIndex`] into this set.
    /// When allocating a new page, we'll use [`BTreeSet::pop_first`] to insert it into the lowest-available [`PageIndex`].
    ///
    /// Using a `BTreeSet` and popping the lowest item means that, over time,
    /// datastores can converge on a dense sequence of pages.
    /// This assumes that deletes are distributed randomly.
    /// Low-indexed pages will be quickly replaced, while high-indexed pages will remain vacant for longer.
    free_page_slots: BTreeSet<PageIndex>,
    /// The set of pages that aren't yet full,
    /// sorted by the number of var-len granules available in each page.
    ///
    /// Used during insertion to locate a page with enough space to store a given row.
    ///
    /// The first value in the tuple is [`Page::available_var_len_granules`], and the second value is the page index.
    ///
    /// Pages for which [`Page::is_full`] is true are not stored.
    ///
    /// If multiple pages have the same number of granules available, they are then sorted by `PageIndex`.
    /// This maintains a deterministic sort order,
    /// so that replaying the same set of operations on multiple datastores
    /// will always result in the same layout of rows in pages,
    /// regardless of when those datastores were (re)started prior to or during the sequence of operations.
    non_full_pages: BTreeSet<(usize, PageIndex)>,
}

impl MemoryUsage for Pages {
    fn heap_usage(&self) -> usize {
        let Self {
            pages,
            free_page_slots,
            non_full_pages,
        } = self;
        pages.heap_usage() + free_page_slots.heap_usage() + non_full_pages.heap_usage()
    }
}

impl Pages {
    pub fn get(&self, page_index: PageIndex) -> Option<&Page> {
        self.pages
            .get(page_index.idx())
            .and_then(|page_slot| page_slot.as_deref())
    }

    pub fn get_mut(&mut self, page_index: PageIndex) -> Option<&mut Page> {
        self.pages
            .get_mut(page_index.idx())
            .and_then(|page_slot| page_slot.as_deref_mut())
    }

    #[cfg(test)]
    pub(crate) fn assert_non_full_pages_consistent(&self, fixed_row_size: Size) {
        let mut seen_page_indexes = BTreeSet::new();
        for &(_, page_index) in &self.non_full_pages {
            assert!(
                seen_page_indexes.insert(page_index),
                "page {:?} appears multiple times in non_full_pages",
                page_index
            );
        }

        for (idx, page) in self.pages.iter().enumerate() {
            let page_index = PageIndex(idx as u64);
            if let Some(page) = page {
                let is_full = page.is_full(fixed_row_size);
                let available_granules = page.available_var_len_granules();
                let entries_for_page: Vec<_> = self
                    .non_full_pages
                    .iter()
                    .copied()
                    .filter(|&(_, idx)| idx == page_index)
                    .collect();

                if is_full {
                    assert!(
                        entries_for_page.is_empty(),
                        "page {:?} has 0 available var-len granules but appears in non_full_pages as {:?}",
                        page_index,
                        entries_for_page
                    );
                } else {
                    assert_eq!(
                        entries_for_page,
                        vec![(available_granules, page_index)],
                        "page {:?} has {} available var-len granules but non_full_pages has {:?}",
                        page_index,
                        available_granules,
                        entries_for_page
                    );
                }
            } else {
                let entries_for_page: Vec<_> = self
                    .non_full_pages
                    .iter()
                    .copied()
                    .filter(|&(_free_granules, idx)| idx == page_index)
                    .collect();
                assert!(
                    entries_for_page.is_empty(),
                    "page slot {:?} is None, but appears in non_full_pages as {:?}",
                    page_index,
                    entries_for_page,
                );
            }
        }
    }

    /// Is there space to allocate another page?
    pub fn can_allocate_new_page(&self) -> Result<PageIndex, Error> {
        let new_idx = self.len();
        if new_idx <= PageIndex::MAX.idx() {
            Ok(PageIndex(new_idx as _))
        } else {
            Err(Error::TooManyPages)
        }
    }

    /// The number of present pages in `self`, i.e. those which have been allocated and not since freed.
    pub fn num_present_pages(&self) -> usize {
        self.pages
            .len()
            .checked_sub(self.free_page_slots.len())
            .expect("pages len to be greater than number of free slots")
    }

    #[cfg(test)]
    pub fn free_empty_page(&mut self, page_index: PageIndex) {
        let page = self.get(page_index).expect("page to free to have been present");

        assert_eq!(page.num_rows(), 0);

        let free_granules = page.available_var_len_granules();

        let removed_from_non_full = self.non_full_pages.remove(&(free_granules, page_index));
        assert!(removed_from_non_full);

        self.pages[page_index.idx()] = None;

        let newly_inserted_into_free_pages_set = self.free_page_slots.insert(page_index);

        assert!(newly_inserted_into_free_pages_set)
    }

    /// Make all pages within `self` clear,
    /// deleting all rows.
    //
    // TODO(delete-free-page): Determine what to do with this method.
    // It doesn't really make sense given that it clears all pages but doesn't delete them,
    // but it's only used in benchmarks, and those benchmarks are using it specifically to bypass allocating new pages.
    #[doc(hidden)] // Used in benchmarks.
    pub fn clear(&mut self) {
        // Clear every page.
        for page in self.pages.iter_mut().flatten() {
            page.clear();
        }
        // Mark every page non-full.
        self.non_full_pages = (0..self.pages.len())
            // We could probably compute the number of available granules once and use it for all pages,
            // rather than calling the method on each page,
            // but we'd have to do some amount of reasoning to demonstrate it was correct
            // based on the definition of `Page::clear`,
            // and why bother?
            .filter_map(|idx| {
                let idx = PageIndex(idx as u64);
                self.get(idx).map(|page| (page.available_var_len_granules(), idx))
            })
            .collect();
    }

    /// Get a reference to fixed-len row data.
    ///
    /// Used in benchmarks.
    /// Higher-level code paths are expected to go through [`super::de::read_row_from_pages`].
    #[doc(hidden)] // Used in benchmarks.
    pub fn get_fixed_len_row(&self, row: RowPointer, fixed_row_size: Size) -> &Bytes {
        self.get(row.page_index())
            .expect("`get_fixed_len_row` of row in not-present page")
            .get_row_data(row.page_offset(), fixed_row_size)
    }

    /// Allocates one additional page,
    /// returning an error if the new number of pages would overflow `PageIndex::MAX`.
    ///
    /// The new page is initially empty, but is not added to the non-full set.
    /// Callers should call [`Pages::record_page_non_full`] after operating on the new page.
    fn allocate_new_page(&mut self, pool: &PagePool, fixed_row_size: Size) -> Result<PageIndex, Error> {
        if let Some(idx) = self.free_page_slots.pop_first() {
            // If `self` contains holes from previously-freed pages,
            // fill the lowest-`PageIndex` such hole.
            let page = pool.take_with_fixed_row_size(fixed_row_size);
            self.pages[idx.idx()] = Some(page);
            Ok(idx)
        } else {
            // If `self` is currently dense, try to put the new page at the end.
            let new_idx = self.can_allocate_new_page()?;

            let page = pool.take_with_fixed_row_size(fixed_row_size);
            self.pages.push(Some(page));

            Ok(new_idx)
        }
    }

    /// Reserve a new, initially empty page.
    // TODO(delete-free-page): Determine what to do with this method.
    // It doesn't really make sense in a world where `Pages` doesn't contain empty `Page`s,
    // but it's only used for tests and benches.
    pub fn reserve_empty_page(&mut self, pool: &PagePool, fixed_row_size: Size) -> Result<PageIndex, Error> {
        let idx = self.allocate_new_page(pool, fixed_row_size)?;
        self.record_page_non_full(idx, fixed_row_size);
        Ok(idx)
    }

    /// Call `f` with a reference to a page which satisfies
    /// `page.has_space_for_row(fixed_row_size, num_var_len_granules)`.
    pub fn with_page_to_insert_row<Res>(
        &mut self,
        pool: &PagePool,
        fixed_row_size: Size,
        num_var_len_granules: usize,
        f: impl FnOnce(&mut Page) -> Res,
    ) -> Result<(PageIndex, Res), Error> {
        let page_index = self.find_page_with_space_for_row(pool, fixed_row_size, num_var_len_granules)?;
        let res = f(self
            .get_mut(page_index)
            .expect("page returned by `find_page_with_space_for_row` to be present in `self.pages`"));
        self.record_page_non_full(page_index, fixed_row_size);
        Ok((page_index, res))
    }

    /// Find a page with sufficient available space to store a row of size `fixed_row_size`
    /// containing `num_var_len_granules` granules of var-len data.
    ///
    /// Retrieving a page in this way will remove it from the non-full set.
    /// After performing an insertion, the caller should use [`Pages::record_page_non_full`]
    /// to restore the page to the non-full set.
    fn find_page_with_space_for_row(
        &mut self,
        pool: &PagePool,
        fixed_row_size: Size,
        num_var_len_granules: usize,
    ) -> Result<PageIndex, Error> {
        if let Some((page_num_free_granules, page_idx)) = self
            .non_full_pages
            .range((num_var_len_granules, PageIndex(0))..)
            .copied()
            .find(|(_, page_idx)| {
                self.get(*page_idx)
                    .expect("page in `self.non_full_pages` to be present in `self.pages`")
                    .has_space_for_row(fixed_row_size, num_var_len_granules)
            })
        {
            self.non_full_pages.remove(&(page_num_free_granules, page_idx));
            return Ok(page_idx);
        }

        self.allocate_new_page(pool, fixed_row_size)
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
    ///   with what has been passed to the manager in all other ops
    ///   and must be consistent with the `var_len_visitor` the manager was made with.
    // TODO(bikeshedding): rename to make purpose as bench interface clear?
    pub unsafe fn insert_row(
        &mut self,
        pool: &PagePool,
        var_len_visitor: &impl VarLenMembers,
        fixed_row_size: Size,
        fixed_len: &Bytes,
        var_len: &[&[u8]],
        blob_store: &mut dyn BlobStore,
    ) -> Result<(PageIndex, PageOffset), Error> {
        debug_assert!(fixed_len.len() == fixed_row_size.len());

        match self.with_page_to_insert_row(
            pool,
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
    ) -> BlobNumBytes {
        let page_index = row_ptr.page_index();

        self.with_updating_non_full_pages(page_index, fixed_row_size, |this| {
            let page = this
                .get_mut(page_index)
                .expect("page containing `row_ptr` with validity safety invariant to be present");

            // SAFETY:
            // - `row_ptr.page_offset()` does point to a valid row in this page
            //   as the caller promised that `row_ptr` points to a valid row in `self`.
            //
            // - `fixed_row_size` is consistent with the size in bytes of the fixed part of the row.
            //   The size is also conistent with `var_len_visitor`.
            unsafe { page.delete_row(row_ptr.page_offset(), fixed_row_size, var_len_visitor, blob_store) }
        })
    }

    /// Collect information about the page `self[page_index]` sufficient to update [`Self::non_full_pages`],
    /// then run `body` to update the page, and finally update [`Self::non_full_pages`] for its new fullness and capacity.
    ///
    /// `body` should not update any pages other than the one identified by `page_index`.
    ///
    /// If `page_index` does not refer to a present page, i.e. `self.pages[page_index.idx]` is `None`,
    /// this method will panic.
    fn with_updating_non_full_pages<Ret>(
        &mut self,
        page_index: PageIndex,
        fixed_row_size: Size,
        body: impl FnOnce(&mut Self) -> Ret,
    ) -> Ret {
        let page = self
            .get(page_index)
            .expect("page to be present in `with_updating_non_full_pages`");

        let full_before = page.is_full(fixed_row_size);
        let available_granules_before = page.available_var_len_granules();

        let ret = body(self);

        self.update_page_non_full(available_granules_before, full_before, page_index, fixed_row_size);

        ret
    }

    /// Update [`Self::non_full_pages`] to change the number of var-len granules available in the page at `self[page_index]`,
    /// first deleting any old entry and then re-inserting the new entry.
    ///
    /// The entry for `page` in `self.non_full_granules` should not have been deleted prior to calling this method.
    /// If the entry has already been deleted or was never present, instead use [`Self::record_page_non_full`].
    ///
    /// `available_granules_before` should be the previous count from [`Page::available_var_len_granules`],
    /// prior to whatever operation made space available in the page.
    /// This is necessary because `non_full_pages` is a `BTreeSet` sorted by `(available_granules, page_index)`,
    /// so locating the `page_index` without the `available_granules` would be slow.
    ///
    /// `full_before` should be the result of [`Page::is_full`] prior to whatever operation made space available in the page.
    /// This is necessary because `non_full_pages` does not store full pages (as the name implies),
    /// so we should not attempt to delete the previous entry if the page was previously full.
    fn update_page_non_full(
        &mut self,
        available_granules_before: usize,
        full_before: bool,
        page_index: PageIndex,
        fixed_row_size: Size,
    ) {
        if full_before {
            debug_assert!(!self.non_full_pages.remove(&(available_granules_before, page_index)));
        } else {
            let _prev = self.non_full_pages.remove(&(available_granules_before, page_index));
            debug_assert!(_prev);
        }

        self.record_page_non_full(page_index, fixed_row_size);
    }

    /// Record the number of available var-len granules in the page at `self[page_index]` into [`Self::non_full_pages`].
    ///
    /// Prior to calling this function, there must not be an entry for `page_index` in [`Self::non_full_pages`].
    ///
    /// The page must still be present, that is, `self.pages[page_index.idx()]` must be `Some`.
    /// Otherwise this method will panic.
    fn record_page_non_full(&mut self, page_index: PageIndex, fixed_row_size: Size) {
        debug_assert!(!self.non_full_pages.iter().any(|(_, idx)| *idx == page_index));

        let page = self
            .get(page_index)
            .expect("page to be present when recording its fullness");
        let available_granules = page.available_var_len_granules();

        if !page.is_full(fixed_row_size) {
            self.non_full_pages.insert((available_granules, page_index));
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
    // FIXME: this method appears not to correctly set `non_full_pages` on the result.
    // It is also unused except for benchmarks, so it may be best to just remove it.
    pub unsafe fn copy_filter(
        &self,
        var_len_visitor: &impl VarLenMembers,
        fixed_row_size: Size,
        mut blob_policy: Option<&mut impl FnMut(BlobHash)>,
        mut filter: impl FnMut(&Page, PageOffset) -> bool,
    ) -> Self {
        // Build a new container to hold the materialized view.
        // Push pages into it later.
        let mut partial_copied_pages = Self::default();

        // A destination page that was not filled entirely,
        // or `None` if it's time to allocate a new destination page.
        let mut partial_page = None;

        // Copy each page.
        for from_page in self.pages.iter().filter_map(|page| page.as_deref()) {
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
                        blob_policy.as_mut(),
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
                    partial_copied_pages.pages.push(Some(to_page));
                }
            }
        }

        partial_copied_pages
    }

    /// Set this [`Pages`]' contents to be the `pages`.
    ///
    /// Used when restoring from a snapshot.
    ///
    /// Each page in the `pages` must be consistent with the schema for this [`Pages`],
    /// i.e. the schema for the [`crate::table::Table`] which contains `self`.
    ///
    /// Should only ever be called when `self.is_empty()`.
    ///
    /// Also populates `self.non_full_pages`.
    pub fn set_contents(&mut self, pages: Vec<Option<Box<Page>>>, fixed_row_size: Size) {
        debug_assert!(self.is_empty());
        self.non_full_pages = pages
            .iter()
            .enumerate()
            .filter_map(|(idx, page)| {
                page.as_ref().and_then(|page| {
                    (!page.is_full(fixed_row_size)).then_some((page.available_var_len_granules(), PageIndex(idx as _)))
                })
            })
            .collect();
        self.free_page_slots = pages
            .iter()
            .enumerate()
            .filter_map(|(idx, page)| page.is_none().then_some(PageIndex(idx as _)))
            .collect();
        self.pages = pages;
    }

    /// Consumes the page manager, returning all the pages it held.
    ///
    /// Indexes in the iterator will not necessarily correspond to the pages' `PageIndex`es.
    pub fn into_page_iter(self) -> impl Iterator<Item = Box<Page>> {
        self.pages.into_iter().flatten()
    }

    /// Iterate over only those pages in `self` that are present.
    ///
    /// Indexes in the iterator will not necessarily correspond to the pages' `PageIndex`es.
    /// `pages.iter_present_pages().enumerate()` does not yield the correct `PageIndex`es for the yielded pages.
    /// Use [`Self::iter_present_pages_with_page_index`] for a version that also yields correct `PageIndex`es.
    pub fn iter_present_pages(&self) -> impl Iterator<Item = &Page> {
        self.pages.iter().filter_map(|page| page.as_deref())
    }

    /// Iterate over only those pages in `self` that are present, paired with their `PageIndex`es.
    pub fn iter_present_pages_with_page_index(&self) -> impl Iterator<Item = (PageIndex, &Page)> {
        self.pages
            .iter()
            .enumerate()
            .filter_map(|(idx, page)| page.as_deref().map(|page| (PageIndex(idx as u64), page)))
    }
}

impl Deref for Pages {
    type Target = [Option<Box<Page>>];

    fn deref(&self) -> &Self::Target {
        &self.pages
    }
}

impl DerefMut for Pages {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pages
    }
}
