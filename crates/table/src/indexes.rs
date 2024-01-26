//! Provides primitive types and definitions around
//! bytes, row hashes, (page) sizes, offsets, and indices.

use super::util::range_move;
use crate::static_assert_size;
use ahash::RandomState;
use core::fmt;
use core::mem::MaybeUninit;
use core::ops::{AddAssign, Div, Mul, Range, SubAssign};
use derive_more::{Add, Sub};
use nohash_hasher::IsEnabled;

/// A byte is a possibly uninit `u8`.
pub type Byte = MaybeUninit<u8>;

/// A slice of [`Byte`]s.
pub type Bytes = [Byte];

/// Total size of a page, incl. header.
///
/// Defined as 64 KiB.
pub(crate) const PAGE_SIZE: usize = u16::MAX as usize + 1;

/// Total size of a page header.
///
/// 64 as the header is aligned to 64 bytes.
pub(crate) const PAGE_HEADER_SIZE: usize = 64;

/// The size of the data segment of a [`Page`](super::page::Page).
///
/// Currently 64KiB - 64 bytes.
/// The 64 bytes are used for the header of the `Page`.
// pub for benchmarks
pub const PAGE_DATA_SIZE: usize = PAGE_SIZE - PAGE_HEADER_SIZE;

/// The content hash of a row.
///
/// Notes:
/// - The hash is not cryptographically secure.
///
/// - The hash is valid only for the lifetime of a `Table`.
///   This entails that it should not be persisted to disk
///   or used as a stable identifier over the network.
///   For example, the hashing algorithm could be different
///   on different machines based on availability of hardware instructions.
///   Moreover, due to random seeds, when restarting from disk,
///   the hashes may be different for the same rows.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(any(test, feature = "proptest"), derive(proptest_derive::Arbitrary))]
pub struct RowHash(pub u64);

static_assert_size!(RowHash, 8);

/// `RowHash` is already a hash, so no need to hash again.
impl IsEnabled for RowHash {}

impl RowHash {
    /// Returns a `Hasher` builder that yields the type of hashes that `RowHash` stores.
    pub fn hasher_builder() -> RandomState {
        // For equal `row`s, all calls within the same process will yield the same hash.
        RandomState::with_seed(0x42)
    }
}

/// The size of something in page storage in bytes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Add, Sub)]
pub struct Size(pub u16);

impl Size {
    /// Returns the size for use in `usize` computations.
    #[inline]
    #[allow(clippy::len_without_is_empty)]
    pub const fn len(self) -> usize {
        self.0 as usize
    }
}

impl Mul<usize> for Size {
    type Output = Size;

    #[inline]
    fn mul(self, rhs: usize) -> Self::Output {
        Size((self.len() * rhs) as u16)
    }
}

/// An offset into a [`Page`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Add, Sub)]
#[cfg_attr(any(test, feature = "proptest"), derive(proptest_derive::Arbitrary))]
pub struct PageOffset(
    #[cfg_attr(any(test, feature = "proptest"), proptest(strategy = "0..PageOffset::PAGE_END.0"))] pub u16,
);

static_assert_size!(PageOffset, 2);

impl PageOffset {
    /// Returns the offset as a `usize` index.
    #[inline]
    pub const fn idx(self) -> usize {
        self.0 as usize
    }

    /// Is this offset the null offset, which refers to an empty var-len object?
    ///
    /// The null `PageOffset` is used as a sentinel in `VarLenRef`s and `VarLenGranule`s
    /// as the empty list.
    /// `VAR_LEN_NULL` can never refer to an allocated `VarLenGranule`,
    /// because the existence of a `VarLenGranule` implies the existence of at least one fixed-len row,
    /// which means the fixed-len high water mark is strictly greater than zero.
    ///
    /// Note that the null `PageOffset` refers to a valid fixed-len row slot.
    /// It may only be used as a sentinel value when referring to var-len allocations.
    #[inline]
    pub const fn is_var_len_null(self) -> bool {
        self.0 == Self::VAR_LEN_NULL.0
    }

    /// The null offset, pointing to the beginning of a page.
    ///
    /// The null `PageOffset` is used as a sentinel in `VarLenRef`s and `VarLenGranule`s
    /// as the empty list.
    /// `VAR_LEN_NULL` can never refer to an allocated `VarLenGranule`,
    /// because the existence of a `VarLenGranule` implies the existence of at least one fixed-len row,
    /// which means the fixed-len high water mark is strictly greater than zero.
    ///
    /// Note that the null `PageOffset` refers to a valid fixed-len row slot.
    /// It may only be used as a sentinel value when referring to var-len allocations.
    pub const VAR_LEN_NULL: Self = Self(0);

