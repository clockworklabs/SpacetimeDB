//! Provides a [`Page`] abstraction that stores rows
//! and an associated header necessary for the page to work.
//! Consult the documentation of this type for a list of operations
//! and a description of how page work.
//!
//! A page can provide a split mutable view of its fixed section and its variable section.
//! This is provided through [`Page::split_fixed_var_mut`] with view operations
//! defined on [`FixedView`] and [`VarView`].
//!
//! [ralfj_safe_valid]: https://www.ralfj.de/blog/2018/08/22/two-kinds-of-invariants.html
//!
//! Technical terms:
//!
//! - `valid` refers to, when referring to a type, granule, or row,
//!    depending on the context, a memory location that holds a *safe* object.
//!    When "valid for writes" is used, the location must be properly aligned
//!    and none of its bytes may be uninit,
//!    but the value need not be valid at the type in question.
//!    "Valid for writes" is equivalent to valid-unconstrained.
//!
//! - `valid-unconstrained`, when referring to a memory location with a given type,
//!    that the location stores a byte pattern which Rust/LLVM's memory model recognizes as valid,
//!    and therefore must not contain any uninit,
//!    but the value is not required to be logically meaningful,
//!    and no code may depend on the data within it to uphold any invariants.
//!    E.g. an unallocated [`VarLenGranule`] within a page stores valid-unconstrained bytes,
//!    because the bytes are either 0 from the initial [`alloc_zeroed`] of the page,
//!    or contain stale data from a previously freed [`VarLenGranule`].
//!
//! - `unused` means that it is safe to overwrite a block of memory without cleaning up its previous value.
//!
//!    See the post [Two Kinds of Invariants: Safety and Validity][ralf_safe_valid]
//!    for a discussion on safety and validity invariants.

use super::{
    blob_store::BlobStore,
    fixed_bit_set::FixedBitSet,
    indexes::{Byte, Bytes, PageOffset, Size, PAGE_HEADER_SIZE, PAGE_SIZE},
    layout::MIN_ROW_SIZE,
    var_len::{is_granule_offset_aligned, VarLenGranule, VarLenGranuleHeader, VarLenMembers, VarLenRef},
};
use crate::{fixed_bit_set::IterSet, indexes::max_rows_in_page, static_assert_size, table::BlobNumBytes, MemoryUsage};
use core::{mem, ops::ControlFlow};
use spacetimedb_sats::{de::Deserialize, ser::Serialize};
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum Error {
    #[error("Want to allocate a var-len object of {need} granules, but have only {have} granules available")]
    InsufficientVarLenSpace { need: u16, have: u16 },
    #[error("Want to allocate a fixed-len row of {} bytes, but the page is full", need.len())]
    InsufficientFixedLenSpace { need: Size },
}

/// A cons-cell in a freelist either
/// for an unused fixed-len cell or a variable-length granule.
#[repr(C)] // Required for a stable ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::NoUninit, Serialize, Deserialize)]
struct FreeCellRef {
    /// The address of the next free cell in a freelist.
    ///
    /// The `PageOffset::PAGE_END` is used as a sentinel to signal "`None`".
    next: PageOffset,
}

impl MemoryUsage for FreeCellRef {
    fn heap_usage(&self) -> usize {
        let Self { next } = self;
        next.heap_usage()
    }
}

impl FreeCellRef {
    /// The sentinel for NULL cell references.
    const NIL: Self = Self {
        next: PageOffset::PAGE_END,
    };

    /// Replaces the cell reference with `offset`, returning the existing one.
    #[inline]
    fn replace(&mut self, offset: PageOffset) -> FreeCellRef {
        let next = mem::replace(&mut self.next, offset);
        Self { next }
    }

    /// Returns whether the cell reference is non-empty.
    #[inline]
    const fn has(&self) -> bool {
        !self.next.is_at_end()
    }

    /// Take the first free cell in the freelist starting with `self`, if any,
    /// and promote the second free cell as the freelist head.
    ///
    /// # Safety
    ///
    /// When `self.has()`, it must point to a valid `FreeCellRef`.
    #[inline]
    unsafe fn take_freelist_head(
        self: &mut FreeCellRef,
        row_data: &Bytes,
        adjust_free: impl FnOnce(PageOffset) -> PageOffset,
    ) -> Option<PageOffset> {
        self.has().then(|| {
            let head = adjust_free(self.next);
            // SAFETY: `self.next` so `head` points to a valid `FreeCellRef`.
            let next = unsafe { get_ref(row_data, head) };
            self.replace(*next).next
        })
    }

    /// Prepend `new_head` to the freelist starting with `self`.
    ///
    /// SAFETY: `new_head`, after adjustment, must be in bounds of `row_data`.
    /// Moreover, it must be valid for writing a `FreeCellRef` to it,
    /// which includes being properly aligned with respect to `row_data` for a `FreeCellRef`.
    /// Additionally, `self` must contain a valid `FreeCellRef`.
    #[inline]
    unsafe fn prepend_freelist(
        self: &mut FreeCellRef,
        row_data: &mut Bytes,
        new_head: PageOffset,
        adjust_free: impl FnOnce(PageOffset) -> PageOffset,
    ) {
        let next = self.replace(new_head);
        let new_head = adjust_free(new_head);
        // SAFETY: Per caller contract, `new_head` is in bounds of `row_data`.
        // Moreover, `new_head` points to an unused `FreeCellRef`, so we can write to it.
        let next_slot: &mut FreeCellRef = unsafe { get_mut(row_data, new_head) };
        *next_slot = next;
    }
}

/// All the fixed size header information.
#[repr(C)] // Required for a stable ABI.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)] // So we can dump and restore pages during snapshotting.
struct FixedHeader {
    /// A pointer to the head of the freelist which stores
    /// all the unused (freed) fixed row cells.
    /// These cells can be reused when inserting a new row.
    next_free: FreeCellRef,

    /// High water mark (HWM) for fixed-length rows.
    /// Points one past the last-allocated (highest-indexed) fixed-length row,
    /// so to allocate a fixed-length row from the gap,
    /// post-increment this index.
    // TODO(perf,future-work): determine how to lower the high water mark when freeing the topmost row.
    last: PageOffset,

    /// The number of rows currently in the page.
    ///
    /// N.B. this is not the same as `self.present_rows.len()`
    /// as that counts both zero and one bits.
    num_rows: u16,

    // TODO(stable-module-abi): should this be inlined into the page?
    /// For each fixed-length row slot, true if a row is stored there,
    /// false if the slot is unallocated.
    ///
    /// Unallocated row slots store valid-unconstrained bytes, i.e. are never uninit.
    present_rows: FixedBitSet,
}

impl MemoryUsage for FixedHeader {
    fn heap_usage(&self) -> usize {
        let Self {
            next_free,
            last,
            num_rows,
            present_rows,
        } = self;
        next_free.heap_usage() + last.heap_usage() + num_rows.heap_usage() + present_rows.heap_usage()
    }
}

static_assert_size!(FixedHeader, 16);

impl FixedHeader {
    /// Returns a new `FixedHeader`
    /// using the provided `max_rows_in_page` to decide how many rows `present_rows` can represent.
    #[inline]
    fn new(max_rows_in_page: usize) -> Self {
        Self {
            next_free: FreeCellRef::NIL,
            // Points one after the last allocated fixed-length row, or `NULL` for an empty page.
            last: PageOffset::VAR_LEN_NULL,
            num_rows: 0,
            present_rows: FixedBitSet::new(max_rows_in_page),
        }
    }

    /// Set the (fixed) row starting at `offset`
    /// and lasting `fixed_row_size` as `present`.
    #[inline]
    fn set_row_present(&mut self, offset: PageOffset, fixed_row_size: Size) {
        self.set_row_presence(offset, fixed_row_size, true);
        self.num_rows += 1;
    }

    /// Sets whether the (fixed) row starting at `offset`
    /// and lasting `fixed_row_size` is `present` or not.
    #[inline]
    fn set_row_presence(&mut self, offset: PageOffset, fixed_row_size: Size, present: bool) {
        self.present_rows.set(offset / fixed_row_size, present);
    }

    /// Returns whether the (fixed) row starting at `offset`
    /// and lasting `fixed_row_size` is present or not.
    #[inline]
    fn is_row_present(&self, offset: PageOffset, fixed_row_size: Size) -> bool {
        self.present_rows.get(offset / fixed_row_size)
    }

    /// Resets the header information to its state
    /// when it was first created in [`FixedHeader::new`]
    /// but with `max_rows_in_page` instead of the value passed on creation.
    fn reset_for(&mut self, max_rows_in_page: usize) {
        self.next_free = FreeCellRef::NIL;
        self.last = PageOffset::VAR_LEN_NULL;
        self.num_rows = 0;
        self.present_rows.reset_for(max_rows_in_page);
    }

    /// Resets the header information to its state
    /// when it was first created in [`FixedHeader::new`].
    ///
    /// The header is only good for the original row size.
    #[inline]
    fn clear(&mut self) {
        self.next_free = FreeCellRef::NIL;
        self.last = PageOffset::VAR_LEN_NULL;
        self.num_rows = 0;
        self.present_rows.clear();
    }
}

/// All the var-len header information.
#[repr(C)] // Required for a stable ABI.
#[derive(bytemuck::NoUninit, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct VarHeader {
    /// A pointer to the head of the freelist which stores
    /// all the unused (freed) var-len granules.
    /// These cells can be reused when inserting a new row.
    next_free: FreeCellRef,

    /// The length of the freelist with its head referred to by `next_free`.
    /// Stored in units of var-len nodes.
    ///
    /// This field is redundant,
    /// as it can be recovered by traversing `next_free`.
    /// However, traversing this linked-list is not cache friendly,
    /// so we memoize the computation here.
    freelist_len: u16,

    /// High water mark (HWM) for var-len granules.
    /// Points to the last-allocated (lowest-indexed) var-len granule,
    /// so to allocate a var-len granule from the gap,
    /// pre-decrement this index.
    // TODO(perf,future-work): determine how to "lower" the high water mark when freeing the "top"-most granule.
    first: PageOffset,

    /// The number of granules currently used by rows within this page.
    ///
    /// [`Page::bytes_used_by_rows`] needs this information.
    /// Stored here because otherwise counting it would require traversing all the present rows.
    num_granules: u16,
}

impl MemoryUsage for VarHeader {
    fn heap_usage(&self) -> usize {
        let Self {
            next_free,
            freelist_len,
            first,
            num_granules,
        } = self;
        next_free.heap_usage() + freelist_len.heap_usage() + first.heap_usage() + num_granules.heap_usage()
    }
}

static_assert_size!(VarHeader, 8);

impl Default for VarHeader {
    fn default() -> Self {
        Self {
            next_free: FreeCellRef::NIL,
            freelist_len: 0,
            first: PageOffset::PAGE_END,
            num_granules: 0,
        }
    }
}

impl VarHeader {
    /// Resets the header information to its state
    /// when it was first created in [`VarHeader::default`].
    fn clear(&mut self) {
        *self = Self::default();
    }
}

/// The metadata / header of a page that is necessary
/// for modifying and interpreting the `row_data`.
///
/// This header info is split into a header for the fixed part
/// and one for the variable part.
/// The header is stored in the same heap allocation as the `row_data`
/// as the whole [`Page`] is `Box`ed.
#[repr(C)] // Required for a stable ABI.
#[repr(align(64))] // Alignment must be same as `VarLenGranule::SIZE`.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)] // So we can dump and restore pages during snapshotting.
pub(super) struct PageHeader {
    /// The header data relating to the fixed component of a row.
    fixed: FixedHeader,
    /// The header data relating to the var-len component of a row.
    var: VarHeader,
    /// The content-addressed hash of the page on disk,
    /// if unmodified since the last snapshot,
    /// and `None` otherwise.
    ///
    /// This means that modifications to the page always sets this field to `None`.
    unmodified_hash: Option<blake3::Hash>,
}

impl MemoryUsage for PageHeader {
    fn heap_usage(&self) -> usize {
        let Self {
            fixed,
            var,
            // MEMUSE: no allocation, ok to ignore
            unmodified_hash: _,
        } = self;
        fixed.heap_usage() + var.heap_usage()
    }
}

static_assert_size!(PageHeader, PAGE_HEADER_SIZE);

impl PageHeader {
    /// Returns a new `PageHeader` proper a [`Page`] for holding at most `max_rows_in_page` rows.
    fn new(max_rows_in_page: usize) -> Self {
        Self {
            fixed: FixedHeader::new(max_rows_in_page),
            var: VarHeader::default(),
            unmodified_hash: None,
        }
    }

    /// Resets the header information to its state
    /// when it was first created in [`PageHeader::new`]
    /// but with `max_rows_in_page` instead of the value passed on creation.
    fn reset_for(&mut self, max_rows_in_page: usize) {
        self.fixed.reset_for(max_rows_in_page);
        self.var.clear();
        self.unmodified_hash = None;
    }

    /// Resets the header information to its state
    /// when it was first created in [`PageHeader::new`].
    ///
    /// The header is only good for the original row size.
    fn clear(&mut self) {
        self.fixed.clear();
        self.var.clear();
        self.unmodified_hash = None;
    }

