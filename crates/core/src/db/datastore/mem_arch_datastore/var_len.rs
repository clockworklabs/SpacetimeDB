//! Provides the definitions of [`VarLenRef`], [`VarLenGranule`], and [`VarLenMembers`].

use super::{
    blob_store::BlobHash,
    indexes::{Byte, Bytes, PageOffset, Size},
    util::slice_assume_init_ref,
};
use crate::{static_assert_align, static_assert_size};
use core::iter;
use core::marker::PhantomData;
use core::mem::{self, MaybeUninit};

/// Reference to var-len object within a page.
// TODO: make this larger and do short-string optimization?
// - Or store a few elts inline and then a `VarLenRef`?
// - Or first store `VarLenRef` that records num inline elements (remaining inline are uninit)
//  (bitfield; only need 10 bits for `len_in_bytes`)?
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(C)]
pub struct VarLenRef {
    /// The length of the var-len object in bytes.
    /// When `self.is_large_blob()` returns true,
    /// this is not the proper length of the object.
    /// Rather, the blob store must be consulted for the true length.
    pub length_in_bytes: u16,
    /// The offset to the first granule containing some of the object's data
    /// as well a pointer to the next granule.
    pub first_granule: PageOffset,
}

// Implementation of `super::page::VarLenMembers` depends on
// `size = 4` and `align = 2` of `VarLenRef`.
static_assert_size!(VarLenRef, 4);
static_assert_align!(VarLenRef, 2);

impl VarLenRef {
    /// Does this refer to a large blob object
    /// where `self.first_granule` is a blob hash?
    #[inline]
    pub const fn is_large_blob(self) -> bool {
        self.length_in_bytes == Self::LARGE_BLOB_SENTINEL
    }

    /// The sentinel for a var-len ref to a large blob.
    pub const LARGE_BLOB_SENTINEL: u16 = u16::MAX;

    /// Returns a var-len ref for a large blob object.
    #[inline]
    pub const fn large_blob(first_granule: PageOffset) -> Self {
        Self {
            length_in_bytes: Self::LARGE_BLOB_SENTINEL,
            first_granule,
        }
    }

    /// Returns the number of granules this var-len ref uses in it page.
    #[inline]
    pub const fn granules_used(&self) -> usize {
        VarLenGranule::bytes_to_granules(self.length_in_bytes as usize).0
    }

    /// Is this reference NULL?
    #[inline]
    pub const fn is_null(self) -> bool {
        self.first_granule.is_null()
    }

    /// The NULL var-len reference for empty variable components.
    ///
    /// A null `VarLenRef` can occur when a row has no var-len component
    /// or needs to point to one that is empty.
    pub const NULL: Self = Self {
        length_in_bytes: 0,
        first_granule: PageOffset::NULL,
    };
}

const _BLOB_SENTINEL_MORE_THAN_MAX_OBJ_SIZE: () =
    assert!(VarLenGranule::OBJECT_SIZE_BLOB_THRESHOLD < VarLenRef::LARGE_BLOB_SENTINEL as usize);

const _GRANULES_USED_FOR_BLOB_IS_CONSISTENT: () = {
    let vlr = VarLenRef::large_blob(PageOffset::NULL);
    assert!(vlr.is_large_blob() == (vlr.granules_used() == 1));
};

/// Returns whether `offset` is properly aligned for storing a [`VarLenGranule`].
pub fn is_granule_offset_aligned(offset: PageOffset) -> bool {
    offset.0 == offset.0 & VarLenGranuleHeader::NEXT_BITMASK
}

/// The header of a [`VarLenGranule`] storing
/// - (low 6 bits) the number of bytes the granule contains
/// - (high 10 bits) the offset of the next granule in the linked-list
///   used to store an object in variable storage.
///
/// For efficiency, this data is packed as a bitfield
/// in a `u16` with bits used per above.
#[derive(Copy, Clone)]
pub struct VarLenGranuleHeader(u16);

