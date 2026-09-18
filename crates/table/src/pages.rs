//! Provides [`Pages`], a page manager dealing with [`Page`]s as a collection.

use super::blob_store::BlobStore;
use super::indexes::{Bytes, PageIndex, PageOffset, RowPointer};
use super::page::Page;
use super::page_pool::PagePool;
use super::table::BlobNumBytes;
use super::var_len::VarLenMembers;
use core::ops::Deref;
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

    /// Free the page at `page_index`, leaving `self.pages[page_index.idx()]` as `None`.
    ///
    /// Prior to calling this method, `self.pages[page_index.idx()]` must be:
    /// - Present, i.e. `Some` and not `None`.
    /// - Empty, i.e. have `page.num_rows() == 0`.
    ///
    /// Prior to calling this method, the page's entry should already have been deleted from `self.non_full_pages`.
    ///
    /// Calling when in violation of these invariants may result in a panic and/or unexpected behavior.
    /// If a non-empty page is freed,
    /// future `unsafe` operations which attempt to read from rows that were previously in that page
    /// may result in Undefined Behavior.
    ///
    /// The freed page will be returned to the `pool`.
    fn free_empty_page(&mut self, pool: &PagePool, page_index: PageIndex) {
        let page = self.get(page_index).expect("page to free to have been present");

        debug_assert_eq!(page.num_rows(), 0);

        // The page should already have been removed from `self.non_full_pages`.
        debug_assert!({
            let free_granules = page.available_var_len_granules();

            !self.non_full_pages.remove(&(free_granules, page_index))
        });

        let page = self.pages[page_index.idx()]
            .take()
            .expect("freed page to have been present after we already checked its presence");

        pool.put(page);

        let newly_inserted_into_free_pages_set = self.free_page_slots.insert(page_index);

        debug_assert!(newly_inserted_into_free_pages_set)
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
        page_pool: &PagePool,
        blob_store: &mut dyn BlobStore,
    ) -> BlobNumBytes {
        let page_index = row_ptr.page_index();

        self.with_updating_non_full_pages_and_maybe_freeing_page(page_pool, page_index, fixed_row_size, |this| {
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
    /// Free the page and return it to the `pool` if it is empty after the `body`.
    ///
    /// `body` should not update any pages other than the one identified by `page_index`.
    ///
    /// If `page_index` does not refer to a present page, i.e. `self.pages[page_index.idx]` is `None`,
    /// this method will panic.
    fn with_updating_non_full_pages_and_maybe_freeing_page<Ret>(
        &mut self,
        pool: &PagePool,
        page_index: PageIndex,
        fixed_row_size: Size,
        body: impl FnOnce(&mut Self) -> Ret,
    ) -> Ret {
        let page = self
            .get(page_index)
            .expect("page to be present in `with_updating_non_full_pages_and_maybe_freeing_page`");

        let full_before = page.is_full(fixed_row_size);
        let available_granules_before = page.available_var_len_granules();

        let ret = body(self);

        let page = self
            .get(page_index)
            .expect("page to remain present in `with_updating_non_full_pages_and_maybe_freeing_page` after it was checked earlier");

        if page.is_empty() {
            self.remove_non_full_marker(available_granules_before, full_before, page_index);
            self.free_empty_page(pool, page_index);
        } else {
            self.remove_non_full_marker(available_granules_before, full_before, page_index);
            self.record_page_non_full(page_index, fixed_row_size);
        }

        ret
    }

    /// Remove a page's pre-existing entry in `self.non_full_pages`.
    fn remove_non_full_marker(&mut self, available_granules_before: usize, full_before: bool, page_index: PageIndex) {
        if full_before {
            debug_assert!(!self.non_full_pages.remove(&(available_granules_before, page_index)));
        } else {
            let _prev = self.non_full_pages.remove(&(available_granules_before, page_index));
            debug_assert!(_prev);
        }
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