    /// Returns the maximum number of rows the page can hold.
    ///
    /// Note that this number can be bigger
    /// than the value provided in [`Self::new`] due to rounding up.
    pub(super) fn max_rows_in_page(&self) -> usize {
        self.fixed.present_rows.bits()
    }

    /// Returns a pointer to the `present_rows` bitset.
    /// This is exposed for testing only.
    #[cfg(test)]
    pub(super) fn present_rows_storage_ptr_for_test(&self) -> *const () {
        self.fixed.present_rows.storage().as_ptr().cast()
    }
}

/// Fixed-length row portions must be at least large enough to store a `FreeCellRef`.
const _MIN_ROW_SIZE_CAN_STORE_FCR: () = assert!(MIN_ROW_SIZE.len() >= mem::size_of::<FreeCellRef>());

/// [`VarLenGranule`]s must be at least large enough to store a [`FreeCellRef`].
const _VLG_CAN_STORE_FCR: () = assert!(VarLenGranule::SIZE.len() >= MIN_ROW_SIZE.len());

/// Pointers properly aligned for a [`VarLenGranule`] must be properly aligned for [`FreeCellRef`].
/// This is the case as the former's alignment is a multiple of the latter's alignment.
const _VLG_ALIGN_MULTIPLE_OF_FCR: () = assert!(mem::align_of::<VarLenGranule>() % mem::align_of::<FreeCellRef>() == 0);

/// The actual row data of a [`Page`].
type RowData = [Byte; PageOffset::PAGE_END.idx()];

/// A page of row data with an associated `header` and the raw `row_data` itself.
///
/// As a rough summary, the strategy employed by this page is:
///
/// - The fixed-len parts of rows grows left-to-right
///   and starts from the beginning of the `row_data`
///   until its high water mark (fixed HWM), i.e., `self.header.fixed.last`.
///
/// - The var-len parts of rows grows right-to-left
///   and starts from the end of the `row_data`
///   until its high water mark (variable HWM), i.e., `self.header.var.first`.
///
///   Each var-len object is stored in terms of a linked-list of chunks.
///   Each chunk in this case is a [`VarLenGranule`] taking up 64 bytes where:
///   - 6 bits = length, 10 bits = next-cell-pointer
///   - 62 bytes = the bytes of the object
///
/// - As new rows are added, the HWMs move appropriately.
///   When the fixed and variable HWMs meet, the page is full.
///
/// - When rows are freed, a freelist strategy is used both for
///   the fixed parts and each `VarLenGranule`.
///   These freelists are then used first before using space from the gap.
///   The head of these freelists are stored in `next_free`
///   in the fixed and variable headers respectively.
///
/// - As the fixed parts of rows may store pointers into the var-length section,
///   to ensure that these pointers aren't dangling,
///   the page uses pointer fixups when adding to, deleting from, and copying the page.
///   These fixups are handled by having callers provide `VarLenMembers`
///   to find the var-len reference slots in the fixed parts.
#[repr(C)]
// ^-- Required for a stable ABI.
#[repr(align(64))]
// ^-- Must have align at least that of `VarLenGranule`,
// so that `row_data[PageOffset::PAGE_END - VarLenGranule::SIZE]` is an aligned pointer to `VarLenGranule`.
// TODO(bikeshedding): consider raising the alignment. We may want this to be OS page (4096) aligned.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)] // So we can dump and restore pages during snapshotting.
pub struct Page {
    /// The header containing metadata on how to interpret and modify the `row_data`.
    header: PageHeader,
    /// The actual bytes stored in the page.
    /// This contains row data, fixed and variable, and freelists.
    row_data: RowData,
}

impl MemoryUsage for Page {
    fn heap_usage(&self) -> usize {
        self.header.heap_usage()
    }
}

static_assert_size!(Page, PAGE_SIZE);

/// A mutable view of the fixed-len section of a [`Page`].
pub struct FixedView<'page> {
    /// A mutable view of the fixed-len bytes.
    fixed_row_data: &'page mut Bytes,
    /// A mutable view of the fixed header.
    header: &'page mut FixedHeader,
}

impl FixedView<'_> {
    /// Returns a mutable view of the row from `start` lasting `fixed_row_size` number of bytes.
    ///
    /// This method is safe, but callers should take care that `start` and `fixed_row_size`
    /// are correct for this page, and that `start` is aligned.
    /// Callers should further ensure that mutations to the row leave the row bytes
    /// in an expected state, i.e. initialized where required by the row type,
    /// and with `VarLenRef`s that point to valid granules and with correct lengths.
    pub fn get_row_mut(&mut self, start: PageOffset, fixed_row_size: Size) -> &mut Bytes {
        &mut self.fixed_row_data[start.range(fixed_row_size)]
    }

    /// Returns a shared view of the row from `start` lasting `fixed_row_size` number of bytes.
    fn get_row(&mut self, start: PageOffset, fixed_row_size: Size) -> &Bytes {
        &self.fixed_row_data[start.range(fixed_row_size)]
    }

    /// Frees the row starting at `row_offset` and lasting `fixed_row_size` bytes.
    ///
    /// # Safety
    ///
    /// `range_move(0..fixed_row_size, row_offset)` must be in bounds of `row_data`.
    /// Moreover, it must be valid for writing a `FreeCellRef` to it,
    /// which includes being properly aligned with respect to `row_data` for a `FreeCellRef`.
    pub unsafe fn free(&mut self, row_offset: PageOffset, fixed_row_size: Size) {
        // TODO(perf,future-work): if `row` is at the HWM, return it to the gap.

        // SAFETY: Per caller contract, `row_offset` must be in bounds of `row_data`.
        // Moreover, it must be valid for writing a `FreeCellRef` to it,
        // which includes being properly aligned with respect to `row_data` for a `FreeCellRef`.
        // We also know that `self.header.next_free` contains a valid `FreeCellRef`.
        unsafe {
            self.header
                .next_free
                .prepend_freelist(self.fixed_row_data, row_offset, |x| x)
        };
        self.header.num_rows -= 1;
        self.header.set_row_presence(row_offset, fixed_row_size, false);
    }
}

/// A mutable view of the var-len section of a [`Page`].
pub struct VarView<'page> {
    /// A mutable view of the var-len bytes.
    var_row_data: &'page mut Bytes,
    /// A mutable view of the var-len header.
    header: &'page mut VarHeader,
    /// One past the end of the fixed-len section of the page.
    last_fixed: PageOffset,
}

impl<'page> VarView<'page> {
    /// Returns the number of granules required to store the data,
    /// whether the page has enough space,
    /// and whether the object needs to go in the blob store.
    ///
    /// If the third value is `true`, i.e., the object will go in the blob store,
    /// the first value will always be `1`.
    fn has_enough_space_for(&self, len_in_bytes: usize) -> (usize, bool, bool) {
        let (num_granules_req, in_blob) = VarLenGranule::bytes_to_granules(len_in_bytes);
        let enough_space = num_granules_req <= self.num_granules_available();
        (num_granules_req, enough_space, in_blob)
    }

    /// Returns the number of granules available for allocation.
    fn num_granules_available(&self) -> usize {
        self.header.freelist_len as usize
            + VarLenGranule::space_to_granules(gap_remaining_size(self.header.first, self.last_fixed))
    }

    /// Provides an adjuster of offset in terms of `Page::row_data`
    /// to work in terms of `VarView::var_row_data`.
    ///
    /// This has to be done due to `page.row_data.split_at_mut(last_fixed)`.
    #[inline(always)]
    fn adjuster(&self) -> impl FnOnce(PageOffset) -> PageOffset {
        let lf = self.last_fixed;
        move |offset| offset - lf
    }

    /// Allocates a linked-list of granules, in the var-len storage of the page,
    /// for a var-len object of `obj_len` bytes.
    ///
    /// Returns a [`VarLenRef`] pointing to the head of that list,
    /// and a boolean `in_blob` for whether the allocation is a `BlobHash`
    /// and the object must be inserted into the large-blob store.
    ///
    /// The length of each granule is set, but no data is written to any granule.
    /// Thus, the caller must proceed to write data to each granule for the claimed lengths.
    ///
    /// # Safety post-requirements
    ///
    /// The following are the safety *post-requirements* of calling this method.
    /// That is, this method is safe to call,
    /// but may leave the page in an inconsistent state
    /// which must be rectified before other **unsafe methods** may be called.
    ///
    /// 1. When the returned `in_blob` holds, caller must ensure that,
    ///    before the granule's data is read from / assumed to be initialized,
    ///    the granule pointed to by the returned `vlr.first_granule`
    ///    has an initialized header and a data section initialized to at least
    ///    as many bytes as claimed by the header.
    ///
    /// 2. The caller must initialize each granule with data for the claimed length
    ///    of the granule's data.
    pub fn alloc_for_len(&mut self, obj_len: usize) -> Result<(VarLenRef, bool), Error> {
        // Safety post-requirements of `alloc_for_obj_common`:
        // 1. caller promised they will be satisfied.
        // 2a. already satisfied as the closure below returns all the summands of `obj_len`.
        // 2b. caller promised in 2. that they will satisfy this.
        self.alloc_for_obj_common(obj_len, |req_granules| {
            let rem = obj_len % VarLenGranule::DATA_SIZE;
            (0..req_granules).map(move |rev_idx| {
                let len = if rev_idx == 0 && rem != 0 {
                    // The first allocated granule will be the last in the list.
                    // Thus, `rev_idx == 0` is the last element and might not take up a full granule.
                    rem
                } else {
                    VarLenGranule::DATA_SIZE
                };
                // Caller will initialize the granule's data for `len` bytes.
                (<&[u8]>::default(), len)
            })
        })
    }

    /// Returns an iterator over all offsets of the `VarLenGranule`s of the var-len object
    /// that has its first granule at offset `first_granule`.
    /// An empty iterator will be returned when `first_granule` is `NULL`.
    ///
    /// # Safety
    ///
    /// `first_granule` must be an offset to a granule or `NULL`.
    /// The data of the granule need not be initialized.
    pub unsafe fn granule_offset_iter(&mut self, first_granule: PageOffset) -> GranuleOffsetIter<'_, 'page> {
        GranuleOffsetIter {
            next_granule: first_granule,
            var_view: self,
        }
    }

    /// Allocates and stores `slice` as a linked-list of granules
    /// in the var-len storage of the page.
    ///
    /// Returns a [`VarLenRef`] pointing to the head of that list,
    /// and a boolean `in_blob` for whether the allocation is a `BlobHash`
    /// and the `slice` must be inserted into the large-blob store.
    ///
    /// # Safety post-requirements
    ///
    /// The following are the safety *post-requirements* of calling this method.
    /// That is, this method is safe to call,
    /// but may leave the page in an inconsistent state
    /// which must be rectified before other **unsafe methods** may be called.
    ///
    /// 1. When the returned `in_blob` holds, caller must ensure that,
    ///    before the granule's data is read from / assumed to be initialized,
    ///    the granule pointed to by the returned `vlr.first_granule`
    ///    has an initialized header and a data section initialized to at least
    ///    as many bytes as claimed by the header.
    pub fn alloc_for_slice(&mut self, slice: &[u8]) -> Result<(VarLenRef, bool), Error> {
        let obj_len = slice.len();
        // Safety post-requirement 2. of `alloc_for_obj_common` is already satisfied
        // as `chunks(slice)` will return sub-slices where the sum is `obj_len`.
        // Moreover, we initialize each granule already with the right data and length.
        // The requirement 1. is forwarded to the caller.
        let chunks = |_| VarLenGranule::chunks(slice).rev().map(|c| (c, c.len()));
        self.alloc_for_obj_common(obj_len, chunks)
    }