    /// Is this offset at the [`PageOffset::PAGE_END`]?
    #[inline]
    pub const fn is_at_end(self) -> bool {
        self.0 == Self::PAGE_END.0
    }

    /// The offset one past the end of the page.
    /// That is, for `row_data: [Byte; PAGE_DATA_SIZE]`, this is `row_data.len()`.
    ///
    /// This also means that `PAGE_END` is **not** a page offset
    /// that can be used for indexing, but it can be used as a sentinel.
    pub const PAGE_END: Self = Self(PAGE_DATA_SIZE as u16);

    /// Returns a range from this offset lasting `size` bytes.
    #[inline]
    pub const fn range(self, size: Size) -> Range<usize> {
        range_move(0..size.len(), self.idx())
    }
}

impl PartialEq<Size> for PageOffset {
    #[inline]
    fn eq(&self, other: &Size) -> bool {
        self.0 == other.0
    }
}

impl AddAssign<Size> for PageOffset {
    #[inline]
    fn add_assign(&mut self, rhs: Size) {
        self.0 += rhs.0;
    }
}

impl SubAssign<Size> for PageOffset {
    #[inline]
    fn sub_assign(&mut self, rhs: Size) {
        self.0 -= rhs.0;
    }
}

impl Div<Size> for PageOffset {
    type Output = usize;

    #[inline]
    fn div(self, size: Size) -> Self::Output {
        self.idx() / size.len()
    }
}

impl fmt::LowerHex for PageOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

/// The index of a [`Page`] within a [`Pages`].
#[cfg_attr(any(test, feature = "proptest"), derive(proptest_derive::Arbitrary))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PageIndex(#[cfg_attr(any(test, feature = "proptest"), proptest(strategy = "0..MASK_PI"))] pub u64);

static_assert_size!(PageIndex, 8);

impl PageIndex {
    /// The maximum page index, currently `(1 << 39) - 1`.
    ///
    /// Limiting `PageIndex` to 39 bits allows it to be packed into `RowPointer`'s 64 bits
    /// alongside a `PageOffset`, a `SquashedOffset` and a reserved bit.
    pub const MAX: Self = Self(MASK_PI);

    /// Returns this index as a `usize`.
    #[inline]
    pub const fn idx(self) -> usize {
        self.0 as usize
    }
}

/// Indicates which version of a `Table` is referred to by a `RowPointer`.
///
/// Currently, `SquashedOffset` has two meaningful values,
/// [`SquashedOffset::TX_STATE`] and [`SquashedOffset::COMMITTED_STATE`],
/// which refer to the TX scratchpad and the committed state respectively.
///
/// In the future, `SquashedOffset` will be extended to capture
/// which savepoint within a transaction the pointer refers to,
/// or which committed-unsquashed preceding transaction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(any(test, feature = "proptest"), derive(proptest_derive::Arbitrary))]
pub struct SquashedOffset(pub u8);

static_assert_size!(SquashedOffset, 1);

impl SquashedOffset {
    /// Does this `SquahsedOffset` refer to the TX scratchpad?
    #[inline]
    pub const fn is_tx_state(self) -> bool {
        self.0 == Self::TX_STATE.0
    }

    /// The `SquashedOffset` for the TX scratchpad.
    pub const TX_STATE: Self = Self(0);

    /// Does this `SquashedOffset` refer to the committed (squashed) state?
    #[inline]
    pub const fn is_committed_state(self) -> bool {
        self.0 == Self::COMMITTED_STATE.0
    }

    /// The `SquashedOffset` for the committed (squashed) state.
    pub const COMMITTED_STATE: Self = Self(1);
}

/// Offset to a buffer inside `Pages` referring
/// to the index of a specific page
/// and the offset within the page.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowPointer(pub u64);

static_assert_size!(RowPointer, 8);

// Offsets and bits for the various components of `RowPointer`.
const OFFSET_RB: u64 = 0;
const BITS_RB: u64 = 1;
// ^-- Bit 1 is reserved so it can be used in `OffsetOrCollider` ("reserved bit").
const OFFSET_PI: u64 = OFFSET_RB + BITS_RB;
const BITS_PI: u64 = 39;
const OFFSET_PO: u64 = OFFSET_PI + BITS_PI;
const BITS_PO: u64 = 16;
const OFFSET_SQ: u64 = OFFSET_PO + BITS_PO;
const BITS_SQ: u64 = 8;

// Extracting masks for the various components of `RowPointer`.
const MASK_RB: u64 = (1 << BITS_RB) - 1;
const MASK_PI: u64 = (1 << BITS_PI) - 1;
const MASK_PO: u64 = (1 << BITS_PO) - 1;
const MASK_SQ: u64 = (1 << BITS_SQ) - 1;

