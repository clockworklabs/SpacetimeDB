use core::ops::Deref;

use crate::page::Page;

use super::page_manager::{PageFrameReadGuard, PageHandle};

/// Page ownership protected by a table borrow or an eviction pin.
#[derive(Clone, Debug)]
pub enum PageRef<'a> {
    Resident(&'a Page),
    Tiered(PageHandle),
}

impl PageRef<'_> {
    #[inline(always)]
    fn borrow(&self) -> PageBorrow<'_> {
        match self {
            Self::Resident(page) => PageBorrow::Resident(page),
            Self::Tiered(handle) => PageBorrow::Tiered(handle),
        }
    }

    #[inline(always)]
    pub fn read(&self) -> PageRead<'_> {
        self.borrow().read()
    }

    #[inline(always)]
    pub fn with_page<T>(&self, f: impl FnOnce(&Page) -> T) -> T {
        self.borrow().with_page(f)
    }
}

/// A temporary borrow of page ownership retained by a RowRef.
#[derive(Clone, Copy)]
pub enum PageBorrow<'a> {
    Resident(&'a Page),
    Tiered(&'a PageHandle),
}

impl<'a> PageBorrow<'a> {
    #[inline(always)]
    pub fn read(self) -> PageRead<'a> {
        match self {
            Self::Resident(page) => PageRead::Resident(page),
            Self::Tiered(handle) => PageRead::Tiered(handle.read()),
        }
    }

    /// Resident column reads use the borrowed page without locking or building a read guard.
    /// The table borrow protects resident pages; tiered pages still acquire their read lock.
    #[inline(always)]
    pub fn with_page<T>(self, f: impl FnOnce(&Page) -> T) -> T {
        match self {
            Self::Resident(page) => f(page),
            Self::Tiered(handle) => f(&handle.read()),
        }
    }

    pub(crate) fn pin(self) -> Option<PageHandle> {
        match self {
            Self::Resident(_) => None,
            Self::Tiered(handle) => Some(handle.clone()),
        }
    }
}

pub enum PageRead<'a> {
    Resident(&'a Page),
    Tiered(PageFrameReadGuard),
}

impl Deref for PageRead<'_> {
    type Target = Page;

    #[inline]
    fn deref(&self) -> &Page {
        match self {
            Self::Resident(page) => page,
            Self::Tiered(guard) => guard,
        }
    }
}