    /// Allocates for `obj_len` bytes as a linked-list of granules
    /// in the var-len storage of the page.
    ///
    /// For every granule in the aforementioned linked-list,
    /// the caller must provide an element in the *reversed* iterator `chunks`,
    /// and of pairs `(chunk, len)`.
    /// To each granule `chunk` will be written and the granule will be of length `len`.
    /// The caller can opt to provide `chunk` that is not of `len`.
    ///
    /// Returns a [`VarLenRef`] pointing to the head of that list,
    /// and a boolean `in_blob` for whether the allocation is a `BlobHash`
    /// and the `slice` must be inserted into the large-blob store.
    ///
    /// # Safety post-requirements
    ///
    /// The following are the safety *post-requirements* of calling this method.
    /// That is, this method is safe to call,
    /// but may leave the page in an inconsistent state
    /// which must be rectified before other **unsafe methods** may be called.
    ///
    /// 1. When the returned `in_blob` holds, caller must ensure that,
    ///    before the granule's data is read from / assumed to be initialized,
    ///    the granule pointed to by the returned `vlr.first_granule`
    ///    has an initialized header and a data section initialized to at least
    ///    as many bytes as claimed by the header.
    ///
    /// 2. Otherwise, when `in_blob` doesn't hold the safety post-requirements are:
    ///
    ///    a. Let `cs = chunks(req_granules)` for the `req_granules` derived from `obj_len`.
    ///       Then, `obj_len == cs.map(|(_, len)| len).sum()`.
    ///
    ///    b. For each `(_, len) ∈ cs`, caller must ensure that
    ///       the relevant granule is initialized with data for at least `len`
    ///       before the granule's data is read from / assumed to be initialized.
    fn alloc_for_obj_common<'chunk, Cs: Iterator<Item = (&'chunk [u8], usize)>>(
        &mut self,
        obj_len: usize,
        chunks: impl Copy + FnOnce(usize) -> Cs,
    ) -> Result<(VarLenRef, bool), Error> {
        // Check that we have sufficient space to allocate `obj_len` bytes in var-len data.
        let (req_granules, enough_space, in_blob) = self.has_enough_space_for(obj_len);
        if !enough_space {
            return Err(Error::InsufficientVarLenSpace {
                need: req_granules.try_into().unwrap_or(u16::MAX),
                have: self.num_granules_available().try_into().unwrap_or(u16::MAX),
            });
        }

        // For large blob objects, only reserve a granule.
        // The caller promised that they will initialize it with a blob hash.
        if in_blob {
            let vlr = self.alloc_blob_hash()?;
            return Ok((vlr, true));
        };

        // Write each `chunk` to var-len storage.
        // To do this, we allocate granules for and store the chunks in reverse,
        // starting with the end first.
        // The offset to the previous granule in the iteration is kept to
        // link it in as the next pointer in the current iteration.
        let mut next = PageOffset::VAR_LEN_NULL;
        debug_assert_eq!(obj_len, chunks(req_granules).map(|(_, len)| len).sum::<usize>());
        for (chunk, len) in chunks(req_granules) {
            // This should never error, since we already checked for available space.
            let granule = self.alloc_granule()?;
            // SAFETY:
            // 1. `granule` is properly aligned as it came from `alloc_granule`
            //    and so is `next` as it's either NULL or was the previous `granule`.
            //    This also ensures that both are in bounds
            //    of the page for `granule + granule + VarLenGranule::SIZE`.
            //
            // 2. `next` is either NULL or was initialized in the previous loop iteration.
            //
            // 3. `granule` points to an unused slot as the space was just allocated.
            unsafe { self.write_chunk_to_granule(chunk, len, granule, next) };
            next = granule;
        }

        Ok((
            VarLenRef {
                first_granule: next,
                length_in_bytes: obj_len as u16,
            },
            false,
        ))
    }

    /// Allocates a granule for a large blob object
    /// and returns a [`VarLenRef`] pointing to that granule.
    ///
    /// The granule is not initialized by this method, and contains valid-unconstrained bytes.
    /// It is the caller's responsibility to initialize it with a [`BlobHash`](super::blob_hash::BlobHash).
    #[cold]
    fn alloc_blob_hash(&mut self) -> Result<VarLenRef, Error> {
        // Var-len hashes are 32 bytes, which fits within a single granule.
        self.alloc_granule().map(VarLenRef::large_blob)
    }

    /// Inserts `var_len_obj` into `blob_store`
    /// and stores the blob hash in the granule pointed to by `vlr.first_granule`.
    ///
    /// This insertion will never fail.
    ///
    /// # Safety
    ///
    /// `vlr.first_granule` must point to an unused `VarLenGranule` in bounds of this page,
    /// which must be valid for writes.
    pub unsafe fn write_large_blob_hash_to_granule(
        &mut self,
        blob_store: &mut dyn BlobStore,
        var_len_obj: &impl AsRef<[u8]>,
        vlr: VarLenRef,
    ) -> BlobNumBytes {
        let hash = blob_store.insert_blob(var_len_obj.as_ref());

        let granule = vlr.first_granule;
        // SAFETY:
        // 1. `granule` is properly aligned for `VarLenGranule` and is in bounds of the page.
        // 2. The null granule is trivially initialized.
        // 3. The caller promised that `granule` is safe to overwrite.
        unsafe { self.write_chunk_to_granule(&hash.data, hash.data.len(), granule, PageOffset::VAR_LEN_NULL) };
        var_len_obj.as_ref().len().into()
    }

    /// Write the `chunk` (data) to the [`VarLenGranule`] pointed to by `granule`,
    /// set the granule's length to be `len`,
    /// and set the next granule in the list to `next`.
    ///
    /// SAFETY:
    ///
    /// 1. Both `granule` and `next` must be properly aligned pointers to [`VarLenGranule`]s
    ///    and they must be in bounds of the page. However, neither need to point to init data.
    ///
    /// 2. The caller must initialize the granule pointed to by `next`
    ///    before the granule-list is read from (e.g., iterated on).
    ///    The null granule is considered trivially initialized.
    ///
    /// 3. The space pointed to by `granule` must be unused and valid for writes,
    ///    and will be overwritten here.
    unsafe fn write_chunk_to_granule(&mut self, chunk: &[u8], len: usize, granule: PageOffset, next: PageOffset) {
        let granule = self.adjuster()(granule);
        // SAFETY: A `PageOffset` is always in bounds of the page.
        let ptr: *mut VarLenGranule = unsafe { offset_to_ptr_mut(self.var_row_data, granule).cast() };

        // TODO(centril,bikeshedding): check if creating the `VarLenGranule` first on stack
        // and then writing to `ptr` would have any impact on perf.
        // This would be nicer as it requires less `unsafe`.

        // We need to initialize `Page::header`
        // without materializing a `&mut` as that is instant UB.
        // SAFETY: `ptr` isn't NULL as `&mut self.row_data` itself is a non-null pointer.
        let header = unsafe { &raw mut (*ptr).header };

        // SAFETY: `header` is valid for writes as only we have exclusive access.
        //          (1) The `ptr` was also promised as aligned
        //          and `granule + (granule + 64 bytes)` is in bounds of the page per caller contract.
        //          (2) Moreover, `next` will be an initialized granule per caller contract,
        //          so we can link it into the list without causing UB elsewhere.
        //          (3) It's also OK to write to `granule` as it's unused.
        unsafe {
            header.write(VarLenGranuleHeader::new(len as u8, next));
        }

        // SAFETY: We can treat any part of `row_data` as `.data`. Also (1) and (2).
        let data = unsafe { &mut (*ptr).data };

        // Copy the data into the granule.
        data[0..chunk.len()].copy_from_slice(chunk);
    }

    /// Allocate a [`VarLenGranule`] at the returned [`PageOffset`].
    ///
    /// The allocated storage is not initialized by this method,
    /// and will be valid-unconstrained at [`VarLenGranule`].
    ///
    /// This offset will be properly aligned for `VarLenGranule` when converted to a pointer.
    ///
    /// Returns an error when there are neither free granules nor space in the gap left.
    fn alloc_granule(&mut self) -> Result<PageOffset, Error> {
        let granule = self
            .alloc_from_freelist()
            .or_else(|| self.alloc_from_gap())
            .ok_or(Error::InsufficientVarLenSpace { need: 1, have: 0 })?;

        debug_assert!(
            is_granule_offset_aligned(granule),
            "Allocated an unaligned var-len granule: {:x}",
            granule,
        );

        self.header.num_granules += 1;

        Ok(granule)
    }

    /// Allocate a [`VarLenGranule`] at the returned [`PageOffset`]
    /// taken from the freelist, if any.
    #[inline]
    fn alloc_from_freelist(&mut self) -> Option<PageOffset> {
        // SAFETY: `header.next_free` points to a `c: FreeCellRef` when the former `.has()`.
        let free = unsafe {
            self.header
                .next_free
                .take_freelist_head(self.var_row_data, |o| o - self.last_fixed)
        }?;
        self.header.freelist_len -= 1;
        Some(free)
    }

    /// Allocate a [`VarLenGranule`] at the returned [`PageOffset`]
    /// taken from the gap, if there is space left, or `None` if there is insufficient space.
    #[inline]
    fn alloc_from_gap(&mut self) -> Option<PageOffset> {
        if gap_enough_size_for_row(self.header.first, self.last_fixed, VarLenGranule::SIZE) {
            // `var.first` points *at* the lowest-indexed var-len granule,
            // *not* before it, so pre-decrement.
            self.header.first -= VarLenGranule::SIZE;
            Some(self.header.first)
        } else {
            None
        }
    }

    /// Free a single var-len granule pointed to at by `offset`.
    ///
    /// SAFETY: `offset` must point to a valid [`VarLenGranule`].
    #[inline]
    unsafe fn free_granule(&mut self, offset: PageOffset) {
        // TODO(perf,future-work): if `chunk` is at the HWM, return it to the gap.
        //       Returning a single chunk to the gap is easy,
        //       but we want to return a whole "run" of sequential freed chunks,
        //       which requries some bookkeeping (or an O(> n) linked list traversal).
        self.header.freelist_len += 1;
        self.header.num_granules -= 1;
        let adjuster = self.adjuster();

        // SAFETY: Per caller contract, `offset` is a valid `VarLenGranule`,
        // and is therefore in bounds of the page row data.
        // By `_VLG_CAN_STORE_FCR`, and as we won't be reading from the granule anymore,
        // we know that this makes it valid for writing a `FreeCellRef` to it.
        // Moreover, by `_VLG_ALIGN_MULTIPLE_OF_FCR`,
        // the derived pointer is properly aligned (64) for a granule
        // and as `64 % 2 == 0` the alignment of a granule works for a `FreeCellRef`.
        // Finally, `self.header.next_free` contains a valid `FreeCellRef`.
        unsafe {
            self.header
                .next_free
                .prepend_freelist(self.var_row_data, offset, adjuster)
        };
    }

    /// Returns a reference to the granule at `offset`.
    ///
    /// SAFETY: `offset` must point to a valid [`VarLenGranule`].
    unsafe fn get_granule_ref(&self, offset: PageOffset) -> &VarLenGranule {
        unsafe { get_ref(self.var_row_data, self.adjuster()(offset)) }
    }

    /// Frees the blob pointed to by the [`BlobHash`] stored in the granule at `offset`.
    ///
    /// Panics when `offset` is NULL.
    ///
    /// SAFETY: `offset` must point to a valid [`VarLenGranule`] or be NULL.
    #[cold]
    #[inline(never)]
    unsafe fn free_blob(&self, offset: PageOffset, blob_store: &mut dyn BlobStore) -> BlobNumBytes {
        assert!(!offset.is_var_len_null());

        // SAFETY: Per caller contract + the assertion above,
        // we know `offset` refers to a valid `VarLenGranule`.
        let granule = unsafe { self.get_granule_ref(offset) };

        // Actually free the blob.
        let hash = granule.blob_hash();

        // The size of `deleted_bytes` is calculated here instead of requesting it from `blob_store`.
        // This is because the actual number of bytes deleted depends on the `blob_store`'s logic.
        // We prefer to measure it from the datastore's point of view.
        let blob_store_deleted_bytes = blob_store
            .retrieve_blob(&hash)
            .expect("failed to free var-len blob")
            .len()
            .into();

        // Actually free the blob.
        blob_store.free_blob(&hash).expect("failed to free var-len blob");

        blob_store_deleted_bytes
    }

    /// Frees an entire var-len linked-list object.
    ///
    /// If the `var_len_obj` is a large blob,
    /// the `VarLenGranule` which stores its blob hash will be freed from the page,
    /// but the blob itself will not be freed from the blob store.
    /// If used incorrectly, this may leak large blobs.
    ///
    /// This behavior is used to roll-back on failure in `[crate::bflatn::ser::write_av_to_page]`,
    /// where inserting large blobs is deferred until all allocations succeed.
    /// Freeing a fully-inserted object should instead use [`Self::free_object`].
    ///
    /// # Safety
    ///
    /// `var_len_obj.first_granule` must point to a valid [`VarLenGranule`] or be NULL.
    pub unsafe fn free_object_ignore_blob(&mut self, var_len_obj: VarLenRef) {
        let mut next_granule = var_len_obj.first_granule;

        while !next_granule.is_var_len_null() {
            // SAFETY: Per caller contract, `first_granule` points to a valid granule or is NULL.
            // We know however at this point that it isn't NULL so it is valid.
            // Thus the successor is too a valid granule or NULL.
            // However, again, at this point we know that the successor isn't NULL.
            // It follows then by induction that any `next_granule` at this point is valid.
            // Thus we have fulfilled the requirement that `next_granule` points to a valid granule.
            let header = unsafe { self.get_granule_ref(next_granule) }.header;
            // SAFETY: `next_granule` still points to a valid granule per above.
            unsafe {
                self.free_granule(next_granule);
            }
            next_granule = header.next();
        }
    }

    /// Frees an entire var-len linked-list object.
    ///
    /// SAFETY: `var_len_obj.first_granule` must point to a valid [`VarLenGranule`] or be NULL.
    unsafe fn free_object(&mut self, var_len_obj: VarLenRef, blob_store: &mut dyn BlobStore) -> BlobNumBytes {
        let mut blob_store_deleted_bytes = BlobNumBytes::default();
        // For large blob objects, extract the hash and tell `blob_store` to discard it.
        if var_len_obj.is_large_blob() {
            // SAFETY: `var_len_obj.first_granule` was promised to
            // point to a valid [`VarLenGranule`] or be NULL, as required.
            unsafe {
                blob_store_deleted_bytes = self.free_blob(var_len_obj.first_granule, blob_store);
            }
        }

        // SAFETY: `free_object_ignore_blob` has the same safety contract as this method.
        unsafe {
            self.free_object_ignore_blob(var_len_obj);
        }

        blob_store_deleted_bytes
    }
}