// TODO(centril): consider flagging in the granule header that it is a blobstore ref.
// We could do this by using `VarLenGranuleHeader::LEN_BITMASK` (63) to signal
// "is blob-stored" as `VarLenGranule::DATA_SIZE` == 62. So 63 is currently dead state.

impl VarLenGranuleHeader {
    /// The total size of a variable granule's header in bytes.
    const SIZE: usize = mem::size_of::<Self>();

    /// The number of bits used to store the `len` of a [`VarLenGranule`] is 6.
    const LEN_BITS: u16 = 6;

    /// The `len` of a [`VarLenGranule`] is stored in the low 6 bits.
    ///
    /// The 6 bits are enough to store at most `2^6` (`64`).
    /// However, a granule can never store more than [`VarLenGranule::DATA_SIZE`] (`62`),
    /// which is `2` less than `2^6`.
    const LEN_BITMASK: u16 = (1 << Self::LEN_BITS) - 1;

    /// The [`LEN_BITMASK`] will preserve all granule lengths possible.
    const _ASSERT_LEN_BITMASK_FITS_ALL_POSSIBLE_GRANULE_LENGTHS: () =
        assert!(VarLenGranule::DATA_SIZE <= Self::LEN_BITMASK as usize);

    // The `next` of a `VarLenGranule` is stored in the high 10 bits.
    // It is not shifted; the low 6 bits will always be 0 due to alignment.
    const NEXT_BITMASK: u16 = !Self::LEN_BITMASK;

    /// Returns a new header with the length component changed to `len`.
    fn with_len(self, len: u8) -> Self {
        // Zero any previous `len` field.
        let mut new = self;
        new.0 &= !Self::LEN_BITMASK;

        // Ensure that the `len` doesn't overflow into the `next`.
        let capped_len = (len as u16) & Self::LEN_BITMASK;
        debug_assert_eq!(
            capped_len, len as u16,
            "Len {} overflows the length of a `VarLenGranule`",
            len
        );

        // Insert the truncated `len`.
        new.0 |= capped_len;

        debug_assert_eq!(self.next(), new.next(), "`set_len` has modified `next`");
        debug_assert_eq!(
            new.len() as u16,
            capped_len,
            "`set_len` has not inserted the correct `len`: expected {:x}, found {:x}",
            capped_len,
            new.len()
        );

        new
    }

    /// Returns a new header with the next-granule component changed to `next`.
    fn with_next(self, PageOffset(next): PageOffset) -> Self {
        let mut new = self;

        // Zero any previous `next` field.
        new.0 &= !Self::NEXT_BITMASK;

        // Ensure that the `next` is aligned,
        // and therefore doesn't overwrite any of the `len`.
        let aligned_next = next & Self::NEXT_BITMASK;
        debug_assert_eq!(aligned_next, next, "Next {:x} is unaligned", next);

        // Insert the aligned `next`.
        new.0 |= aligned_next;

        debug_assert_eq!(self.len(), new.len(), "`set_next` has modified `len`");
        debug_assert_eq!(
            new.next().0,
            aligned_next,
            "`set_next` has not inserted the correct `next`"
        );

        new
    }

    /// Returns a new header for a granule storing `len` bytes
    /// and with the next granule in the list located `next`.
    pub fn new(len: u8, next: PageOffset) -> Self {
        // TODO: check for large rows and error,
        //       to signal that we should use the blob store instead of pages?
        //       Or otherwise handle large fixed-len rows.
        Self(0).with_len(len).with_next(next)
    }

    /// Returns the number of bytes the granule contains.
    const fn len(&self) -> u8 {
        (self.0 & Self::LEN_BITMASK) as u8
    }

    /// Returns the offset / Address of the next granule in the linked-list.
    pub const fn next(&self) -> PageOffset {
        PageOffset(self.0 & Self::NEXT_BITMASK)
    }
}

