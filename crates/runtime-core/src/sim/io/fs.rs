use alloc::{collections::BTreeMap, sync::Arc};
use core::{
    cmp,
    sync::atomic::{AtomicU64, Ordering},
};

pub const PAGE_SIZE: usize = 4096;
const PAGE_SIZE_U64: u64 = PAGE_SIZE as u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    UnalignedOffset,
    UnalignedBuffer,
    OffsetOverflow,
}

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PageIndex(u64);

impl PageIndex {
    fn from_offset(offset: u64) -> Self {
        assert!(offset.is_multiple_of(PAGE_SIZE_U64));
        Self(offset / PAGE_SIZE_U64)
    }
}

struct Page {
    bytes: spin::Mutex<[u8; PAGE_SIZE]>,
}

impl Page {
    fn zeroed() -> Self {
        Self {
            bytes: spin::Mutex::new([0; PAGE_SIZE]),
        }
    }
}

/// A memory-backed file.
///
/// A [File] is backed by a sparse array of [Page]s. Missing pages are read as
/// zeroes.
///
/// Read and write operations must be page-aligned. Only full pages can be read
/// or written. Writing a page is atomic.
#[derive(Clone)]
pub struct File {
    pages: Arc<spin::Mutex<BTreeMap<PageIndex, Arc<Page>>>>,
    len: Arc<AtomicU64>,
}

impl File {
    pub(super) fn new() -> Self {
        Self {
            pages: Arc::new(spin::Mutex::new(BTreeMap::new())),
            len: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(super) fn len(&self) -> u64 {
        self.len.load(Ordering::Relaxed)
    }

    #[allow(unused)]
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Change the file length.
    ///
    /// The new length must be page-aligned.
    ///
    /// Extending allocates pages eagerly as needed. Shrinking drops all pages
    /// at or beyond the new EOF.
    pub(super) fn set_len(&self, new_len: u64) -> Result<()> {
        use cmp::Ordering::*;

        if !new_len.is_multiple_of(PAGE_SIZE_U64) {
            return Err(Error::UnalignedOffset);
        }
        let old_len = self.len();

        match new_len.cmp(&old_len) {
            Equal => {}
            Greater => {
                let first_new_page = old_len / PAGE_SIZE_U64;
                let end_page = new_len / PAGE_SIZE_U64;

                for index in first_new_page..end_page {
                    self.get_or_allocate_page(PageIndex(index));
                }

                self.len.store(new_len, Ordering::Relaxed);
            }
            Less => {
                self.len.store(new_len, Ordering::Relaxed);

                let first_removed = PageIndex::from_offset(new_len);
                let removed = self.pages.lock().split_off(&first_removed);
                drop(removed);
            }
        }

        Ok(())
    }

    /// Read one complete page.
    pub(super) fn read_page(&self, dst: &mut [u8], index: u64) -> Result<()> {
        if dst.len() != PAGE_SIZE {
            return Err(Error::UnalignedBuffer);
        }

        match self.get_page(PageIndex(index)) {
            Some(page) => {
                dst.copy_from_slice(&*page.bytes.lock());
            }
            None => {
                dst.fill(0);
            }
        }

        Ok(())
    }

    /// Write one complete page.
    pub(super) fn write_page(&self, src: &[u8], index: u64) -> Result<()> {
        if src.len() != PAGE_SIZE {
            return Err(Error::UnalignedBuffer);
        }

        let page = self.get_or_allocate_page(PageIndex(index));
        page.bytes.lock().copy_from_slice(src);

        let end = index
            .checked_add(1)
            .and_then(|pages| pages.checked_mul(PAGE_SIZE_U64))
            .ok_or(Error::OffsetOverflow)?;

        self.len.fetch_max(end, Ordering::Relaxed);

        Ok(())
    }

    fn get_page(&self, index: PageIndex) -> Option<Arc<Page>> {
        self.pages.lock().get(&index).cloned()
    }

    fn get_or_allocate_page(&self, index: PageIndex) -> Arc<Page> {
        let mut pages = self.pages.lock();
        Arc::clone(pages.entry(index).or_insert_with(|| Arc::new(Page::zeroed())))
    }
}