/// An iterator yielding the offsets to the granules of a var-len object.
pub struct GranuleOffsetIter<'vv, 'page> {
    /// Our mutable view of the page.
    var_view: &'vv mut VarView<'page>,
    /// The offset, that will be yielded next, pointing to next granule.
    next_granule: PageOffset,
}

impl GranuleOffsetIter<'_, '_> {
    /// Returns a mutable view of, for the `granule` at `offset`, `granule.data[start..]`.
    ///
    /// # Safety
    ///
    /// - `offset` must point to a valid granule
    /// - `start < VarLenGranule::DATA_SIZE`
    pub unsafe fn get_mut_data(&mut self, offset: PageOffset, start: usize) -> &mut Bytes {
        // SAFETY: Caller promised that `offset` points o a valid granule.
        let granule: &mut VarLenGranule = unsafe { get_mut(self.var_view.var_row_data, offset) };
        // SAFETY: Caller promised `start < granule.data.len()`.
        unsafe { granule.data.as_mut_slice().get_unchecked_mut(start..) }
    }
}

impl Iterator for GranuleOffsetIter<'_, '_> {
    type Item = PageOffset;
    fn next(&mut self) -> Option<Self::Item> {
        let adjust = self.var_view.adjuster();

        if self.next_granule.is_var_len_null() {
            return None;
        }
        let ret = adjust(self.next_granule);
        // SAFETY: By construction,
        // the initial `next_granule` was promised to either be `NULL` or point to a valid granule.
        // For a given granule, the same applies to its `.next()` granule.
        // At this point, we've excluded `NULL`,
        // so we know inductively that `next_granule` points to a valid granule, as required.
        let granule: &VarLenGranule = unsafe { get_ref(self.var_view.var_row_data, ret) };
        self.next_granule = granule.header.next();

        Some(ret)
    }
}

/// Assert that `ptr` is sufficiently aligned to reference a value of `T`.
///
/// In release mode, this is a no-op.
fn assert_alignment<T>(ptr: *const Byte) {
    debug_assert_eq!(
        ptr as usize % mem::align_of::<T>(),
        0,
        "Wanted a PageOffset with align 0x{:x} (for {}) but found 0x{:x}",
        mem::align_of::<T>(),
        std::any::type_name::<T>(),
        ptr as usize,
    );
}

/// Returns a reference to the [`T`] pointed to at by `offset`.
///
/// # Safety
///
/// `offset` must point to a valid `T` in `row_data`.
#[inline]
pub unsafe fn get_ref<T>(row_data: &Bytes, offset: PageOffset) -> &T {
    // SAFETY: Caller promised that `offset` is in bounds of `row_data`.
    let ptr = unsafe { offset_to_ptr(row_data, offset) };
    assert_alignment::<T>(ptr);
    let ptr = ptr.cast::<T>();
    // SAFETY: Caller promised that `offset` points to a `T` in `row_data`.
    unsafe { &*ptr }
}

/// Returns a mutable reference to the [`T`] pointed to at by `offset`.
///
/// # Safety
///
/// `offset` must point to a valid `T` in `row_data`.
#[inline]
unsafe fn get_mut<T>(row_data: &mut Bytes, offset: PageOffset) -> &mut T {
    // SAFETY: Caller promised that `offset` is in bounds of `row_data`.
    let ptr = unsafe { offset_to_ptr_mut(row_data, offset) };
    assert_alignment::<T>(ptr as *const Byte);
    let ptr = ptr.cast::<T>();
    // SAFETY: Caller promised that `offset` points to a `T` in `row_data`.
    unsafe { &mut *ptr }
}

/// Returns a raw const pointer into the `row_data` at `offset` bytes.
///
/// # Safety
///
/// `offset` must be in bounds or one past end of `row_data`.
#[inline]
unsafe fn offset_to_ptr(row_data: &Bytes, offset: PageOffset) -> *const Byte {
    debug_assert!(offset.idx() <= row_data.len());

    // SAFETY: per caller contract, `offset` is in bounds or one past end of `row_data`.
    unsafe { row_data.as_ptr().add(offset.idx()) }
}

/// Returns a raw mutable pointer into the `row_data` at `offset` bytes.
///
/// SAFETY: `offset` must be in bounds or one past end of `row_data`.
#[inline]
unsafe fn offset_to_ptr_mut(row_data: &mut Bytes, offset: PageOffset) -> *mut Byte {
    debug_assert!(offset.idx() <= row_data.len());

    // SAFETY: per caller contract, `offset` is in bounds or one past end of `row_data`.
    unsafe { row_data.as_mut_ptr().add(offset.idx()) }
}

/// Returns the size of the gap,
/// assuming `first_var` is the high water mark (HWM) of the var-len section,
/// pointing *at* the granule with the lowest offset,
/// and `last_fixed` is the HWM of the fixed-len section,
/// pointing *one past the end* of the last fixed row.
#[inline]
fn gap_remaining_size(first_var: PageOffset, last_fixed: PageOffset) -> Size {
    // For illustration, suppose `row_data` is 10 bytes, i.e., `[Byte; 10]`.
    // Let's assume the following setup with a full page,
    // where capital letters are fixed rows and lower case are variable.
    //
    // [ A, B, C, D, E, f, g, h, i, j ]
    //                  ^
    //               first_var
    //                  ^
    //               last_fixed
    //
    // The high water mark `first_var` points *at* the granule with the lowest offset (`f`).
    // Whereas `last_fixed` points *one past the end* (`f`) of the last fixed row (`E`)
    //
    // This is the case we have to consider in terms of possible underflow.
    // As both HWMs would point at the same place,
    // the result would be `0`, and no underflow occurs.
    Size((first_var - last_fixed).0)
}

/// Returns whether the remaining gap is large enough to host an object `fixed_row_size` large,
/// assuming `first_var` is the high water mark (HWM) of the var-len section,
/// pointing *at* the granule with the lowest offset,
/// and `last_fixed` is the HWM of the fixed-len section,
/// pointing *one past the end* of the last fixed row.
#[inline]
fn gap_enough_size_for_row(first_var: PageOffset, last_fixed: PageOffset, fixed_row_size: Size) -> bool {
    gap_remaining_size(first_var, last_fixed) >= fixed_row_size
}

impl Page {
    /// Returns a new page allocated on the heap.
    ///
    /// The new page supports a rows with `fixed_row_size`.
    pub fn new(fixed_row_size: Size) -> Box<Self> {
        Self::new_with_max_row_count(max_rows_in_page(fixed_row_size))
    }

    /// Returns a new page allocated on the heap.
    ///
    /// The new page supports `max_rows_in_page` at most.
    pub fn new_with_max_row_count(max_rows_in_page: usize) -> Box<Self> {
        // TODO(perf): mmap? allocator may do so already.
        // mmap may be more efficient as we save allocator metadata.
        use std::alloc::{alloc_zeroed, handle_alloc_error, Layout};

        let layout = Layout::new::<Page>();

        // Allocate with `alloc_zeroed` so that the bytes are initially 0, rather than uninit.
        // We will never write an uninit byte into the page except in the `PageHeader`,
        // so it is safe for `row_data` to have type `[u8; _]` rather than `[MaybeUninit<u8>; _]`.
        // `alloc_zeroed` may be more efficient than `alloc` + `memset`;
        // in particular, it may `mmap` pages directly from the OS, which are always zeroed for security reasons.
        // TODO: use Box::new_zeroed() once stabilized.
        // SAFETY: The layout's size is non-zero.
        let raw: *mut Page = unsafe { alloc_zeroed(layout) }.cast();

        if raw.is_null() {
            handle_alloc_error(layout);
        }

        // We need to initialize `Page::header`
        // without materializing a `&mut` as that is instant UB.
        // SAFETY: `raw` isn't NULL.
        let header = unsafe { &raw mut (*raw).header };

        // SAFETY: `header` is valid for writes as only we have exclusive access.
        //          The pointer is also aligned.
        unsafe { header.write(PageHeader::new(max_rows_in_page)) };

        // SAFETY: We used the global allocator with a layout for `Page`.
        //         We have initialized the `header`,
        //         and the `row_bytes` are initially 0 by `alloc_zeroed`,
        //         making the pointee a `Page` valid for reads and writes.
        unsafe { Box::from_raw(raw) }
    }

    /// Returns the number of rows stored in this page.
    ///
    /// This method runs in constant time.
    pub fn num_rows(&self) -> usize {
        self.header.fixed.num_rows as usize
    }

    #[cfg(test)]
    /// Use this page's present rows bitvec to compute the number of present rows.
    ///
    /// This can be compared with [`Self::num_rows`] as a consistency check during tests.
    pub fn reconstruct_num_rows(&self) -> usize {
        // If we cared, we could rewrite this to `u64::count_ones` on each block of the bitset.
        // We do not care. This method is slow.
        self.header.fixed.present_rows.iter_set().count()
    }

    /// Returns the number of var-len granules allocated in this page.
    ///
    /// This method runs in constant time.
    pub fn num_var_len_granules(&self) -> usize {
        self.header.var.num_granules as usize
    }

    #[cfg(test)]
    /// # Safety
    ///
    /// - `var_len_visitor` must be a valid [`VarLenMembers`] visitor
    ///   specialized to the type and layout of rows within this [`Page`].
    /// - `fixed_row_size` must be exactly the length in bytes of fixed rows in this page,
    ///   which must further be the length of rows expected by the `var_len_visitor`.
    pub unsafe fn reconstruct_num_var_len_granules(
        &self,
        fixed_row_size: Size,
        var_len_visitor: &impl VarLenMembers,
    ) -> usize {
        self.iter_fixed_len(fixed_row_size)
            .flat_map(|row| unsafe {
                // Safety: `row` came out of `iter_fixed_len`,
                // which, due to caller requirements on `fixed_row_size`,
                // is giving us valid, aligned, initialized rows of the row type.
                var_len_visitor.visit_var_len(self.get_row_data(row, fixed_row_size))
            })
            .flat_map(|var_len_obj| unsafe {
                // Safety: We believe `row` to be valid
                // and `var_len_visitor` to be correctly visiting its var-len members.
                // Therefore, `var_len_obj` is a valid var-len object.
                self.iter_var_len_object(var_len_obj.first_granule)
            })
            .count()
    }

    /// Returns the number of bytes used by rows stored in this page.
    ///
    /// This is necessarily an overestimate of live data bytes, as it includes:
    /// - Padding bytes within the fixed-length portion of the rows.
    /// - [`VarLenRef`] pointer-like portions of rows.
    /// - Unused trailing parts of partially-filled [`VarLenGranule`]s.
    /// - [`VarLenGranule`]s used to store [`BlobHash`]es.
    ///
    /// Note that large blobs themselves are not counted.
    /// The caller should obtain a count of the bytes used by large blobs
    /// from the [`super::blob_store::BlobStore`].
    ///
    /// This method runs in constant time.
    pub fn bytes_used_by_rows(&self, fixed_row_size: Size) -> usize {
        let fixed_row_bytes = self.num_rows() * fixed_row_size.len();
        let var_len_bytes = self.num_var_len_granules() * VarLenGranule::SIZE.len();
        fixed_row_bytes + var_len_bytes
    }

    #[cfg(test)]
    /// # Safety
    ///
    /// - `var_len_visitor` must be a valid [`VarLenMembers`] visitor
    ///   specialized to the type and layout of rows within this [`Page`].
    /// - `fixed_row_size` must be exactly the length in bytes of fixed rows in this page,
    ///   which must further be the length of rows expected by the `var_len_visitor`.
    pub unsafe fn reconstruct_bytes_used_by_rows(
        &self,
        fixed_row_size: Size,
        var_len_visitor: &impl VarLenMembers,
    ) -> usize {
        let fixed_row_bytes = self.reconstruct_num_rows() * fixed_row_size.len();
        let var_len_bytes = unsafe { self.reconstruct_num_var_len_granules(fixed_row_size, var_len_visitor) }
            * VarLenGranule::SIZE.len();
        fixed_row_bytes + var_len_bytes
    }

    /// Returns the range of row data starting at `offset` and lasting `size` bytes.
    pub fn get_row_data(&self, row: PageOffset, size: Size) -> &Bytes {
        &self.row_data[row.range(size)]
    }

    /// Returns whether the row at `offset` is present or not.
    pub fn has_row_offset(&self, fixed_row_size: Size, offset: PageOffset) -> bool {
        // Check that the `offset` is properly aligned for a row of size `fixed_row_size`.
        // This cannot be `debug_assert!` as the caller could rely on this
        // reporting properly whether `offset` is at a row boundary or not.
        assert_eq!(offset.idx() % fixed_row_size.len(), 0);

        self.header.fixed.is_row_present(offset, fixed_row_size)
    }