/// Each variable length object in a page is stored as a linked-list of chunks.
/// These chunks are called *granules* and they can store up to 62 bytes of `data`.
/// Additionally, 2 bytes are used for the [`header: VarLenGranuleHeader`](VarLenGranuleHeader).
#[repr(C)] // Required for a stable ABI.
#[repr(align(64))] // Alignment must be same as `VarLenGranule::SIZE`.
pub struct VarLenGranule {
    /// The header of the granule, containing the length and the next-cell offset.
    pub header: VarLenGranuleHeader,
    /// The data storing some part, or whole, of the var-len object.
    pub data: [Byte; Self::DATA_SIZE],
}

impl VarLenGranule {
    /// The total size of a variable length granule in bytes.
    pub const SIZE: Size = Size(64);

    /// The size, in bytes, of the data section of a variable length granule.
    pub const DATA_SIZE: usize = Self::SIZE.len() - VarLenGranuleHeader::SIZE;

    /// The max number of granules an object can use
    /// before being put into large blob storage.
    pub const OBJECT_MAX_GRANULES_BEFORE_BLOB: usize = 16;

    /// The max size of an object before being put into large blob storage.
    pub const OBJECT_SIZE_BLOB_THRESHOLD: usize = Self::DATA_SIZE * Self::OBJECT_MAX_GRANULES_BEFORE_BLOB;

    /// Returns the number of granules that would fit into `available_len`.
    pub const fn space_to_granules(available_len: Size) -> usize {
        // Floor division (the default div operator) here
        // to ensure we don't allocate e.g., a 64-byte granule in a 63-byte space.
        available_len.len() / Self::SIZE.len()
    }

    /// Returns the number of granules needed to store an object of `len_in_bytes` in size.
    /// Also returns whether the object needs to go into the blob store.
    pub const fn bytes_to_granules(len_in_bytes: usize) -> (usize, bool) {
        if len_in_bytes > VarLenGranule::OBJECT_SIZE_BLOB_THRESHOLD {
            // If `obj` is large enough to go in the blob store,
            // you require space for a blob-hash,
            // rather than the whole object.
            // A blob hash fits in a single granule as SHA-256 needs 32 bytes < 62 bytes.
            (1, true)
        } else {
            // Using `div_ceil` here to ensure over- rather than under-allocation.
            (len_in_bytes.div_ceil(Self::DATA_SIZE), false)
        }
    }

    /// Chunks `bytes` into an iterator where each element fits into a granule.
    pub fn chunks(bytes: &[u8]) -> impl DoubleEndedIterator<Item = &[u8]> {
        bytes.chunks(Self::DATA_SIZE)
    }

    /// Returns the data from the var-len object in this granule.
    pub fn data(&self) -> &[u8] {
        // TODO: Does this need to return `&Byte`, in case we store padding bytes?
        //       If we put non-byte-string arrays in a `VarLenGranule`,
        //       we may have structures in there which include padding bytes.
        let len = self.header.len() as usize;
        let slice = &self.data[0..len];

        // SAFETY: Assuming we've determined that we never store `uninit` padding bytes in a var-len object,
        //         the paths that construct a `VarLenGranule` always initialize the bytes up to the length.
        unsafe { slice_assume_init_ref(slice) }
    }

    /// Assumes that the granule stores a [`BlobHash`] and returns it.
    ///
    /// Panics if the assumption is wrong.
    pub fn blob_hash(&self) -> BlobHash {
        self.data().try_into().unwrap()
    }
}

/// A single [`VarLenGranule`] is needed to store a [`BlobHash`].
#[allow(clippy::assertions_on_constants)]
const _VLG_CAN_STORE_BLOB_HASH: () = assert!(VarLenGranule::DATA_SIZE >= BlobHash::SIZE);

/// A visitor object which can iterate over the var-len slots in a row.
///
/// Each var-len visitor is specialized to a particular row type,
/// though implementors of `VarLenMembers` decide whether this specialization
/// is per instance or per type.
///
/// The trivial implementor of `VarLenMembers` is [`AlignedVarLenOffsets`],
/// which stores the offsets of var-len members in a particular row type in a slice,
/// and uses pointer arithmetic to return references to them.
pub trait VarLenMembers {
    /// The iterator type returned by [`VarLenMembers::visit_var_len`].
    type Iter<'this, 'row>: Iterator<Item = &'row MaybeUninit<VarLenRef>>
    where
        Self: 'this;

