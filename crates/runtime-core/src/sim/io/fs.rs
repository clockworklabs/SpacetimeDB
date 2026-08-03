use alloc::{collections::BTreeMap, rc::Rc};
use core::{
    cell::{Cell, RefCell},
    cmp,
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
    bytes: RefCell<[u8; PAGE_SIZE]>,
}

impl Page {
    fn zeroed() -> Self {
        Self {
            bytes: RefCell::new([0; PAGE_SIZE]),
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
    pages: RefCell<BTreeMap<PageIndex, Rc<Page>>>,
    len: Cell<u64>,
}

impl File {
    pub(super) const fn new() -> Self {
        Self {
            pages: RefCell::new(BTreeMap::new()),
            len: Cell::new(0),
        }
    }

    pub(super) const fn len(&self) -> u64 {
        self.len.get()
    }

    #[allow(unused)]
    pub(super) const fn is_empty(&self) -> bool {
        self.len.get() == 0
    }

    /// Change the file length.
    ///
    /// The new length must be page-aligned.
    ///
    /// Extending allocates pages eagerly as needed. Shrinking drops all pages
    /// at or beyond the new EOF.
    pub fn set_len(&self, new_len: u64) -> Result<()> {
        use cmp::Ordering::*;

        if !new_len.is_multiple_of(PAGE_SIZE_U64) {
            return Err(Error::UnalignedOffset);
        }
        let old_len = self.len.get();

        match new_len.cmp(&old_len) {
            Equal => {}
            Greater => {
                let first_new_page = old_len / PAGE_SIZE_U64;
                let end_page = new_len / PAGE_SIZE_U64;

                for index in first_new_page..end_page {
                    self.get_or_allocate_page(PageIndex(index));
                }

                self.len.set(new_len);
            }
            Less => {
                self.len.set(new_len);

                let first_removed = PageIndex::from_offset(new_len);
                let removed = self.pages.borrow_mut().split_off(&first_removed);
                drop(removed);
            }
        }

        Ok(())
    }

    /// Read one complete page.
    pub fn read_page(&self, dst: &mut [u8], index: u64) -> Result<()> {
        if dst.len() != PAGE_SIZE {
            return Err(Error::UnalignedBuffer);
        }

        match self.get_page(PageIndex(index)) {
            Some(page) => {
                dst.copy_from_slice(&*page.bytes.borrow());
            }
            None => {
                dst.fill(0);
            }
        }

        Ok(())
    }

    /// Write one complete page.
    pub fn write_page(&self, src: &[u8], index: u64) -> Result<()> {
        if src.len() != PAGE_SIZE {
            return Err(Error::UnalignedBuffer);
        }

        let page = self.get_or_allocate_page(PageIndex(index));
        page.bytes.borrow_mut().copy_from_slice(src);

        let end = index
            .checked_add(1)
            .and_then(|pages| pages.checked_mul(PAGE_SIZE_U64))
            .ok_or(Error::OffsetOverflow)?;

        self.len.set(cmp::max(self.len.get(), end));

        Ok(())
    }

    fn get_page(&self, index: PageIndex) -> Option<Rc<Page>> {
        self.pages.borrow().get(&index).cloned()
    }

    fn get_or_allocate_page(&self, index: PageIndex) -> Rc<Page> {
        let mut pages = self.pages.borrow_mut();
        Rc::clone(pages.entry(index).or_insert_with(|| Rc::new(Page::zeroed())))
    }
}