    /// Returns split mutable views of this page over the fixed and variable sections.
    pub fn split_fixed_var_mut(&mut self) -> (FixedView<'_>, VarView<'_>) {
        // The fixed HWM (`fixed.last`) points *one past the end* of the fixed section
        // which is exactly what we want for `split_at_mut`.
        let last_fixed = self.header.fixed.last;
        let (fixed_row_data, var_row_data) = self.row_data.split_at_mut(last_fixed.idx());

        // Construct the fixed-len view.
        let fixed = FixedView {
            fixed_row_data,
            header: &mut self.header.fixed,
        };

        // Construct the var-len view.
        let var = VarView {
            var_row_data,
            header: &mut self.header.var,
            last_fixed,
        };

        (fixed, var)
    }

    /// Returns a mutable view of the row from `start` lasting `fixed_row_size` number of bytes.
    ///
    /// This method is safe, but callers should take care that `start` and `fixed_row_size`
    /// are correct for this page, and that `start` is aligned.
    /// Callers should further ensure that mutations to the row leave the row bytes
    /// in an expected state, i.e. initialized where required by the row type,
    /// and with `VarLenRef`s that point to valid granules and with correct lengths.
    ///
    /// This call will clear the unmodified hash
    /// as it is expected that the caller will alter the the page.
    pub fn get_fixed_row_data_mut(&mut self, start: PageOffset, fixed_row_size: Size) -> &mut Bytes {
        self.header.unmodified_hash = None;
        &mut self.row_data[start.range(fixed_row_size)]
    }

    /// Return the total required var-len granules to store `objects`.
    pub fn total_granules_required_for_objects(objects: &[impl AsRef<[u8]>]) -> usize {
        objects
            .iter()
            .map(|obj| VarLenGranule::bytes_to_granules(obj.as_ref().len()).0)
            .sum()
    }

    /// Does the page have space to store a row,
    /// where the fixed size part is `fixed_row_size` bytes large,
    /// and the row has the given `var_len_objects`?
    pub fn has_space_for_row_with_objects(&self, fixed_row_size: Size, var_len_objects: &[impl AsRef<[u8]>]) -> bool {
        let num_granules_required = Self::total_granules_required_for_objects(var_len_objects);
        self.has_space_for_row(fixed_row_size, num_granules_required)
    }

    /// Does the page have space to store a row,
    /// where the fixed size part is `fixed_row_size` bytes large,
    /// and the variable part requires `num_granules`.
    pub fn has_space_for_row(&self, fixed_row_size: Size, num_granules: usize) -> bool {
        // Determine the gap remaining after allocating for the fixed part.
        let gap_remaining = gap_remaining_size(self.header.var.first, self.header.fixed.last);
        let gap_avail_for_granules = if self.header.fixed.next_free.has() {
            // If we have a free fixed length block, then we can use the whole gap for var-len granules.
            gap_remaining
        } else {
            // If we need to grow the fixed-length store into the gap,
            if gap_remaining < fixed_row_size {
                // if the gap is too small for fixed-length row, fail.
                return false;
            }
            // Otherwise, the space available in the gap for var-len granules
            // is the current gap size less the fixed-len row size.
            gap_remaining - fixed_row_size
        };

        // Convert the gap size to granules.
        let gap_in_granules = VarLenGranule::space_to_granules(gap_avail_for_granules);
        // Account for granules available in the freelist.
        let needed_granules_after_freelist = num_granules.saturating_sub(self.header.var.freelist_len as usize);

        gap_in_granules >= needed_granules_after_freelist
    }

    /// Returns whether the row is full with respect to storing a fixed row with `fixed_row_size`
    /// and no variable component.
    pub fn is_full(&self, fixed_row_size: Size) -> bool {
        !self.has_space_for_row(fixed_row_size, 0)
    }

    /// Will leave partially-allocated chunks if fails prematurely,
    /// so always check `Self::has_space_for_row` before calling.
    ///
    /// This method is provided for testing the page store directly;
    /// higher-level codepaths are expected to use [`crate::bflatn::ser::write_av_to_page`],
    /// which performs similar operations to this method,
    /// but handles rollback on failure appropriately.
    ///
    /// This function will never fail if `Self::has_space_for_row` has returned true.
    ///
    /// # Safety
    ///
    /// - `var_len_visitor` is suitable for visiting var-len refs in `fixed_row`.
    ///
    /// - `fixed_row.len()` must be consistent with `var_len_visitor` and `self`.
    ///   That is, `VarLenMembers` must be specialized for a row type with that length,
    ///   and all past, present, and future fixed-length rows stored in this `Page`
    ///   must also be of that length.
    pub unsafe fn insert_row(
        &mut self,
        fixed_row: &Bytes,
        var_len_objects: &[impl AsRef<[u8]>],
        var_len_visitor: &impl VarLenMembers,
        blob_store: &mut dyn BlobStore,
    ) -> Result<PageOffset, Error> {
        // Allocate the fixed-len row.
        let fixed_row_size = Size(fixed_row.len() as u16);

        // SAFETY: Caller promised that `fixed_row.len()` uses the right `fixed_row_size`
        // and we trust that others have too.
        let fixed_len_offset = unsafe { self.alloc_fixed_len(fixed_row_size)? };

        // Store the fixed-len row.
        let (mut fixed, mut var) = self.split_fixed_var_mut();
        let row = fixed.get_row_mut(fixed_len_offset, fixed_row_size);
        row.copy_from_slice(fixed_row);

        // Store all var-len refs into their appropriate slots in the fixed-len row.
        // SAFETY:
        // - The `fixed_len_offset` given by `alloc_fixed_len` resuls in `row`
        //   being properly aligned for the row type.
        // - Caller promised that `fixed_row.len()` matches the row type size exactly.
        // - `var_len_visitor` is suitable for `fixed_row`.
        let vlr_slot_iter = unsafe { var_len_visitor.visit_var_len_mut(row) };
        for (var_len_ref_slot, var_len_obj) in vlr_slot_iter.zip(var_len_objects) {
            let (var_len_ref, in_blob) = var.alloc_for_slice(var_len_obj.as_ref())?;
            if in_blob {
                // The blob store insertion will never fail.
                // SAFETY: `alloc_for_slice` always returns a pointer
                // to a `VarLenGranule` in bounds of this page.
                // As `in_blob` holds, it is also unused, as required.
                // We'll now make that granule valid.
                unsafe {
                    var.write_large_blob_hash_to_granule(blob_store, var_len_obj, var_len_ref);
                }
            }
            *var_len_ref_slot = var_len_ref;
        }

        Ok(fixed_len_offset)
    }

    /// Allocates space for a fixed size row of `fixed_row_size` bytes.
    ///
    /// # Safety
    ///
    /// `fixed_row_size` must be equal to the value passed
    /// to all other methods ever invoked on `self`.
    pub unsafe fn alloc_fixed_len(&mut self, fixed_row_size: Size) -> Result<PageOffset, Error> {
        self.alloc_fixed_len_from_freelist(fixed_row_size)
            .or_else(|| self.alloc_fixed_len_from_gap(fixed_row_size))
            .ok_or(Error::InsufficientFixedLenSpace { need: fixed_row_size })
    }

    /// Allocates a space for a fixed size row of `fixed_row_size` in the freelist, if possible.
    ///
    /// This call will clear the unmodified hash.
    #[inline]
    fn alloc_fixed_len_from_freelist(&mut self, fixed_row_size: Size) -> Option<PageOffset> {
        let header = &mut self.header.fixed;
        // SAFETY: `header.next_free` points to a `FreeCellRef` when the former `.has()`.
        let free = unsafe { header.next_free.take_freelist_head(&self.row_data, |x| x) }?;
        header.set_row_present(free, fixed_row_size);

        // We are and have modified the page, so clear the unmodified hash.
        self.header.unmodified_hash = None;

        Some(free)
    }

    /// Allocates a space for a fixed size row of `fixed_row_size` in the freelist, if possible.
    ///
    /// This call will clear the unmodified hash.
    #[inline]
    fn alloc_fixed_len_from_gap(&mut self, fixed_row_size: Size) -> Option<PageOffset> {
        if gap_enough_size_for_row(self.header.var.first, self.header.fixed.last, fixed_row_size) {
            // We're modifying the page, so clear the unmodified hash.
            self.header.unmodified_hash = None;

            // Enough space in the gap; move the high water mark and return the old HWM.
            // `fixed.last` points *after* the highest-indexed fixed-len row,
            // so post-increment.
            let ptr = self.header.fixed.last;
            self.header.fixed.last += fixed_row_size;
            self.header.fixed.set_row_present(ptr, fixed_row_size);
            Some(ptr)
        } else {
            // Not enough space in the gap for another row!
            None
        }
    }