    /// The iterator type returned by [`VarLenMembers::visit_var_len_mut`].
    type IterMut<'this, 'row>: Iterator<Item = &'row mut MaybeUninit<VarLenRef>>
    where
        Self: 'this;

    /// Treats `row` as storage for a row of the particular type handled by `self`,
    /// and iterates over the (possibly uninitialized) `VarLenRef`s within it.
    ///
    /// Callers are responsible for maintaining whether var-len members have been initialized.
    ///
    /// # Safety
    ///
    /// - `row` must be properly aligned for the row type.
    ///   This alignment constraint should be defined (and documented!)
    ///   by the implementor of `VarLenMembers`.
    ///
    /// - `row` must further be a slice of exactly the number of bytes of the row type.
    ///   Implementors may or may not check this property via `debug_assert!`,
    ///   but callers *must always* ensure it for safety.
    ///   These invariants allow us to construct references to [`VarLenRef`]s inside the slice.
    ///
    ///   Note that `Itererator::next` is a safe function,
    ///   so it must always be valid to advance an iterator to it end.
    ///
    /// - All callers of `visit_var_len` on a particular `row`
    ///   must visit the same set of `VarLenRef`s in the same order,
    ///   though they may do so through different implementors of `VarLenMembers`.
    ///   E.g. it would be valid to use an `AlignedVarLenOffsets` to initialize a row,
    ///   then later read from it using a hypothetical optimized JITted visitor,
    ///   provided the JITted visitor visited the same set of offsets.
    ///
    /// - `Self::visit_var_len` and `Self::visit_var_len_mut`
    ///   must visit the same set of `VarLenRef`s in the same order.
    ///   Various consumers in `Page` and friends depend on this and the previous requirement.
    unsafe fn visit_var_len_mut<'this, 'row>(&'this self, row: &'row mut Bytes) -> Self::IterMut<'this, 'row>;

    /// Treats `row` as storage for a row of the particular type handled by `self`,
    /// and iterates over the (possibly uninitialized) `VarLenRef`s within it.
    ///
    /// Callers are responsible for maintaining whether var-len members have been initialized.
    ///
    /// # Safety
    ///
    /// - `row` must be properly aligned for the row type.
    ///   This alignment constraint should be defined (and documented!)
    ///   by the implementor of `VarLenMembers`.
    ///
    /// - `row` must further be a slice of exactly the number of bytes of the row type.
    ///   Implementors may or may not check this property via `debug_assert!`,
    ///   but callers *must always* ensure it for safety.
    ///   These invariants allow us to construct references to [`VarLenRef`]s inside the slice.
    ///
    ///   Note that `Itererator::next` is a safe function,
    ///   so it must always be valid to advance an iterator to it end.
    ///
    /// - All callers of `visit_var_len` on a particular `row`
    ///   must visit the same set of `VarLenRef`s in the same order,
    ///   though they may do so through different implementors of `VarLenMembers`.
    ///   E.g. it would be valid to use an `AlignedVarLenOffsets` to initialize a row,
    ///   then later read from it using a hypothetical optimized JITted visitor,
    ///   provided the JITted visitor visited the same set of offsets.
    ///
    /// - `Self::visit_var_len` and `Self::visit_var_len_mut`
    ///   must visit the same set of `VarLenRef`s in the same order.
    ///   Various consumers in `Page` and friends depend on this and the previous requirement.
    unsafe fn visit_var_len<'this, 'row>(&'this self, row: &'row Bytes) -> Self::Iter<'this, 'row>;
}

