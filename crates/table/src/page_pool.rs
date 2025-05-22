use super::{indexes::Size, page::Page};
use crate::indexes::max_rows_in_page;
use crate::{page::PageHeader, MemoryUsage};
use core::sync::atomic::{AtomicUsize, Ordering};
use crossbeam_queue::ArrayQueue;
use spacetimedb_sats::bsatn::{self, DecodeError};
use spacetimedb_sats::de::{
    DeserializeSeed, Deserializer, Error, NamedProductAccess, ProductVisitor, SeqProductAccess,
};
use std::sync::Arc;

/// A page pool of currently unused pages available for use in [`Pages`](super::pages::Pages).
#[derive(Clone)]
pub struct PagePool {
    inner: Arc<PagePoolInner>,
}

impl MemoryUsage for PagePool {
    fn heap_usage(&self) -> usize {
        let Self { inner } = self;
        inner.heap_usage()
    }
}

impl PagePool {
    pub fn new_for_test() -> Self {
        Self::new(Some(100 * size_of::<Page>()))
    }

    /// Returns a new page pool with `max_size` bytes rounded down to the nearest multiple of 64 KiB.
    ///
    /// if no size is provided, a default of 1 page is used.
    pub fn new(max_size: Option<usize>) -> Self {
        const PAGE_SIZE: usize = size_of::<Page>();
        // TODO(centril): Currently, we have a test `test_index_scans`.
        // The test sets up a `Location` table, like in BitCraft, with a `chunk` field,
        // and populates it with 1000 different chunks with 1200 rows each.
        // Then it asserts that the cold latency of an index scan on `chunk` takes < 1 ms.
        // However, for reasons currently unknown to us,
        // a large page pool, with capacity `1 << 26` bytes, on i7-7700K, 64GB RAM,
        // will turn the latency into 30-40 ms.
        // As a precaution, we use a smaller page pool by default.
        const DEFAULT_MAX_SIZE: usize = 128 * PAGE_SIZE; // 128 pages

        let queue_size = max_size.unwrap_or(DEFAULT_MAX_SIZE) / PAGE_SIZE;
        let inner = Arc::new(PagePoolInner::new(queue_size));
        Self { inner }
    }

    /// Puts back a [`Page`] into the pool.
    pub fn put(&self, page: Box<Page>) {
        self.inner.put(page);
    }

    /// Puts back a [`Page`] into the pool.
    pub fn put_many(&self, pages: impl Iterator<Item = Box<Page>>) {
        for page in pages {
            self.put(page);
        }
    }

    /// Takes a [`Page`] from the pool or creates a new one.
    ///
    /// The returned page supports fixed rows of size `fixed_row_size`.
    pub fn take_with_fixed_row_size(&self, fixed_row_size: Size) -> Box<Page> {
        self.inner.take_with_fixed_row_size(fixed_row_size)
    }

    /// Takes a [`Page`] from the pool or creates a new one.
    ///
    /// The returned page supports a maximum of `max_rows_in_page` rows.
    fn take_with_max_row_count(&self, max_rows_in_page: usize) -> Box<Page> {
        self.inner.take_with_max_row_count(max_rows_in_page)
    }

    /// Deserialize a page from `buf` but reuse the allocations in the pool.
    pub fn take_deserialize_from(&self, buf: &[u8]) -> Result<Box<Page>, DecodeError> {
        self.deserialize(bsatn::Deserializer::new(&mut &*buf))
    }

    /// Returns the number of pages dropped by the pool because the pool was at capacity.
    pub fn dropped_pages_count(&self) -> usize {
        self.inner.dropped_pages_count.load(Ordering::Relaxed)
    }

    /// Returns the number of fresh pages allocated through the pool.
    pub fn new_pages_allocated_count(&self) -> usize {
        self.inner.new_pages_allocated_count.load(Ordering::Relaxed)
    }