// Zeroing masks for the various components of `RowPointer`.
const MASK_ZERO_RB: u64 = !(MASK_RB << OFFSET_RB);
const MASK_ZERO_PI: u64 = !(MASK_PI << OFFSET_PI);
const MASK_ZERO_PO: u64 = !(MASK_PO << OFFSET_PO);
const MASK_ZERO_SQ: u64 = !(MASK_SQ << OFFSET_SQ);

impl RowPointer {
    /// Returns a row pointer that is at the given `page_offset`,
    /// in the page with `page_index`,
    /// and with the `squashed_offset` (savepoint offset).
    #[inline(always)]
    pub const fn new(
        reserved_bit: bool,
        page_index: PageIndex,
        page_offset: PageOffset,
        squashed_offset: SquashedOffset,
    ) -> Self {
        Self(0)
            .with_reserved_bit(reserved_bit)
            .with_squashed_offset(squashed_offset)
            .with_page_index(page_index)
            .with_page_offset(page_offset)
    }

    /// Returns the reserved bit.
    #[inline(always)]
    pub const fn reserved_bit(self) -> bool {
        ((self.0 >> OFFSET_RB) & MASK_RB) != 0
    }

    /// Returns the index of the page.
    #[inline(always)]
    pub const fn page_index(self) -> PageIndex {
        PageIndex((self.0 >> OFFSET_PI) & MASK_PI)
    }

    /// Returns the offset within the page.
    #[inline(always)]
    pub const fn page_offset(self) -> PageOffset {
        PageOffset(((self.0 >> OFFSET_PO) & MASK_PO) as u16)
    }

    /// Returns the squashed offset, i.e., the savepoint offset.
    #[inline(always)]
    pub const fn squashed_offset(self) -> SquashedOffset {
        SquashedOffset(((self.0 >> OFFSET_SQ) & MASK_SQ) as u8)
    }

    /// Returns a new row pointer
    /// with its reserved bit changed to `reserved_bit`.
    #[inline(always)]
    pub const fn with_reserved_bit(self, reserved_bit: bool) -> Self {
        Self::with(self, reserved_bit as u64, MASK_RB, OFFSET_RB, MASK_ZERO_RB)
    }

    /// Returns a new row pointer
    /// with its `PageIndex` changed to `page_index`.
    #[inline(always)]
    pub const fn with_page_index(self, page_index: PageIndex) -> Self {
        Self::with(self, page_index.0, MASK_PI, OFFSET_PI, MASK_ZERO_PI)
    }

    /// Returns a new row pointer
    /// with its `PageOffset` changed to `page_offset`.
    #[inline(always)]
    pub const fn with_page_offset(self, page_offset: PageOffset) -> Self {
        Self::with(self, page_offset.0 as u64, MASK_PO, OFFSET_PO, MASK_ZERO_PO)
    }

    /// Returns a new row pointer
    /// with its `SquashedOffset` changed to `squashed_offset`.
    #[inline(always)]
    pub const fn with_squashed_offset(self, squashed_offset: SquashedOffset) -> Self {
        Self::with(self, squashed_offset.0 as u64, MASK_SQ, OFFSET_SQ, MASK_ZERO_SQ)
    }

    #[inline(always)]
    const fn with(self, v: u64, mask: u64, offset: u64, zero: u64) -> Self {
        let vmoved = (v & mask) << offset;
        Self((self.0 & zero) | vmoved)
    }
}

impl fmt::Debug for RowPointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RowPointer(r: {:?}, pi: {:?}, po: {:?}, so: {:?})",
            self.reserved_bit() as u8,
            self.page_index().idx(),
            self.page_offset().idx(),
            self.squashed_offset().0,
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn row_pointer_ops_work(
            ((rb1, pi1, po1, so1), (rb2, pi2, po2, so2)) in (
                (any::<bool>(), any::<PageIndex>(), any::<PageOffset>(), any::<SquashedOffset>()),
                (any::<bool>(), any::<PageIndex>(), any::<PageOffset>(), any::<SquashedOffset>()),
        )) {
            let check = |rb, pi, po, so, ptr: RowPointer| {
                prop_assert_eq!(rb, ptr.reserved_bit());
                prop_assert_eq!(pi, ptr.page_index());
                prop_assert_eq!(po, ptr.page_offset());
                prop_assert_eq!(so, ptr.squashed_offset());
                Ok(())
            };
            let ptr = RowPointer::new(rb1, pi1, po1, so1);
            check(rb1, pi1, po1, so1, ptr)?;
            check(rb2, pi1, po1, so1, ptr.with_reserved_bit(rb2))?;
            check(rb1, pi2, po1, so1, ptr.with_page_index(pi2))?;
            check(rb1, pi1, po2, so1, ptr.with_page_offset(po2))?;
            check(rb1, pi1, po1, so2, ptr.with_squashed_offset(so2))?;
        }
    }
}