/// Treat `init_row` as storage for a row of the particular type handled by `visitor`,
/// and iterate over the assumed-to-be initialized `VarLenRef`s within it.
///
/// # Safety
///
/// - Callers must satisfy the contract of [`VarLenMembers::visit_var_len`]
///   with respect to `visitor` and `init_row`.
///
/// - `init_row` must be initialized to the extent the returned iterator is advanced.
///   This implies that e.g., `visit_var_len_assume_init(visitor, init_row).take(1).count()`
///   only requires the first offset to be init.
///   Meanwhile, `visit_var_len_assume_init(visitor, init_row).count()`
///   requires all of `visitor, init_row` to be init.
pub unsafe fn visit_var_len_assume_init<'row>(
    visitor: &'row impl VarLenMembers,
    init_row: &'row Bytes,
) -> impl 'row + Iterator<Item = VarLenRef> {
    // SAFETY: `init_row` is valid per safety requirements.
    // SAFETY: `vlr` is initialized in `init_row` per safety requirements.
    unsafe { visitor.visit_var_len(init_row) }.map(move |vlr| unsafe { vlr.assume_init_read() })
}

/// Slice of offsets to var-len members, in units of 2-byte words.
///
/// Units of 2-byte words because `VarLenRef` is 2-byte aligned.
/// Note that `VarLenRef` is 4 bytes wide, but only 2-byte aligned.
///
/// The listed offsets must not overlap, i.e. there must be a gap of at least 2 between each offset.
///
/// For `AlignedVarLenOffsets([n])`, a 4-byte `VarLenRef` exists in each row at +2n bytes.
///
/// e.g.:
/// AlignedVarLenOffsets([0, 4])
/// has:
/// row >= 12 bytes,
/// - var-len ref at +0..4 bytes (i.e. +0..2 `u16`s).
/// - fixed-len field(s) at +4..8 bytes (i.e. +2..4 `u16`s).
/// - var-len ref at +8..12 bytes (i.e. +4..6 `u16`s).
/// - fixed-len field(s) at +12.. (i.e. +6.. `u16`s), if row_size > 12.
//
// TODO: this is not suitable for sum types,
//       as they do not have a single statically-known set of `VarLenRef` offsets.
//       For sums, we'll need a visitor which inspects the tag to determine the live variant,
//       then visits the refs within that variant.
//       We will further need composable var-len visitors, as sums can contain products and vice versa.
#[derive(Copy, Clone)]
pub struct AlignedVarLenOffsets<'a>(&'a [u16]);

impl<'a> AlignedVarLenOffsets<'a> {
    pub const fn from_offsets(offsets: &'a [u16]) -> Self {
        Self(offsets)
    }
}

impl<'a> VarLenMembers for AlignedVarLenOffsets<'a> {
    type Iter<'this, 'row> = AlignedVarLenOffsetsIter<'this, 'row>
        where Self: 'this;

    type IterMut<'this, 'row> = AlignedVarLenOffsetsIterMut<'this, 'row>
        where Self: 'this;

    /// # SAFETY:
    ///
    /// `row` must be 2-byte aligned.
    ///
    /// `row` must be an allocation of at least `2n + 2` bytes,
    /// where `n` is the largest offset in `self`.
    ///
    /// All callers of `visit_var_len` on a particular `row`
    /// must visit the same set of `VarLenRef`s,
    /// though they may do so through different implementors of `VarLenMembers`.
    /// E.g. it would be valid to use an `AlignedVarLenOffsets` to initialize a row,
    /// then later read from it using a hypothetical optimized JITted visitor,
    /// provided the JITted visitor visited the same set of offsets.
    unsafe fn visit_var_len<'this, 'row>(&'this self, row: &'row Bytes) -> Self::Iter<'this, 'row> {
        AlignedVarLenOffsetsIter {
            offsets: self,
            _row_lifetime: PhantomData,
            row: row.as_ptr(),
            next_offset_idx: 0,
        }
    }

    /// # SAFETY:
    ///
    /// `row` must be 2-byte aligned.
    ///
    /// `row` must be an allocation of at least `2n + 2` bytes,
    /// where `n` is the largest offset in `self`.
    ///
    /// All callers of `visit_var_len` on a particular `row`
    /// must visit the same set of `VarLenRef`s,
    /// though they may do so through different implementors of `VarLenMembers`.
    /// E.g. it would be valid to use an `AlignedVarLenOffsets` to initialize a row,
    /// then later read from it using a hypothetical optimized JITted visitor,
    /// provided the JITted visitor visited the same set of offsets.
    unsafe fn visit_var_len_mut<'this, 'row>(&'this self, row: &'row mut Bytes) -> Self::IterMut<'this, 'row> {
        AlignedVarLenOffsetsIterMut {
            offsets: self,
            _row_lifetime: PhantomData,
            row: row.as_mut_ptr(),
            next_offset_idx: 0,
        }
    }
}

