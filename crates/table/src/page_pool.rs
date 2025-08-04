use super::{
    indexes::max_rows_in_page,
    page::{Page, PageHeader},
};
use derive_more::Deref;
use spacetimedb_data_structures::object_pool::{Pool, PooledObject};
use spacetimedb_sats::bsatn::{self, DecodeError};
use spacetimedb_sats::de::{
    DeserializeSeed, Deserializer, Error, NamedProductAccess, ProductVisitor, SeqProductAccess,
};
use spacetimedb_sats::layout::Size;
use spacetimedb_sats::memory_usage::MemoryUsage;

impl PooledObject for Box<Page> {
    type ResidentBytesStorage = ();
    fn resident_object_bytes(_: &Self::ResidentBytesStorage, num_objects: usize) -> usize {
        // Each page takes up a fixed amount.
        num_objects * size_of::<Page>()
    }
    fn add_to_resident_object_bytes(_: &Self::ResidentBytesStorage, _: usize) {}
    fn sub_from_resident_object_bytes(_: &Self::ResidentBytesStorage, _: usize) {}
}

/// A page pool of currently unused pages available for use in [`Pages`](super::pages::Pages).
#[derive(Clone, Deref)]
pub struct PagePool {
    pool: Pool<Box<Page>>,
}

impl MemoryUsage for PagePool {
    fn heap_usage(&self) -> usize {
        self.pool.heap_usage()
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
        let pool = Pool::new(queue_size);
        Self { pool }
    }

    /// Takes a [`Page`] from the pool or creates a new one.
    ///
    /// The returned page supports fixed rows of size `fixed_row_size`.
    pub fn take_with_fixed_row_size(&self, fixed_row_size: Size) -> Box<Page> {
        self.take_with_max_row_count(max_rows_in_page(fixed_row_size))
    }

    /// Takes a [`Page`] from the pool or creates a new one.
    ///
    /// The returned page supports a maximum of `max_rows_in_page` rows.
    fn take_with_max_row_count(&self, max_rows_in_page: usize) -> Box<Page> {
        self.pool.take(
            |page| page.reset_for(max_rows_in_page),
            || Page::new_with_max_row_count(max_rows_in_page),
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr::addr_eq;

    fn present_rows_ptr(page: &Page) -> *const () {
        page.page_header_for_test().present_rows_storage_ptr_for_test()
    }

    #[test]
    fn page_pool_bitset_reuse() {
        let pool = PagePool::new_for_test();
        // Create a page and put it back.
        let page1 = pool.take_with_max_row_count(10);
        let page1_pr_ptr = present_rows_ptr(&page1);
        pool.put(page1);

        // Extract another page again, but use a different max row count (64).
        // The bitset should be the same, as `10.div_ceil(64) == 64`.
        let page2 = pool.take_with_max_row_count(64);
        assert!(addr_eq(page1_pr_ptr, present_rows_ptr(&page2)));
        pool.put(page2);

        // Extract a page again, but this time, go beyond the first bitset block.
        let page3 = pool.take_with_max_row_count(64 + 1);
        // The bitset should not be the same, as `65.div_ceil(64) == 2`.
        assert!(!addr_eq(page1_pr_ptr, present_rows_ptr(&page3)));
    }
}