    /// Returns the number of pages reused from the pool.
    pub fn pages_reused_count(&self) -> usize {
        self.inner.pages_reused_count.load(Ordering::Relaxed)
    }

    /// Returns the number of pages returned to the pool.
    pub fn pages_returned_count(&self) -> usize {
        self.inner.pages_returned_count.load(Ordering::Relaxed)
    }
}

impl<'de> DeserializeSeed<'de> for &PagePool {
    type Output = Box<Page>;

    fn deserialize<D: Deserializer<'de>>(self, de: D) -> Result<Self::Output, D::Error> {
        de.deserialize_product(self)
    }
}

impl<'de> ProductVisitor<'de> for &PagePool {
    type Output = Box<Page>;

    fn product_name(&self) -> Option<&str> {
        Some("Page")
    }

    fn product_len(&self) -> usize {
        2
    }

    fn visit_seq_product<A: SeqProductAccess<'de>>(self, mut prod: A) -> Result<Self::Output, A::Error> {
        let header = prod
            .next_element::<PageHeader>()?
            .ok_or_else(|| A::Error::invalid_product_length(2, &self))?;
        let row_data = prod
            .next_element()?
            .ok_or_else(|| A::Error::invalid_product_length(2, &self))?;

        // TODO(perf, centril): reuse the allocation of `present_rows` in `page`.
        let mut page = self.take_with_max_row_count(header.max_rows_in_page());
        // SAFETY: `header` and `row_data` are consistent with each other.
        unsafe { page.set_raw(header, row_data) };

        Ok(page)
    }

    fn visit_named_product<A: NamedProductAccess<'de>>(self, _: A) -> Result<Self::Output, A::Error> {
        unreachable!()
    }
}

/// The inner actual page pool containing all the logic.
struct PagePoolInner {
    pages: ArrayQueue<Box<Page>>,
    dropped_pages_count: AtomicUsize,
    new_pages_allocated_count: AtomicUsize,
    pages_reused_count: AtomicUsize,
    pages_returned_count: AtomicUsize,
}

impl MemoryUsage for PagePoolInner {
    fn heap_usage(&self) -> usize {
        let Self {
            pages,
            dropped_pages_count,
            new_pages_allocated_count,
            pages_reused_count,
            pages_returned_count,
        } = self;
        dropped_pages_count.heap_usage() +
        new_pages_allocated_count.heap_usage() +
        pages_reused_count.heap_usage() +
        pages_returned_count.heap_usage() +
        // This is the amount the queue itself takes up on the heap.
        pages.capacity() * size_of::<(AtomicUsize, Box<Page>)>() +
        // Each page takes up a fixed amount.
        pages.len() * size_of::<Page>()
    }
}

#[inline]
fn inc(atomic: &AtomicUsize) {
    atomic.fetch_add(1, Ordering::Relaxed);
}

impl PagePoolInner {
    /// Creates a new page pool capable of holding `cap` pages.
    fn new(cap: usize) -> Self {
        let pages = ArrayQueue::new(cap);
        Self {
            pages,
            dropped_pages_count: <_>::default(),
            new_pages_allocated_count: <_>::default(),
            pages_reused_count: <_>::default(),
            pages_returned_count: <_>::default(),
        }
    }

    /// Puts back a [`Page`] into the pool.
    fn put(&self, page: Box<Page>) {
        // Add it to the pool if there's room, or just drop it.
        if self.pages.push(page).is_ok() {
            inc(&self.pages_returned_count);
        } else {
            inc(&self.dropped_pages_count);
        }
    }

    /// Takes a [`Page`] from the pool or creates a new one.
    ///
    /// The returned page supports a maximum of `max_rows_in_page` rows.
    fn take_with_max_row_count(&self, max_rows_in_page: usize) -> Box<Page> {
        self.pages
            .pop()
            .map(|mut page| {
                inc(&self.pages_reused_count);
                page.reset_for(max_rows_in_page);
                page
            })
            .unwrap_or_else(|| {
                inc(&self.new_pages_allocated_count);
                Page::new_with_max_row_count(max_rows_in_page)
            })
    }