pub struct AlignedVarLenOffsetsIter<'offsets, 'row> {
    offsets: &'offsets AlignedVarLenOffsets<'offsets>,
    _row_lifetime: PhantomData<&'row Bytes>,
    row: *const Byte,
    next_offset_idx: usize,
}

impl<'offsets, 'row> Iterator for AlignedVarLenOffsetsIter<'offsets, 'row> {
    type Item = &'row MaybeUninit<VarLenRef>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_offset_idx >= self.offsets.0.len() {
            None
        } else {
            // I sure would like to be able to write `self.next_offset_idx.post_increment(1)`...
            // - pgoldman(2023-11-16).
            let curr_offset_idx = self.next_offset_idx;
            self.next_offset_idx += 1;

            // SAFETY: `AlignedVarLenOffsets::visit_var_len`'s safety requirements
            //         mean that `row` is always 2-byte aligned, so this will be too,
            //         and that `row` is large enough for all the `offsets`,
            //         so this `add` is always in-bounds.
            let elt_ptr: *const MaybeUninit<VarLenRef> =
                unsafe { self.row.add(curr_offset_idx * mem::align_of::<VarLenRef>()).cast() };

            // SAFETY: `elt_ptr` is aligned and inbounds.
            //         `MaybeUninit<VarLenRef>` has no value restrictions,
            //         so it's safe to create an `&mut` to `uninit` or garbage.
            Some(unsafe { &*elt_ptr })
        }
    }
}

pub struct AlignedVarLenOffsetsIterMut<'offsets, 'row> {
    offsets: &'offsets AlignedVarLenOffsets<'offsets>,
    _row_lifetime: PhantomData<&'row mut Bytes>,
    row: *mut Byte,
    next_offset_idx: usize,
}

impl<'offsets, 'row> Iterator for AlignedVarLenOffsetsIterMut<'offsets, 'row> {
    type Item = &'row mut MaybeUninit<VarLenRef>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_offset_idx >= self.offsets.0.len() {
            None
        } else {
            // I sure would like to be able to write `self.next_offset_idx.post_increment(1)`...
            // - pgoldman(2023-11-16).
            let curr_offset_idx = self.next_offset_idx;
            self.next_offset_idx += 1;

            // SAFETY: `AlignedVarLenOffsets::visit_var_len`'s safety requirements
            //         mean that `row` is always 2-byte aligned, so this will be too,
            //         and that `row` is large enough for all the `offsets`,
            //         so this `add` is always in-bounds.
            let elt_ptr: *mut MaybeUninit<VarLenRef> =
                unsafe { self.row.add(curr_offset_idx * mem::align_of::<VarLenRef>()).cast() };

            // SAFETY: `elt_ptr` is aligned and inbounds.
            //         `MaybeUninit<VarLenRef>` has no value restrictions,
            //         so it's safe to create an `&mut` to `uninit` or garbage.
            Some(unsafe { &mut *elt_ptr })
        }
    }
}

#[derive(Copy, Clone)]
pub struct NullVarLenVisitor;

impl VarLenMembers for NullVarLenVisitor {
    type Iter<'this, 'row> = iter::Empty<&'row MaybeUninit<VarLenRef>>;
    type IterMut<'this, 'row> = iter::Empty<&'row mut MaybeUninit<VarLenRef>>;

    unsafe fn visit_var_len<'this, 'row>(&'this self, _row: &'row Bytes) -> Self::Iter<'this, 'row> {
        iter::empty()
    }

    unsafe fn visit_var_len_mut<'this, 'row>(&'this self, _row: &'row mut Bytes) -> Self::IterMut<'this, 'row> {
        iter::empty()
    }
}