    /// Returns an iterator over all the [`PageOffset`]s of the fixed rows in this page
    /// beginning with `starting_from`.
    ///
    /// The rows are assumed to be `fixed_row_size` bytes long
    /// and `starting_from` is assumed to be at a valid starting `PageOffset` for a fixed row.
    ///
    /// NOTE: This method is not `unsafe` as it cannot trigger UB.
    /// However, when provided with garbage input, it will return garbage back.
    /// It is the caller's responsibility to ensure that `PageOffset`s derived from
    /// this iterator are valid when used to do anything `unsafe`.
    fn iter_fixed_len_from(&self, fixed_row_size: Size, starting_from: PageOffset) -> FixedLenRowsIter<'_> {
        let idx = starting_from / fixed_row_size;
        FixedLenRowsIter {
            idx_iter: self.header.fixed.present_rows.iter_set_from(idx),
            fixed_row_size,
        }
    }

    /// Returns an iterator over all the [`PageOffset`]s of the fixed rows in this page.
    ///
    /// The rows are assumed to be `fixed_row_size` bytes long.
    ///
    /// NOTE: This method is not `unsafe` as it cannot trigger UB.
    /// However, when provided with garbage input, it will return garbage back.
    /// It is the caller's responsibility to ensure that `PageOffset`s derived from
    /// this iterator are valid when used to do anything `unsafe`.
    pub fn iter_fixed_len(&self, fixed_row_size: Size) -> FixedLenRowsIter<'_> {
        FixedLenRowsIter {
            idx_iter: self.header.fixed.present_rows.iter_set(),
            fixed_row_size,
        }
    }

    /// Returns an iterator over all the `VarLenGranule`s of the var-len object
    /// that has its first granule at offset `first_granule`.
    /// An empty iterator will be returned when `first_granule` is `NULL`.
    ///
    /// # Safety
    ///
    /// `first_granule` must be an offset to a valid granule or `NULL`.
    pub unsafe fn iter_var_len_object(
        &self,
        first_granule: PageOffset,
    ) -> impl Clone + Iterator<Item = &VarLenGranule> {
        VarLenGranulesIter {
            page: self,
            next_granule: first_granule,
        }
    }

    /// Returns an iterator over the data of all the `VarLenGranule`s of the var-len object
    /// that has its first granule at offset `first_granule`.
    /// An empty iterator will be returned when `first_granule` is `NULL`.
    ///
    /// # Safety
    ///
    /// `first_granule` must be an offset to a valid granule or `NULL`.
    pub unsafe fn iter_vlo_data(&self, first_granule: PageOffset) -> impl '_ + Clone + Iterator<Item = &[u8]> {
        // SAFETY: Caller and callee have the exact same safety requirements.
        unsafe { self.iter_var_len_object(first_granule) }.map(|g| g.data())
    }

    /// Free a row, marking its fixed-len and var-len storage granules as available for re-use.
    ///
    /// This call will clear the unmodified hash.
    ///
    /// # Safety
    ///
    /// - `fixed_row` must point to a valid row in this page.
    ///
    /// - `fixed_row_size` must be the size in bytes of the fixed part
    ///   of all past, present, and future rows in this page and future rows in this page.
    ///
    /// - The `var_len_visitor` must visit the same set of `VarLenRef`s in the row
    ///   as the visitor provided to `insert_row`.
    pub unsafe fn delete_row(
        &mut self,
        fixed_row: PageOffset,
        fixed_row_size: Size,
        var_len_visitor: &impl VarLenMembers,
        blob_store: &mut dyn BlobStore,
    ) -> BlobNumBytes {
        // We're modifying the page, so clear the unmodified hash.
        self.header.unmodified_hash = None;

        let (mut fixed, mut var) = self.split_fixed_var_mut();

        let mut blob_store_deleted_bytes = BlobNumBytes::default();

        // Visit the var-len members of the fixed row and free them.
        let row = fixed.get_row(fixed_row, fixed_row_size);
        // SAFETY: `row` is derived from `fixed_row`, which is known by caller requirements to be valid.
        let var_len_refs = unsafe { var_len_visitor.visit_var_len(row) };
        for var_len_ref in var_len_refs {
            // SAFETY: A sound call to `visit_var_len` on a fully initialized valid row,
            // which we've justified that the above is,
            // returns an iterator, that will only yield `var_len_ref`s,
            // where `var_len_ref.first_granule` points to a valid `VarLenGranule` or is NULL.
            blob_store_deleted_bytes += unsafe { var.free_object(*var_len_ref, blob_store) }
        }

        // SAFETY: Caller promised that `fixed_row` points to a valid row in the page.
        // Thus, `range_move(0..fixed_row_size, fixed_row)` is in bounds of `row_data`.
        // Moreover, this entails that it is valid for writing a `FreeCellRef`
        // to the beginning or entire range, as any row can at least hold a `FreeCellRef`
        // and will be properly aligned for it as well.
        unsafe {
            fixed.free(fixed_row, fixed_row_size);
        }

        blob_store_deleted_bytes
    }

    /// Returns the total number of granules used by the fixed row at `fixed_row_offset`
    /// and lasting `fixed_row_size` bytes where `var_len_visitor` is used to find
    /// the [`VarLenRef`]s in the fixed row.
    ///
    /// # Safety
    ///
    /// - `fixed_row_offset` must refer to a previously-allocated and initialized row in `self`,
    ///   and must not have been de-allocated. In other words, the fixed row must be *valid*.
    ///
    /// - `fixed_row_size` and `var_len_visitor` must be consistent with each other
    ///   and with all other calls to any methods on `self`.
    pub unsafe fn row_total_granules(
        &self,
        fixed_row_offset: PageOffset,
        fixed_row_size: Size,
        var_len_visitor: &impl VarLenMembers,
    ) -> usize {
        let fixed_row = self.get_row_data(fixed_row_offset, fixed_row_size);
        // SAFETY:
        // - Caller promised that `fixed_row_offset` is a valid row.
        // - Caller promised consistency of `var_len_visitor` wrt. `fixed_row_size` and this page.
        let vlr_iter = unsafe { var_len_visitor.visit_var_len(fixed_row) };
        vlr_iter.copied().map(|slot| slot.granules_used()).sum()
    }

    /// Copy as many rows from `self` for which `filter` returns `true` into `dst` as will fit,
    /// starting from `starting_from`.
    ///
    /// If less than the entirety of `self` could be processed, return `Continue(resume_point)`,
    /// where `resume_point` is the `starting_from` argument of a subsequent call to `copy_filter_into`
    /// that will complete the iteration.
    /// `dst` should be assumed to be full in this case,
    /// as it does not contain enough free space to store the row of `self` at `resume_point`.
    ///
    /// If the entirety of `self` is processed, return `Break`.
    /// `dst` may or may not be full in this case, but is likely not full.
    ///
    /// # Safety
    ///
    /// The `var_len_visitor` must visit the same set of `VarLenRef`s in the row
    /// as the visitor provided to all other methods on `self` and `dst`.
    ///
    /// The `fixed_row_size` must be consistent with the `var_len_visitor`,
    /// and be equal to the value provided to all other methods on `self` and `dst`.
    ///
    /// The `starting_from` offset must point to a valid starting offset
    /// consistent with `fixed_row_size`.
    /// That is, it must not point into the middle of a row.
    pub unsafe fn copy_filter_into(
        &self,
        starting_from: PageOffset,
        dst: &mut Page,
        fixed_row_size: Size,
        var_len_visitor: &impl VarLenMembers,
        blob_store: &mut dyn BlobStore,
        mut filter: impl FnMut(&Page, PageOffset) -> bool,
    ) -> ControlFlow<(), PageOffset> {
        for row_offset in self
            .iter_fixed_len_from(fixed_row_size, starting_from)
            // Only copy rows satisfying the predicate `filter`.
            .filter(|o| filter(self, *o))
        {
            // SAFETY:
            // - `starting_from` points to a valid row and thus `row_offset` also does.
            // - `var_len_visitor` will visit the right `VarLenRef`s and is consistent with other calls.
            // - `fixed_row_size` is consistent with `var_len_visitor` and `self`.
            if !unsafe { self.copy_row_into(row_offset, dst, fixed_row_size, var_len_visitor, blob_store) } {
                // Target doesn't have enough space for row;
                // stop here and return the offset of the uncopied row
                // so a later call to `copy_filter_into` can start there.
                return ControlFlow::Continue(row_offset);
            }
        }

        // The `for` loop completed.
        // We successfully copied the entire page of `self` into `target`.
        // The caller doesn't need to resume from this offset.
        ControlFlow::Break(())
    }

    /// Copies the row at `row_offset` from `self` into `dst`
    /// or returns `false` otherwise if `dst` has no space for the row.
    ///
    /// # Safety
    ///
    /// - `row_offset` offset must point to a valid row.
    ///
    /// - `var_len_visitor` must visit the same set of `VarLenRef`s in the row
    ///   as the visitor provided to all other methods on `self` and `dst`.
    ///
    /// - `fixed_row_size` must be consistent with the `var_len_visitor`,
    ///   and be equal to the value provided to all other methods on `self` and `dst`.
    unsafe fn copy_row_into(
        &self,
        row_offset: PageOffset,
        dst: &mut Page,
        fixed_row_size: Size,
        var_len_visitor: &impl VarLenMembers,
        blob_store: &mut dyn BlobStore,
    ) -> bool {
        // SAFETY: Caller promised that `starting_from` points to a valid row
        // consistent with `fixed_row_size` which was also
        // claimed to be consistent with `var_len_visitor` and `self`.
        let required_granules = unsafe { self.row_total_granules(row_offset, fixed_row_size, var_len_visitor) };
        if !dst.has_space_for_row(fixed_row_size, required_granules) {
            // Target doesn't have enough space for row.
            return false;
        };

        let src_row = self.get_row_data(row_offset, fixed_row_size);

        // Allocate for the fixed-len data.
        // SAFETY: forward our requirement on `fixed_row_size` to `alloc_fixed_len`.
        let inserted_offset = unsafe { dst.alloc_fixed_len(fixed_row_size) }
            .expect("Failed to allocate fixed-len row in dst page after checking for available space");

        // Copy all fixed-len data. We'll overwrite the var-len parts next.
        let (mut dst_fixed, mut dst_var) = dst.split_fixed_var_mut();
        let dst_row = dst_fixed.get_row_mut(inserted_offset, fixed_row_size);
        dst_row.copy_from_slice(src_row);

        // Copy var-len members into target.
        // Fixup `VarLenRef`s in `dst_row` to point to the copied var-len objects.
        //
        // SAFETY: `src_row` is valid because it came from `self.iter_fixed_len_from`.
        //
        //         Forward our safety requirements re: `var_len_visitor` to `visit_var_len`.
        let src_vlr_iter = unsafe { var_len_visitor.visit_var_len(src_row) };
        // SAFETY: forward our requirement on `var_len_visitor` to `visit_var_len_mut`.
        let target_vlr_iter = unsafe { var_len_visitor.visit_var_len_mut(dst_row) };
        for (src_vlr, target_vlr_slot) in src_vlr_iter.zip(target_vlr_iter) {
            // SAFETY:
            //
            // - requirements of `visit_var_len_assume_init` were met,
            //   so we can assume that `src_vlr.first_granule` points to a valid granule or is NULL.
            //
            // - the call to `dst.has_space_for_row` above ensures
            //   that the allocation will not fail part-way through.
            let target_vlr_fixup = unsafe { self.copy_var_len_into(*src_vlr, &mut dst_var, blob_store) }
                .expect("Failed to allocate var-len object in dst page after checking for available space");

            *target_vlr_slot = target_vlr_fixup;
        }

        true
    }

    /// Copy a var-len object `src_vlr` from `self` into `dst_var`,
    /// and return the `VarLenRef` to the copy in `dst_var`.
    ///
    /// If the `src_vlr` is empty,
    /// i.e., has `first_granule.is_null()` and `length_in_bytes == 0`,
    /// this will return `VarLenRef::NULL`.
    ///
    /// # SAFETY:
    ///
    /// - `src_vlr.first_granule` must point to a valid granule or be NULL.
    ///
    /// - To avoid leaving dangling uninitialized allocations in `dst_var`,
    ///   `dst_var` must already be checked to have enough size to store `src_vlr`
    ///   using `Self::has_space_for_row`.
    unsafe fn copy_var_len_into(
        &self,
        src_vlr: VarLenRef,
        dst_var: &mut VarView<'_>,
        blob_store: &mut dyn BlobStore,
    ) -> Result<VarLenRef, Error> {
        // SAFETY: Caller promised that `src_vlr.first_granule` points to a valid granule is be NULL.
        let mut iter = unsafe { self.iter_var_len_object(src_vlr.first_granule) };

        // If the `src_vlr` is empty, don't copy anything, and return null.
        let Some(mut src_chunk) = iter.next() else {
            debug_assert!(src_vlr.length_in_bytes == 0);
            return Ok(VarLenRef::NULL);
        };
        let mut dst_chunk = dst_var.alloc_granule()?;

        let copied_head = dst_chunk;

        // Weird-looking iterator so we can put the next-pointer into `copied_chunk`.
        for next_src_chunk in iter {
            // Allocate space for the next granule so we can initialize it in the next iteration.
            let next_dst_chunk = dst_var.alloc_granule()?;
            let data = src_chunk.data();
            // Initialize `dst_chunk` with data and next-pointer.
            //
            // SAFETY:
            // 1. `dst_chunk` is properly aligned as it came from `alloc_granule` either
            //    before the loop or in the previous iteration.
            //    This also ensures that both are in bounds
            //    of the page for `granule + granule + VarLenGranule::SIZE`.
            //
            // 2. `next_dst_chunk` will be initialized
            //    either in the next iteration or after the loop ends.
            //
            // 3. `dst_chunk` points to unused data as the space was allocated before the loop
            //    or was `next_dst_chunk` in the previous iteration and hasn't been written to yet.
            unsafe { dst_var.write_chunk_to_granule(data, data.len(), dst_chunk, next_dst_chunk) };
            dst_chunk = next_dst_chunk;
            src_chunk = next_src_chunk;
        }

        let data = src_chunk.data();
        // The last granule has null as next-pointer.
        //
        // SAFETY:
        // 1. `dst_chunk` is properly aligned as it came from `alloc_granule` either
        //    before the loop or in the previous iteration.
        //    This also ensures that both are in bounds
        //    of the page for `granule + granule + VarLenGranule::SIZE`.
        //
        // 2. `next` is NULL which is trivially init.
        //
        // 3. `dst_chunk` points to unused data as the space was allocated before the loop
        //    or was `next_dst_chunk` in the previous iteration and hasn't been written to yet.
        unsafe { dst_var.write_chunk_to_granule(data, data.len(), dst_chunk, PageOffset::VAR_LEN_NULL) };

        // For a large blob object,
        // notify the `blob_store` that we've taken a reference to the blob hash.
        if src_vlr.is_large_blob() {
            blob_store
                .clone_blob(&src_chunk.blob_hash())
                .expect("blob_store could not mark hash as used");
        }

        Ok(VarLenRef {
            first_granule: copied_head,
            length_in_bytes: src_vlr.length_in_bytes,
        })
    }

    /// Make `self` empty, removing all rows from it and resetting the high water marks to zero.
    ///
    /// This also clears the `unmodified_hash`.
    pub fn clear(&mut self) {
        self.header.clear();
    }

    /// Zeroes every byte of row data in this page.
    ///
    /// # Safety
    ///
    /// Causes the page header to no longer match the contents, invalidating many assumptions.
    /// Should be called in conjunction with [`Self::clear`].
    pub unsafe fn zero_data(&mut self) {
        self.row_data.fill(0);
    }

    /// Resets this page for reuse of its allocation.
    ///
    /// The reset page supports `max_rows_in_page` at most.
    pub fn reset_for(&mut self, max_rows_in_page: usize) {
        self.header.reset_for(max_rows_in_page);

        // NOTE(centril): We previously zeroed pages when resetting.
        // This had an adverse performance impact.
        // The reason why we previously zeroed was for security under a multi-tenant setup
        // when exposing a module ABI that allows modules to memcpy whole pages over.
        // However, we have no such ABI for the time being, so we can soundly avoid zeroing.
        // If we ever decide to add such an ABI, we must start zeroing again.
        //
        // // SAFETY: We just reset the page header.
        // unsafe { self.zero_data() };
    }

    /// Sets the header and the row data.
    ///
    /// # Safety
    ///
    /// The `header` and `row_data` must be consistent with each other.
    pub(super) unsafe fn set_raw(&mut self, header: PageHeader, row_data: RowData) {
        self.header = header;
        self.row_data = row_data;
    }

    /// Returns the page header, for testing.
    #[cfg(test)]
    pub(super) fn page_header_for_test(&self) -> &PageHeader {
        &self.header
    }

    /// Computes the content hash of this page.
    pub fn content_hash(&self) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();

        // Hash the page contents.
        hasher.update(&self.row_data);

        // Hash the `FixedHeader`, first copy out the fixed part save for the bitset into an array.
        let fixed = &self.header.fixed;
        let mut fixed_bytes = [0u8; 6];
        fixed_bytes[0..2].copy_from_slice(&fixed.next_free.next.0.to_le_bytes());
        fixed_bytes[2..4].copy_from_slice(&fixed.last.0.to_le_bytes());
        fixed_bytes[4..6].copy_from_slice(&fixed.num_rows.to_le_bytes());
        hasher.update(&fixed_bytes);

        // Hash the fixed bit set.
        hasher.update(bytemuck::must_cast_slice(fixed.present_rows.storage()));

        // Hash the `VarHeader`.
        hasher.update(bytemuck::bytes_of(&self.header.var));

        // We're done.
        // Note that `unmodified_hash` itself must not be hashed to avoid a recursive dependency.
        hasher.finalize()
    }

    /// Computes the content hash of this page and saves it to [`PageHeader::unmodified_hash`].
    pub fn save_content_hash(&mut self) {
        let hash = self.content_hash();
        self.header.unmodified_hash = Some(hash);
    }

    /// Return the page's content hash, computing and saving it if it is not already stored.
    pub fn save_or_get_content_hash(&mut self) -> blake3::Hash {
        self.unmodified_hash().copied().unwrap_or_else(|| {
            self.save_content_hash();
            self.header.unmodified_hash.unwrap()
        })
    }

    /// Returns the stored unmodified hash, if any.
    pub fn unmodified_hash(&self) -> Option<&blake3::Hash> {
        self.header.unmodified_hash.as_ref()
    }
}