    /// Takes a [`Page`] from the pool or creates a new one.
    ///
    /// The returned page supports fixed rows of size `fixed_row_size`.
    fn take_with_fixed_row_size(&self, fixed_row_size: Size) -> Box<Page> {
        self.take_with_max_row_count(max_rows_in_page(fixed_row_size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{iter, ptr::addr_eq};

    fn present_rows_ptr(page: &Page) -> *const () {
        page.page_header_for_test().present_rows_storage_ptr_for_test()
    }

    fn assert_metrics(pool: &PagePool, dropped: usize, new: usize, reused: usize, returned: usize) {
        assert_eq!(pool.dropped_pages_count(), dropped);
        assert_eq!(pool.new_pages_allocated_count(), new);
        assert_eq!(pool.pages_reused_count(), reused);
        assert_eq!(pool.pages_returned_count(), returned);
    }

    #[test]
    fn page_pool_returns_same_page() {
        let pool = PagePool::new_for_test();
        assert_metrics(&pool, 0, 0, 0, 0);

        // Create a page and put it back.
        let page1 = pool.take_with_max_row_count(10);
        assert_metrics(&pool, 0, 1, 0, 0);
        let page1_ptr = &*page1 as *const _;
        let page1_pr_ptr = present_rows_ptr(&page1);
        pool.put(page1);
        assert_metrics(&pool, 0, 1, 0, 1);

        // Extract a page again.
        let page2 = pool.take_with_max_row_count(64);
        assert_metrics(&pool, 0, 1, 1, 1);
        let page2_ptr = &*page2 as *const _;
        let page2_pr_ptr = present_rows_ptr(&page2);
        // It should be the same as the previous one.
        assert!(addr_eq(page1_ptr, page2_ptr));
        // And the bitset should also be the same, as `10.div_ceil(64) == 64`.
        assert!(addr_eq(page1_pr_ptr, page2_pr_ptr));
        pool.put(page2);
        assert_metrics(&pool, 0, 1, 1, 2);

        // Extract a page again, but this time, go beyond the first block.
        let page3 = pool.take_with_max_row_count(64 + 1);
        assert_metrics(&pool, 0, 1, 2, 2);
        let page3_ptr = &*page3 as *const _;
        let page3_pr_ptr = present_rows_ptr(&page3);
        // It should be the same as the previous one.
        assert!(addr_eq(page1_ptr, page3_ptr));
        // But the bitset should not be the same, as `65.div_ceil(64) == 2`.
        assert!(!addr_eq(page1_pr_ptr, page3_pr_ptr));

        // Manually create a page and put it in.
        let page4 = Page::new_with_max_row_count(10);
        let page4_ptr = &*page4 as *const _;
        pool.put(page4);
        pool.put(page3);
        assert_metrics(&pool, 0, 1, 2, 4);
        // When we take out a page, it should be the same as `page4` and not `page1`.
        let page5 = pool.take_with_max_row_count(10);
        assert_metrics(&pool, 0, 1, 3, 4);
        let page5_ptr = &*page5 as *const _;
        // Same as page4.
        assert!(!addr_eq(page5_ptr, page1_ptr));
        assert!(addr_eq(page5_ptr, page4_ptr));
    }

    #[test]
    fn page_pool_drops_past_max_size() {
        const N: usize = 3;
        let pool = PagePool::new(Some(size_of::<Page>() * N));

        let pages = iter::repeat_with(|| pool.take_with_max_row_count(42))
            .take(N + 1)
            .collect::<Vec<_>>();
        assert_metrics(&pool, 0, N + 1, 0, 0);

        pool.put_many(pages.into_iter());
        assert_metrics(&pool, 1, N + 1, 0, N);
        assert_eq!(pool.inner.pages.len(), N);
    }
}
