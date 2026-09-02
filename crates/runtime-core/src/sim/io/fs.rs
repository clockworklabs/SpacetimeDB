use alloc::{collections::BTreeMap, sync::Arc};
use core::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

pub const PAGE_SIZE: usize = 4096;
const PAGE_SIZE_U64: u64 = PAGE_SIZE as u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("unaligned offset")]
    UnalignedOffset,
    #[error("unaligned buffer")]
    UnalignedBuffer,
    #[error("offset overflow")]
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

#[derive(Default)]
struct PageMap {
    volatile: BTreeMap<PageIndex, Arc<Page>>,
    durable: BTreeMap<PageIndex, Arc<Page>>,
}

impl PageMap {
    /// Reset the volatile to the durable state.
    fn crash(&mut self) {
        self.volatile = self.durable.clone();
    }

    /// Move the page at `index` from the volatile to the durable state.
    fn sync(&mut self, index: PageIndex) {
        self.durable.insert(index, self.volatile.get(&index).cloned().unwrap());
    }

    /// Get the page at `index` for reading. Uses the durable state.
    fn get_page(&self, index: PageIndex) -> Option<Arc<Page>> {
        self.durable.get(&index).cloned()
    }

    /// Get the page at `index` for writing, or allocate a new page.
    /// Uses the volatile state.
    fn get_or_allocate_page(&mut self, index: PageIndex) -> Arc<Page> {
        Arc::clone(self.volatile.entry(index).or_insert_with(|| Arc::new(Page::zeroed())))
    }

    /// Change the allocated space, allocating or deallocating pages as needed.
    /// Changes the volatile state only.
    fn set_len_volatile(&mut self, new_len: u64) {
        Self::set_len(&mut self.volatile, new_len);
    }

    /// Like [Self::set_len], but operate on the durable state only.
    fn set_len_durable(&mut self, new_len: u64) {
        Self::set_len(&mut self.durable, new_len);
    }

    fn set_len(page_map: &mut BTreeMap<PageIndex, Arc<Page>>, new_len: u64) {
        use core::cmp::Ordering::*;

        let old_len = page_map.len() as u64;
        match new_len.cmp(&old_len) {
            Equal => {}
            Greater => {
                let first_new_page = old_len / PAGE_SIZE_U64;
                let end_page = new_len / PAGE_SIZE_U64;

                for index in first_new_page..end_page {
                    page_map
                        .entry(PageIndex(index))
                        .or_insert_with(|| Arc::new(Page::zeroed()));
                }
            }
            Less => {
                let first_removed = PageIndex::from_offset(new_len);
                let removed = page_map.split_off(&first_removed);
                drop(removed);
            }
        }
    }
}

pub enum Datasync {
    Sector(u64),
    Length,
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
    pages: Arc<spin::Mutex<PageMap>>,

    volatile_len: Arc<AtomicU64>,
    durable_len: Arc<AtomicU64>,
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("File")
            .field("volatile_len", &self.volatile_len)
            .field("durable_len", &self.durable_len)
            .finish()
    }
}

impl File {
    pub(super) fn new() -> Self {
        Self {
            pages: <_>::default(),
            volatile_len: <_>::default(),
            durable_len: <_>::default(),
        }
    }

    /// Simulate a crash by resetting to the durable state.
    pub(super) fn crash(&self) {
        self.volatile_len
            .store(self.durable_len.load(Ordering::Relaxed), Ordering::Relaxed);
        self.pages.lock().crash();
    }

    pub(super) fn len(&self) -> u64 {
        self.volatile_len.load(Ordering::Relaxed)
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
        if !new_len.is_multiple_of(PAGE_SIZE_U64) {
            return Err(Error::UnalignedOffset);
        }
        self.pages.lock().set_len_volatile(new_len);
        self.volatile_len.store(new_len, Ordering::Relaxed);

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

        self.volatile_len.fetch_max(end, Ordering::Relaxed);

        Ok(())
    }

    /// Execute an `fdatasync(2)` operation as a series of [Datasync] effects.
    ///
    /// The result may or may not leave the durable state in the same state as
    /// the volatile state at the time the operation started.
    ///
    /// It is the caller's responsibility to decide whether the operation is
    /// considered successful - a partial operation may report success, or a
    /// complete operation may report failure.
    pub(super) fn fdatasync(&self, ops: impl IntoIterator<Item = Datasync>) {
        for op in ops {
            match op {
                Datasync::Sector(offset) => {
                    self.pages.lock().sync(PageIndex(offset));
                }
                Datasync::Length => {
                    let new_durable_len = self.volatile_len.load(Ordering::Relaxed);
                    self.durable_len.store(new_durable_len, Ordering::Relaxed);
                    self.pages.lock().set_len_durable(new_durable_len);
                }
            }
        }
    }

    fn get_page(&self, index: PageIndex) -> Option<Arc<Page>> {
        self.pages.lock().get_page(index)
    }

    fn get_or_allocate_page(&self, index: PageIndex) -> Arc<Page> {
        self.pages.lock().get_or_allocate_page(index)
    }
}