/// An iterator over the `PageOffset`s of all present fixed-length rows in a [`Page`].
pub struct FixedLenRowsIter<'page> {
    /// The fixed header of the page,
    /// used to determine where the last fixed row is
    /// and whether the fixed row slot is actually a fixed row.
    idx_iter: IterSet<'page>,
    /// The size of a row in bytes.
    fixed_row_size: Size,
}

impl Iterator for FixedLenRowsIter<'_> {
    type Item = PageOffset;

    fn next(&mut self) -> Option<Self::Item> {
        self.idx_iter
            .next()
            .map(|idx| PageOffset(idx as u16 * self.fixed_row_size.0))
    }
}

/// An iterator over the [`VarLenGranule`]s in a particular [`VarLenRef`] in `page`.
///
/// Constructing a `VarLenGranulesIter` should be considered unsafe
/// because the initial `next_granule` must either be `NULL` or point to a valid [`VarLenGranule`].
///
/// Iterating over [`VarLenRef::NULL`] is safe and will immediately return `None`.
#[derive(Clone, Copy)]
struct VarLenGranulesIter<'page> {
    /// The page to yield granules from.
    page: &'page Page,
    /// Location of the next granule in `page`.
    /// Must either be `NULL` or point to a valid granule.
    next_granule: PageOffset,
    // TODO(perf,bikeshedding): store length and implement `Iterator::size_hint`?
}

