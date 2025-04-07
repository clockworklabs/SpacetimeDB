use super::{indexes::Size, page::Page};
use crate::indexes::max_rows_in_page;
use crate::{page::PageHeader, MemoryUsage};
use core::sync::atomic::AtomicUsize;
use crossbeam_queue::ArrayQueue;
use spacetimedb_sats::bsatn::{self, DecodeError};
use spacetimedb_sats::de::{
    DeserializeSeed, Deserializer, Error, NamedProductAccess, ProductVisitor, SeqProductAccess,
};
use std::sync::Arc;

/// A page pool of currently unused pages available for use in [`Pages`](super::pages::Pages).
#[derive(Default, Clone)]
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
}

impl MemoryUsage for PagePoolInner {
    fn heap_usage(&self) -> usize {
        let Self { pages } = self;
        pages.capacity() * size_of::<(AtomicUsize, Box<Page>)>()
    }
}

impl Default for PagePoolInner {
    fn default() -> Self {
        const MAX_PAGE_MEM: usize = 32 * (1 << 30); // 32 GiB
        const PAGE_SIZE: usize = 64 * (1 << 10); // 64 KiB, `size_of::<Page>()`

        // 2 ^ 19 pages at most.
        // Each slot in the pool is `(AtomicCell, Box<Page>)` which takes up 16 bytes.
        // The pool will therefore have a fixed cost of 2^23 bytes, i.e., 8 MiB.
        const MAX_POOLED_PAGES: usize = MAX_PAGE_MEM / PAGE_SIZE;
        let pages = ArrayQueue::new(MAX_POOLED_PAGES);
        Self { pages }
    }
}

impl PagePoolInner {
    /// Puts back a [`Page`] into the pool.
    fn put(&self, page: Box<Page>) {
        // Add it to the pool if there's room, or just drop it.
        let _ = self.pages.push(page);
    }

    /// Takes a [`Page`] from the pool or creates a new one.
    ///
    /// The returned page supports a maximum of `max_rows_in_page` rows.
    fn take_with_max_row_count(&self, max_rows_in_page: usize) -> Box<Page> {
        self.pages
            .pop()
            .map(|mut page| {
                page.reset_for(max_rows_in_page);
                page
            })
            .unwrap_or_else(|| Page::new_with_max_row_count(max_rows_in_page))
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
    use core::ptr::addr_eq;

    fn present_rows_ptr(page: &Page) -> *const () {
        page.page_header_for_test().present_rows_storage_ptr_for_test()
    }

    #[test]
    fn page_pool_returns_same_page() {
        let pool = PagePool::default();

        // Create a page and put it back.
        let page1 = pool.take_with_max_row_count(10);
        let page1_ptr = &*page1 as *const _;
        let page1_pr_ptr = present_rows_ptr(&page1);
        pool.put(page1);

        // Extract a page again.
        let page2 = pool.take_with_max_row_count(64);
        let page2_ptr = &*page2 as *const _;
        let page2_pr_ptr = present_rows_ptr(&page2);
        // It should be the same as the previous one.
        assert!(addr_eq(page1_ptr, page2_ptr));
        // And the bitset should also be the same, as `10.div_ceil(64) == 64`.
        assert!(addr_eq(page1_pr_ptr, page2_pr_ptr));
        pool.put(page2);

        // Extract a page again, but this time, go beyond the first block.
        let page3 = pool.take_with_max_row_count(64 + 1);
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
        // When we take out a page, it should be the same as `page4` and not `page1`.
        let page5 = pool.take_with_max_row_count(10);
        let page5_ptr = &*page5 as *const _;
        // Same as page4.
        assert!(!addr_eq(page5_ptr, page1_ptr));
        assert!(addr_eq(page5_ptr, page4_ptr));
    }
}