impl<'page> Iterator for VarLenGranulesIter<'page> {
    type Item = &'page VarLenGranule;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_granule.is_var_len_null() {
            return None;
        }

        // SAFETY: By construction,
        // the initial `next_granule` was promised to either be `NULL` or point to a valid granule.
        // For a given granule, the same applies to its `.next()` granule.
        // At this point, we've excluded `NULL`,
        // so we know inductively that `next_granule` points to a valid granule, as required.
        let granule: &VarLenGranule = unsafe { get_ref(&self.page.row_data, self.next_granule) };
        self.next_granule = granule.header.next();

        Some(granule)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        blob_store::NullBlobStore, layout::row_size_for_type, page_pool::PagePool, var_len::AlignedVarLenOffsets,
    };
    use proptest::{collection::vec, prelude::*};
    use spacetimedb_lib::bsatn;

    fn u64_row_size() -> Size {
        let fixed_row_size = row_size_for_type::<u64>();
        assert_eq!(fixed_row_size.len(), 8);
        fixed_row_size
    }

    const U64_VL_VISITOR: AlignedVarLenOffsets<'_> = AlignedVarLenOffsets::from_offsets(&[]);
    fn u64_var_len_visitor() -> &'static AlignedVarLenOffsets<'static> {
        &U64_VL_VISITOR
    }

    fn insert_u64(page: &mut Page, val: u64) -> PageOffset {
        let val_slice = val.to_le_bytes();
        unsafe { page.insert_row(&val_slice, &[] as &[&[u8]], u64_var_len_visitor(), &mut NullBlobStore) }
            .expect("Failed to insert first row")
    }

    fn insert_u64_drop(page: &mut Page, val: u64) {
        insert_u64(page, val);
    }

    fn read_u64(page: &Page, offset: PageOffset) -> u64 {
        let row = page.get_row_data(offset, u64_row_size());
        u64::from_le_bytes(row.try_into().unwrap())
    }

    fn data_sub_n_vlg(n: usize) -> usize {
        PageOffset::PAGE_END.idx() - (VarLenGranule::SIZE * n).len()
    }

    pub(crate) fn hash_unmodified_save_get(page: &mut Page) -> blake3::Hash {
        assert_eq!(page.header.unmodified_hash, None);
        page.save_content_hash();
        page.header.unmodified_hash.unwrap()
    }

    #[test]
    fn insert_one_u64() {
        let mut page = Page::new(u64_row_size());

        // First the hash is not saved, so compute it.
        let hash = hash_unmodified_save_get(&mut page);

        let val: u64 = 0xa5a5_a5a5_a5a5_a5a5;

        let offset = insert_u64(&mut page, val);

        assert_eq!(offset.idx(), 0);

        let row_val = read_u64(&page, offset);

        assert_eq!(row_val, val);

        // The hash should have been cleared.
        assert_ne!(hash, hash_unmodified_save_get(&mut page));
    }

    fn insert_while(
        page: &mut Page,
        mut next_val: u64,
        fixed_row_size: Size,
        vl_num: usize,
        mut insert: impl FnMut(&mut Page, u64),
    ) -> u64 {
        while page.has_space_for_row(fixed_row_size, vl_num) {
            insert(page, next_val);
            next_val += 1;
        }
        next_val
    }

    #[test]
    fn fill_then_iter_fixed_len_u64() {
        let mut page = Page::new(u64_row_size());

        let last_val = insert_while(&mut page, 0, u64_row_size(), 0, insert_u64_drop);
        assert_eq!(last_val, (PageOffset::PAGE_END / u64_row_size()) as u64);

        for (row_idx, expected_val) in page.iter_fixed_len(u64_row_size()).zip(0..last_val) {
            let row_val = read_u64(&page, row_idx);
            assert_eq!(
                row_val, expected_val,
                "row_val {:x} /= expected_val {:x}",
                row_val, expected_val
            );
        }
    }

    #[test]
    fn fill_delete_iter_fixed_len_u64() {
        let mut page = Page::new(u64_row_size());

        // First the hash is not saved, so compute it.
        let hash_pre_ins = hash_unmodified_save_get(&mut page);

        // Insert rows.
        let mut odds: Vec<PageOffset> = Vec::new();
        let last_val = insert_while(&mut page, 2, u64_row_size(), 0, |page, val| {
            let offset = insert_u64(page, val);
            if val % 2 == 1 {
                odds.push(offset);
            }
        });

        // The hash should have been cleared.
        let hash_pre_del = hash_unmodified_save_get(&mut page);
        assert_ne!(hash_pre_ins, hash_pre_del);

        // Delete rows.
        for row_offset in odds {
            unsafe { page.delete_row(row_offset, u64_row_size(), u64_var_len_visitor(), &mut NullBlobStore) };
        }

        // The hash should have been cleared.
        let hash_pre_iter = hash_unmodified_save_get(&mut page);
        assert_ne!(hash_pre_ins, hash_pre_iter);
        assert_ne!(hash_pre_del, hash_pre_iter);

        // Iterate the rows.
        for (row_offset, expected_val) in page.iter_fixed_len(u64_row_size()).zip((2..last_val).step_by(2)) {
            let found_val = read_u64(&page, row_offset);
            assert_eq!(found_val, expected_val);
        }

        // Hash is unchanged.
        assert_eq!(page.header.unmodified_hash, Some(hash_pre_iter));
    }

    #[test]
    /// After deleting a fixed-length row and then inserting a new fixed-length row,
    /// the fixed-length high water mark must not change,
    /// i.e. we must re-use memory from the deleted row to store the new insertion.
    fn reuse_fixed_len_space() {
        let mut page = Page::new(u64_row_size());

        // First the hash is not saved, so compute it.
        let hash_pre_ins = hash_unmodified_save_get(&mut page);

        // Insert two rows.
        let offset_0 = insert_u64(&mut page, 0xa5a5a5a5_a5a5a5a5);
        assert_eq!(offset_0.idx(), 0);
        let offset_1 = insert_u64(&mut page, 0xbeefbeef_beefbeef);
        assert_eq!(offset_1, u64_row_size());

        assert_eq!(page.header.fixed.last, u64_row_size() * 2);

        // Hash has been cleared after inserts.
        let hash_pre_del = hash_unmodified_save_get(&mut page);
        assert_ne!(hash_pre_ins, hash_pre_del);

        // Delete first row.
        unsafe { page.delete_row(offset_0, u64_row_size(), u64_var_len_visitor(), &mut NullBlobStore) };

        assert_eq!(page.header.fixed.last, u64_row_size() * 2);

        // Hash has been cleared after deletes.
        let hash_pre_ins2 = hash_unmodified_save_get(&mut page);
        assert_ne!(hash_pre_ins, hash_pre_ins2);
        assert_ne!(hash_pre_del, hash_pre_ins2);

        // Insert first row again, re-using memory.
        let offset_0_again = insert_u64(&mut page, 0xffffffff_ffffffff);

        assert_eq!(offset_0_again.idx(), 0);
        assert_eq!(offset_0.idx(), offset_0_again.idx());

        assert_eq!(page.header.fixed.last, u64_row_size() * 2);

        // Hash has been cleared after last insert, despite re-using memory.
        let hash_post_ins2 = hash_unmodified_save_get(&mut page);
        assert_ne!(hash_pre_ins, hash_post_ins2);
        assert_ne!(hash_pre_del, hash_post_ins2);
        assert_ne!(hash_pre_ins2, hash_post_ins2);
    }

    const STR_ROW_SIZE: Size = row_size_for_type::<VarLenRef>();

    const _: () = assert!(STR_ROW_SIZE.len() == mem::size_of::<VarLenRef>());

    const STR_VL_VISITOR: AlignedVarLenOffsets<'_> = AlignedVarLenOffsets::from_offsets(&[0]);
    fn str_var_len_visitor() -> &'static AlignedVarLenOffsets<'static> {
        &STR_VL_VISITOR
    }

    fn insert_str(page: &mut Page, data: &[u8]) -> PageOffset {
        let fixed_len_data = [0u8; STR_ROW_SIZE.len()];
        unsafe { page.insert_row(&fixed_len_data, &[data], str_var_len_visitor(), &mut NullBlobStore) }
            .expect("Failed to insert row")
    }

    fn read_str_ref(page: &Page, offset: PageOffset) -> VarLenRef {
        *unsafe { get_ref(&page.row_data, offset) }
    }

    #[test]
    fn insert_empty_str() {
        let mut page = Page::new(STR_ROW_SIZE);

        // First the hash is not saved, so compute it.
        let hash_pre_ins = hash_unmodified_save_get(&mut page);

        // Insert the empty string.
        let offset = insert_str(&mut page, &[]);

        // No granules were used.
        let extracted = read_str_ref(&page, offset);
        let mut granules_iter = unsafe { page.iter_var_len_object(extracted.first_granule) };
        assert!(granules_iter.next().is_none());
        drop(granules_iter);

        // Hash is cleared even though the string was empty.
        assert_ne!(hash_pre_ins, hash_unmodified_save_get(&mut page));
    }

    proptest! {
        #[test]
        fn insert_one_short_str(data in vec(any::<u8>(), 1..VarLenGranule::DATA_SIZE)) {
            let mut page = Page::new(STR_ROW_SIZE);

            // First the hash is not saved, so compute it.
            let hash_pre_ins = hash_unmodified_save_get(&mut page);

            // Insert the row.
            let offset = insert_str(&mut page, &data);

            // Hash was cleared by the insert.
            let hash_pre_iter = hash_unmodified_save_get(&mut page);
            assert_ne!(hash_pre_ins, hash_pre_iter);

            // Confirm we inserted correctly.
            let extracted = read_str_ref(&page, offset);
            let mut data_iter = unsafe { page.iter_vlo_data(extracted.first_granule) };
            let (first, next) = (data_iter.next(), data_iter.next());
            assert_eq!(first, Some(&*data));
            assert_eq!(next, None);

            // Iteration and reading did not change the hash.
            assert_eq!(hash_pre_iter, page.header.unmodified_hash.unwrap());
        }

        #[test]
        fn insert_one_long_str(data in vec(any::<u8>(), (VarLenGranule::OBJECT_SIZE_BLOB_THRESHOLD / 2)..VarLenGranule::OBJECT_SIZE_BLOB_THRESHOLD)) {
            let mut page = Page::new(STR_ROW_SIZE);

            // First the hash is not saved, so compute it.
            let hash_pre_ins = hash_unmodified_save_get(&mut page);

            // Insert the long string.
            let offset = insert_str(&mut page, &data);

            // The hash was cleared, and the new one is different.
            let hash_post_ins = hash_unmodified_save_get(&mut page);
            assert_ne!(hash_pre_ins, hash_post_ins);

            // Check that we inserted correctly.
            let extracted = read_str_ref(&page, offset);

            let mut data_iter = unsafe { page.iter_vlo_data(extracted.first_granule) };
            let mut chunks_iter = data.chunks(VarLenGranule::DATA_SIZE);

            for (i, (data, chunk)) in (&mut data_iter).zip(&mut chunks_iter).enumerate() {
                assert_eq!(
                    data,
                    chunk,
                    "Chunk {} does not match. Left is found, right is expected.",
                    i,
                );
            }

            // Both iterators must be finished, i.e. they must have the same length.
            assert!(data_iter.next().is_none());
            assert!(chunks_iter.next().is_none());

            // Reading did not alter the hash.
            assert_eq!(hash_post_ins, page.header.unmodified_hash.unwrap());
        }
    }

    #[test]
    fn reuse_var_len_space_no_fragmentation_concerns() {
        let data_0 = b"Hello, world!";
        let data_1 = b"How goes life?";
        let data_2 = b"Glad to hear it.";

        let mut page = Page::new(STR_ROW_SIZE);
        let offset_0 = insert_str(&mut page, data_0);
        let offset_1 = insert_str(&mut page, data_1);

        assert_eq!(page.header.var.first.idx(), data_sub_n_vlg(2));

        assert_ne!(offset_0.idx(), offset_1.idx());

        let var_len_0 = read_str_ref(&page, offset_0);

        assert_eq!(var_len_0.length_in_bytes as usize, data_0.len());
        assert_eq!(var_len_0.first_granule.idx(), data_sub_n_vlg(1));

        let var_len_1 = read_str_ref(&page, offset_1);

        assert_eq!(var_len_1.length_in_bytes as usize, data_1.len());
        assert_eq!(var_len_1.first_granule.idx(), data_sub_n_vlg(2));

        let hash_pre_del = hash_unmodified_save_get(&mut page);

        unsafe { page.delete_row(offset_0, STR_ROW_SIZE, str_var_len_visitor(), &mut NullBlobStore) };

        let hash_pre_ins = hash_unmodified_save_get(&mut page);

        let offset_2 = insert_str(&mut page, data_2);

        let hash_post_ins = hash_unmodified_save_get(&mut page);
        assert_ne!(hash_pre_del, hash_pre_ins);
        assert_ne!(hash_pre_del, hash_post_ins);
        assert_ne!(hash_pre_ins, hash_post_ins);

        assert_eq!(page.header.var.first.idx(), data_sub_n_vlg(2));

        assert_eq!(offset_0.idx(), offset_2.idx());

        let var_len_2 = read_str_ref(&page, offset_2);

        assert_eq!(var_len_2.length_in_bytes as usize, data_2.len());
        assert_eq!(var_len_2.first_granule.idx(), var_len_0.first_granule.idx());
    }

    #[test]
    fn free_var_len_obj_multiple_granules() {
        let mut page = Page::new(STR_ROW_SIZE);

        // Allocate a 4-granule var-len object.
        let data_0 = [0xa5u8].repeat(VarLenGranule::DATA_SIZE * 4);
        let offset_0 = insert_str(&mut page, &data_0);

        let var_len_0 = read_str_ref(&page, offset_0);

        // Read the addresses of its var-len granules.
        let granules_0 = unsafe { page.iter_var_len_object(var_len_0.first_granule) }
            .map(|granule| granule as *const VarLenGranule as usize)
            .collect::<Vec<_>>();

        // Sanity checks: we have allocated 4 granules.
        assert_eq!(granules_0.len(), 4);
        assert_eq!(page.header.var.first.idx(), data_sub_n_vlg(4));

        // Delete the row.
        unsafe { page.delete_row(offset_0, STR_ROW_SIZE, str_var_len_visitor(), &mut NullBlobStore) };

        // Allocate a new 4-granule var-len object.
        // This should use the same storage as the original row.
        let data_1 = [0xffu8].repeat(VarLenGranule::DATA_SIZE * 4);
        let offset_1 = insert_str(&mut page, &data_1);

        let var_len_1 = read_str_ref(&page, offset_1);

        // Read the addresses of the new allocation's var-len granules.
        let granules_1 = unsafe { page.iter_var_len_object(var_len_1.first_granule) }
            .map(|granule| granule as *const VarLenGranule as usize)
            .collect::<Vec<_>>();

        // Sanity check: the new allocation is also 4 granules.
        assert_eq!(granules_1.len(), 4);

        for granule in granules_1.iter().copied() {
            // The new var-len allocation must contain all the same granules by address
            // as the old var-len allocation.
            assert!(granules_0.iter().copied().any(|other_granule| other_granule == granule));
        }

        // The var-len high water mark must not have moved.
        assert_eq!(page.header.var.first.idx(), data_sub_n_vlg(4));
    }

    #[test]
    fn reuse_var_len_space_avoid_fragmentation() {
        let data_0 = &[0xa5u8];
        let data_1 = &[0xffu8];
        let data_2 = [0x11u8].repeat(VarLenGranule::DATA_SIZE + 1);
        let data_2 = data_2.as_ref();

        let mut page = Page::new(STR_ROW_SIZE);

        // First the hash is not saved, so compute it.
        let hash_pre_ins = hash_unmodified_save_get(&mut page);

        // Insert two string rows.
        let offset_0 = insert_str(&mut page, data_0);
        let _offset_1 = insert_str(&mut page, data_1);

        assert_eq!(page.header.var.first.idx(), data_sub_n_vlg(2));

        // Hash is cleared by inserting and the new one is different.
        let hash_pre_del = hash_unmodified_save_get(&mut page);
        assert_ne!(hash_pre_ins, hash_pre_del);

        // Delete the first row.
        unsafe { page.delete_row(offset_0, STR_ROW_SIZE, str_var_len_visitor(), &mut NullBlobStore) };

        // Hash is cleared by deleting.
        let hash_post_del = hash_unmodified_save_get(&mut page);
        assert_ne!(hash_pre_ins, hash_post_del);
        assert_ne!(hash_pre_del, hash_post_del);

        // Insert again, re-using memory.
        let offset_2 = insert_str(&mut page, data_2);

        assert_eq!(page.header.var.first.idx(), data_sub_n_vlg(3));

        // Hash is cleared by inserting again, even though we re-used memory.
        let hash_post_ins2 = hash_unmodified_save_get(&mut page);
        assert_ne!(hash_pre_ins, hash_post_ins2);
        assert_ne!(hash_pre_del, hash_post_ins2);
        assert_ne!(hash_post_del, hash_post_ins2);

        // Check that we inserted correctly.
        let var_len_2 = read_str_ref(&page, offset_2);

        let mut data_iter = unsafe { page.iter_vlo_data(var_len_2.first_granule) };
        let mut chunks_iter = data_2.chunks(VarLenGranule::DATA_SIZE);

        for (i, (data, chunk)) in (&mut data_iter).zip(&mut chunks_iter).enumerate() {
            assert_eq!(
                data, chunk,
                "Chunk {} does not match. Left is found, right is expected.",
                i,
            );
        }

        // Both iterators must be finished, i.e. they must have the same length.
        assert!(data_iter.next().is_none());
        assert!(chunks_iter.next().is_none());
    }

    fn check_u64_in_str(page: &Page, row_idx: PageOffset, expected_val: u64) {
        let vlr = read_str_ref(page, row_idx);

        let mut var_len_iter = unsafe { page.iter_vlo_data(vlr.first_granule) };
        let data = var_len_iter.next().unwrap();
        assert!(var_len_iter.next().is_none());
        assert_eq!(data.len(), mem::size_of::<u64>());

        let val = u64::from_le_bytes(data.try_into().unwrap());
        assert_eq!(val, expected_val);
    }

    #[test]
    fn fill_then_iter_var_len_str() {
        let mut page = Page::new(STR_ROW_SIZE);

        // First the hash is not saved, so compute it.
        let hash_pre_ins = hash_unmodified_save_get(&mut page);

        // Insert the strings.
        let last_val = insert_while(&mut page, 0, STR_ROW_SIZE, 1, |page, val| {
            insert_str(page, &val.to_le_bytes());
        });

        // Hash is cleared by inserting and the new one is different.
        let hash_pre_iter = hash_unmodified_save_get(&mut page);
        assert_ne!(hash_pre_ins, hash_pre_iter);

        // Check that we inserted correctly.
        let size_per_row = STR_ROW_SIZE + VarLenGranule::SIZE;

        assert_eq!(last_val, (PageOffset::PAGE_END / size_per_row) as u64);

        for (row_idx, expected_val) in page.iter_fixed_len(STR_ROW_SIZE).zip(0..last_val) {
            check_u64_in_str(&page, row_idx, expected_val);
        }

        // Reading does not alter the hash.
        assert_eq!(hash_pre_iter, page.header.unmodified_hash.unwrap());
    }

    #[test]
    fn fill_delete_iter_var_len_str() {
        let mut page = Page::new(STR_ROW_SIZE);

        // First the hash is not saved, so compute it.
        let hash_pre_ins = hash_unmodified_save_get(&mut page);

        // Insert the string rows.
        let mut odds = Vec::new();
        let last_val = insert_while(&mut page, 0, STR_ROW_SIZE, 1, |page, val| {
            let offset = insert_str(page, &val.to_le_bytes());
            if val % 2 == 1 {
                odds.push(offset);
            }
        });

        let size_per_row = STR_ROW_SIZE + VarLenGranule::SIZE;
        let num_rows_inserted = (PageOffset::PAGE_END / size_per_row) as u64;
        assert_eq!(last_val, num_rows_inserted);

        // Hash was cleared by inserting and is different now.
        let hash_pre_del = hash_unmodified_save_get(&mut page);
        assert_ne!(hash_pre_ins, hash_pre_del);

        // Delete the rows.
        for row_offset in odds {
            unsafe { page.delete_row(row_offset, STR_ROW_SIZE, str_var_len_visitor(), &mut NullBlobStore) };
        }

        // Hash was cleared by deleting and is different now.
        let hash_pre_iter = hash_unmodified_save_get(&mut page);
        assert_ne!(hash_pre_ins, hash_pre_iter);
        assert_ne!(hash_pre_del, hash_pre_iter);

        // Check that we deleted correctly.
        let num_rows_retained = num_rows_inserted.div_ceil(2);
        let num_rows_removed = num_rows_inserted / 2;

        assert_eq!(page.header.fixed.num_rows as u64, num_rows_retained);

        assert_eq!(page.header.var.freelist_len as u64, num_rows_removed);

        for (row_idx, expected_val) in page.iter_fixed_len(STR_ROW_SIZE).zip((0..last_val).step_by(2)) {
            check_u64_in_str(&page, row_idx, expected_val);
        }

        // Reading did not alter the hash.
        assert_eq!(hash_pre_iter, page.header.unmodified_hash.unwrap());
    }

    #[test]
    fn serde_round_trip_whole_page() {
        let pool = PagePool::new_for_test();
        let mut page = Page::new(u64_row_size());

        // Construct an empty page, ser/de it, and assert that it's still empty.
        let hash_pre_ins = hash_unmodified_save_get(&mut page);
        let ser_pre_ins = bsatn::to_vec(&page).unwrap();
        let de_pre_ins = pool.take_deserialize_from(&ser_pre_ins).unwrap();
        assert_eq!(de_pre_ins.content_hash(), hash_pre_ins);
        assert_eq!(de_pre_ins.header.fixed.num_rows, 0);
        assert!(de_pre_ins.header.fixed.present_rows == page.header.fixed.present_rows);

        // Insert some rows into the page.
        let offsets = (0..64)
            .map(|val| insert_u64(&mut page, val))
            .collect::<Vec<PageOffset>>();

        let hash_ins = hash_unmodified_save_get(&mut page);

        // Ser/de the page and assert that it contains the same rows.
        let ser_ins = bsatn::to_vec(&page).unwrap();
        let de_ins = pool.take_deserialize_from(&ser_ins).unwrap();
        assert_eq!(de_ins.content_hash(), hash_ins);
        assert_eq!(de_ins.header.fixed.num_rows, 64);
        assert!(de_ins.header.fixed.present_rows == page.header.fixed.present_rows);
        assert_eq!(
            de_ins.iter_fixed_len(u64_row_size()).collect::<Vec<PageOffset>>(),
            offsets
        );

        // Delete the even-numbered rows, leaving the odds.
        let offsets = offsets
            .into_iter()
            .enumerate()
            .filter_map(|(i, offset)| {
                if i % 2 == 0 {
                    unsafe { page.delete_row(offset, u64_row_size(), u64_var_len_visitor(), &mut NullBlobStore) };
                    None
                } else {
                    Some(offset)
                }
            })
            .collect::<Vec<PageOffset>>();

        // Ser/de the page again and assert that it contains only the odd-numbered rows.
        let hash_del = hash_unmodified_save_get(&mut page);
        let ser_del = bsatn::to_vec(&page).unwrap();
        let de_del = pool.take_deserialize_from(&ser_del).unwrap();
        assert_eq!(de_del.content_hash(), hash_del);
        assert_eq!(de_del.header.fixed.num_rows, 32);
        assert!(de_del.header.fixed.present_rows == page.header.fixed.present_rows);
        assert_eq!(
            de_del.iter_fixed_len(u64_row_size()).collect::<Vec<PageOffset>>(),
            offsets
        );
    }
}
