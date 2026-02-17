//! Table indexes with specialized key types.
//!
//! Indexes could be implemented as `BTreeIndex<AlgebraicValue, RowPointer>` (and once were),
//! but that results in wasted memory and spurious comparisons and branches
//! because the keys must always be homogeneous at a more specific type than `AlgebraicValue`.
//!
//! As an optimization, we hoist the enum out of the keys to the index itself.
//! This is a sizeable improvement for integer keys,
//! as e.g. `u64::cmp` is much faster than `AlgebraicValue::cmp`.
//!
//! This results in some pretty ugly code, where types that would be structs
//! are instead enums with similar-looking variants for each specialized key type,
//! and methods that interact with those enums have matches with similar-looking arms.
//! Some day we may devise a better solution, but this is good enough for now.
//!
//! I (pgoldman 2024-02-05) suspect, but have not measured, that there's no real reason
//! to have a `ProductType` variant, which would apply to multi-column indexes.
//! I believe `ProductValue::cmp` to not be meaningfully faster than `AlgebraicValue::cmp`.
//! Eventually, we will likely want to compile comparison functions and representations
//! for `ProductValue`-keyed indexes which take advantage of type information,
//! since we know when creating the index the number and type of all the indexed columns.
//! This may involve a bytecode compiler, a tree of closures, or a native JIT.
//!
//! We also represent unique indices more compactly than non-unique ones, avoiding the multi-map.
//! Additionally, beyond our btree indices,
//! we support direct unique indices, where key are indices into `Vec`s.
use self::btree_index::{BTreeIndex, BTreeIndexRangeIter};
use self::hash_index::HashIndex;
use self::same_key_entry::SameKeyEntryIter;
use self::unique_btree_index::{UniqueBTreeIndex, UniqueBTreeIndexRangeIter, UniquePointIter};
use self::unique_direct_fixed_cap_index::{UniqueDirectFixedCapIndex, UniqueDirectFixedCapIndexRangeIter};
use self::unique_direct_index::{UniqueDirectIndex, UniqueDirectIndexRangeIter};
use self::unique_hash_index::UniqueHashIndex;
use super::indexes::RowPointer;
use super::table::RowRef;
use crate::table_index::index::Despecialize;
use crate::table_index::unique_direct_index::ToFromUsize;
use crate::{read_column::ReadColumn, static_assert_size};
use core::fmt;
use core::ops::RangeBounds;
use spacetimedb_primitives::ColList;
use spacetimedb_sats::memory_usage::MemoryUsage;
use spacetimedb_sats::SumValue;
use spacetimedb_sats::{
    bsatn::{from_slice, DecodeError},
    i256,
    product_value::InvalidFieldError,
    sum_value::SumTag,
    u256, AlgebraicType, AlgebraicValue, ProductType, F32, F64,
};
use spacetimedb_schema::def::IndexAlgorithm;

mod btree_index;
mod hash_index;
mod index;
mod key_size;
mod same_key_entry;
pub mod unique_btree_index;
pub mod unique_direct_fixed_cap_index;
pub mod unique_direct_index;
mod unique_hash_index;

pub use self::index::{Index, IndexCannotSeekRange, IndexSeekRangeResult, RangedIndex};
pub use self::key_size::KeySize;

/// A point iterator over a [`TypedIndex`], with a specialized key type.
///
/// See module docs for info about specialization.
enum TypedIndexPointIter<'a> {
    NonUnique(SameKeyEntryIter<'a>),
    Unique(UniquePointIter),
}

impl Iterator for TypedIndexPointIter<'_> {
    type Item = RowPointer;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::NonUnique(this) => this.next(),
            Self::Unique(this) => this.next(),
        }
    }
}

/// An iterator over rows matching a certain [`AlgebraicValue`] on the [`TableIndex`].
pub struct TableIndexPointIter<'a> {
    /// The iterator seeking for matching values.
    iter: TypedIndexPointIter<'a>,
}

impl Iterator for TableIndexPointIter<'_> {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

/// A ranged iterator over a [`TypedIndex`], with a specialized key type.
///
/// See module docs for info about specialization.
#[derive(Clone)]
enum TypedIndexRangeIter<'a> {
    /// The range itself provided was empty.
    RangeEmpty,

    // All the non-unique btree index iterators.
    BTreeBool(BTreeIndexRangeIter<'a, bool>),
    BTreeU8(BTreeIndexRangeIter<'a, u8>),
    BTreeSumTag(BTreeIndexRangeIter<'a, SumTag>),
    BTreeI8(BTreeIndexRangeIter<'a, i8>),
    BTreeU16(BTreeIndexRangeIter<'a, u16>),
    BTreeI16(BTreeIndexRangeIter<'a, i16>),
    BTreeU32(BTreeIndexRangeIter<'a, u32>),
    BTreeI32(BTreeIndexRangeIter<'a, i32>),
    BTreeU64(BTreeIndexRangeIter<'a, u64>),
    BTreeI64(BTreeIndexRangeIter<'a, i64>),
    BTreeU128(BTreeIndexRangeIter<'a, u128>),
    BTreeI128(BTreeIndexRangeIter<'a, i128>),
    BTreeU256(BTreeIndexRangeIter<'a, u256>),
    BTreeI256(BTreeIndexRangeIter<'a, i256>),
    BTreeF32(BTreeIndexRangeIter<'a, F32>),
    BTreeF64(BTreeIndexRangeIter<'a, F64>),
    BTreeString(BTreeIndexRangeIter<'a, Box<str>>),
    BTreeAV(BTreeIndexRangeIter<'a, AlgebraicValue>),

    // All the unique btree index iterators.
    UniqueBTreeBool(UniqueBTreeIndexRangeIter<'a, bool>),
    UniqueBTreeU8(UniqueBTreeIndexRangeIter<'a, u8>),
    UniqueBTreeSumTag(UniqueBTreeIndexRangeIter<'a, SumTag>),
    UniqueBTreeI8(UniqueBTreeIndexRangeIter<'a, i8>),
    UniqueBTreeU16(UniqueBTreeIndexRangeIter<'a, u16>),
    UniqueBTreeI16(UniqueBTreeIndexRangeIter<'a, i16>),
    UniqueBTreeU32(UniqueBTreeIndexRangeIter<'a, u32>),
    UniqueBTreeI32(UniqueBTreeIndexRangeIter<'a, i32>),
    UniqueBTreeU64(UniqueBTreeIndexRangeIter<'a, u64>),
    UniqueBTreeI64(UniqueBTreeIndexRangeIter<'a, i64>),
    UniqueBTreeU128(UniqueBTreeIndexRangeIter<'a, u128>),
    UniqueBTreeI128(UniqueBTreeIndexRangeIter<'a, i128>),
    UniqueBTreeU256(UniqueBTreeIndexRangeIter<'a, u256>),
    UniqueBTreeI256(UniqueBTreeIndexRangeIter<'a, i256>),
    UniqueBTreeF32(UniqueBTreeIndexRangeIter<'a, F32>),
    UniqueBTreeF64(UniqueBTreeIndexRangeIter<'a, F64>),
    UniqueBTreeString(UniqueBTreeIndexRangeIter<'a, Box<str>>),
    UniqueBTreeAV(UniqueBTreeIndexRangeIter<'a, AlgebraicValue>),

    UniqueDirect(UniqueDirectIndexRangeIter<'a>),
    UniqueDirectU8(UniqueDirectFixedCapIndexRangeIter<'a>),
}

impl Iterator for TypedIndexRangeIter<'_> {
    type Item = RowPointer;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::RangeEmpty => None,

            Self::BTreeBool(this) => this.next(),
            Self::BTreeU8(this) => this.next(),
            Self::BTreeSumTag(this) => this.next(),
            Self::BTreeI8(this) => this.next(),
            Self::BTreeU16(this) => this.next(),
            Self::BTreeI16(this) => this.next(),
            Self::BTreeU32(this) => this.next(),
            Self::BTreeI32(this) => this.next(),
            Self::BTreeU64(this) => this.next(),
            Self::BTreeI64(this) => this.next(),
            Self::BTreeU128(this) => this.next(),
            Self::BTreeI128(this) => this.next(),
            Self::BTreeU256(this) => this.next(),
            Self::BTreeI256(this) => this.next(),
            Self::BTreeF32(this) => this.next(),
            Self::BTreeF64(this) => this.next(),
            Self::BTreeString(this) => this.next(),
            Self::BTreeAV(this) => this.next(),

            Self::UniqueBTreeBool(this) => this.next(),
            Self::UniqueBTreeU8(this) => this.next(),
            Self::UniqueBTreeSumTag(this) => this.next(),
            Self::UniqueBTreeI8(this) => this.next(),
            Self::UniqueBTreeU16(this) => this.next(),
            Self::UniqueBTreeI16(this) => this.next(),
            Self::UniqueBTreeU32(this) => this.next(),
            Self::UniqueBTreeI32(this) => this.next(),
            Self::UniqueBTreeU64(this) => this.next(),
            Self::UniqueBTreeI64(this) => this.next(),
            Self::UniqueBTreeU128(this) => this.next(),
            Self::UniqueBTreeI128(this) => this.next(),
            Self::UniqueBTreeU256(this) => this.next(),
            Self::UniqueBTreeI256(this) => this.next(),
            Self::UniqueBTreeF32(this) => this.next(),
            Self::UniqueBTreeF64(this) => this.next(),
            Self::UniqueBTreeString(this) => this.next(),
            Self::UniqueBTreeAV(this) => this.next(),

            Self::UniqueDirect(this) => this.next(),
            Self::UniqueDirectU8(this) => this.next(),
        }
    }
}

/// An iterator over rows matching a range of [`AlgebraicValue`]s on the [`TableIndex`].
#[derive(Clone)]
pub struct TableIndexRangeIter<'a> {
    /// The iterator seeking for matching values.
    iter: TypedIndexRangeIter<'a>,
}

impl Iterator for TableIndexRangeIter<'_> {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl fmt::Debug for TableIndexRangeIter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let iter = self.clone();
        f.debug_list().entries(iter).finish()
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, derive_more::From)]
enum BowStr<'a> {
    Borrowed(&'a str),
    Owned(Box<str>),
}

impl<'a> BowStr<'a> {
    fn borrow(&'a self) -> &'a str {
        match self {
            Self::Borrowed(x) => x,
            Self::Owned(x) => x,
        }
    }

    fn into_owned(self) -> Box<str> {
        match self {
            Self::Borrowed(x) => x.into(),
            Self::Owned(x) => x,
        }
    }
}

impl<'a> From<&'a Box<str>> for BowStr<'a> {
    fn from(value: &'a Box<str>) -> Self {
        Self::Borrowed(value)
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, derive_more::From)]
enum CowAV<'a> {
    Borrowed(&'a AlgebraicValue),
    Owned(AlgebraicValue),
}

impl<'a> CowAV<'a> {
    fn borrow(&'a self) -> &'a AlgebraicValue {
        match self {
            Self::Borrowed(x) => x,
            Self::Owned(x) => x,
        }
    }

    fn into_owned(self) -> AlgebraicValue {
        match self {
            Self::Borrowed(x) => x.clone(),
            Self::Owned(x) => x,
        }
    }
}

/// A key into a [`TypedIndex`].
#[derive(enum_as_inner::EnumAsInner, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum TypedIndexKey<'a> {
    Bool(bool),
    U8(u8),
    SumTag(SumTag),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
    U128(u128),
    I128(i128),
    U256(u256),
    I256(i256),
    F32(F32),
    F64(F64),
    String(BowStr<'a>),
    AV(CowAV<'a>),
}

impl<'a> TypedIndexKey<'a> {
    /// Derives a [`TypedIndexKey`] from an [`AlgebraicValue`]
    /// driven by the kind of [`TypedIndex`] provided in `index`.
    #[inline]
    fn from_algebraic_value(index: &TypedIndex, value: &'a AlgebraicValue) -> Self {
        use AlgebraicValue::*;
        use TypedIndex::*;
        match (value, index) {
            (Bool(v), BTreeBool(_) | HashBool(_) | UniqueBTreeBool(_) | UniqueHashBool(_)) => Self::Bool(*v),

            (U8(v), BTreeU8(_) | HashU8(_) | UniqueBTreeU8(_) | UniqueHashU8(_) | UniqueDirectU8(_)) => Self::U8(*v),
            (
                U8(v) | Sum(SumValue { tag: v, .. }),
                BTreeSumTag(_) | HashSumTag(_) | UniqueBTreeSumTag(_) | UniqueHashSumTag(_) | UniqueDirectSumTag(_),
            ) => Self::SumTag(SumTag(*v)),

            (U16(v), BTreeU16(_) | HashU16(_) | UniqueBTreeU16(_) | UniqueHashU16(_) | UniqueDirectU16(_)) => {
                Self::U16(*v)
            }
            (U32(v), BTreeU32(_) | HashU32(_) | UniqueBTreeU32(_) | UniqueHashU32(_) | UniqueDirectU32(_)) => {
                Self::U32(*v)
            }
            (U64(v), BTreeU64(_) | HashU64(_) | UniqueBTreeU64(_) | UniqueHashU64(_) | UniqueDirectU64(_)) => {
                Self::U64(*v)
            }
            (U128(v), BTreeU128(_) | HashU128(_) | UniqueBTreeU128(_) | UniqueHashU128(_)) => Self::U128(v.0),
            (U256(v), BTreeU256(_) | HashU256(_) | UniqueBTreeU256(_) | UniqueHashU256(_)) => Self::U256(**v),

            (I8(v), BTreeI8(_) | HashI8(_) | UniqueBTreeI8(_) | UniqueHashI8(_)) => Self::I8(*v),
            (I16(v), BTreeI16(_) | HashI16(_) | UniqueBTreeI16(_) | UniqueHashI16(_)) => Self::I16(*v),
            (I32(v), BTreeI32(_) | HashI32(_) | UniqueBTreeI32(_) | UniqueHashI32(_)) => Self::I32(*v),
            (I64(v), BTreeI64(_) | HashI64(_) | UniqueBTreeI64(_) | UniqueHashI64(_)) => Self::I64(*v),
            (I128(v), BTreeI128(_) | HashI128(_) | UniqueBTreeI128(_) | UniqueHashI128(_)) => Self::I128(v.0),
            (I256(v), BTreeI256(_) | HashI256(_) | UniqueBTreeI256(_) | UniqueHashI256(_)) => Self::I256(**v),

            (F32(v), BTreeF32(_) | HashF32(_) | UniqueBTreeF32(_) | UniqueHashF32(_)) => Self::F32(*v),
            (F64(v), BTreeF64(_) | HashF64(_) | UniqueBTreeF64(_) | UniqueHashF64(_)) => Self::F64(*v),

            (String(v), BTreeString(_) | HashString(_) | UniqueBTreeString(_) | UniqueHashString(_)) => {
                Self::String(v.into())
            }

            (av, BTreeAV(_) | HashAV(_) | UniqueBTreeAV(_) | UniqueHashAV(_)) => Self::AV(CowAV::Borrowed(av)),
            _ => panic!("value {value:?} is not compatible with index {index:?}"),
        }
    }

    /// Derives a [`TypedIndexKey`] from BSATN-encoded `bytes`,
    /// driven by the kind of [`TypedIndex`] provided in `index`.
    #[inline]
    fn from_bsatn(index: &TypedIndex, ty: &AlgebraicType, bytes: &'a [u8]) -> Result<Self, DecodeError> {
        use TypedIndex::*;
        match index {
            BTreeBool(_) | HashBool(_) | UniqueBTreeBool(_) | UniqueHashBool(_) => from_slice(bytes).map(Self::Bool),

            BTreeU8(_) | HashU8(_) | UniqueBTreeU8(_) | UniqueHashU8(_) | UniqueDirectU8(_) => {
                from_slice(bytes).map(Self::U8)
            }
            BTreeSumTag(_) | HashSumTag(_) | UniqueBTreeSumTag(_) | UniqueHashSumTag(_) | UniqueDirectSumTag(_) => {
                from_slice(bytes).map(Self::SumTag)
            }
            BTreeU16(_) | HashU16(_) | UniqueBTreeU16(_) | UniqueHashU16(_) | UniqueDirectU16(_) => {
                from_slice(bytes).map(Self::U16)
            }
            BTreeU32(_) | HashU32(_) | UniqueBTreeU32(_) | UniqueHashU32(_) | UniqueDirectU32(_) => {
                from_slice(bytes).map(Self::U32)
            }
            BTreeU64(_) | HashU64(_) | UniqueBTreeU64(_) | UniqueHashU64(_) | UniqueDirectU64(_) => {
                from_slice(bytes).map(Self::U64)
            }
            BTreeU128(_) | HashU128(_) | UniqueBTreeU128(_) | UniqueHashU128(_) => from_slice(bytes).map(Self::U128),
            BTreeU256(_) | HashU256(_) | UniqueBTreeU256(_) | UniqueHashU256(_) => from_slice(bytes).map(Self::U256),

            BTreeI8(_) | HashI8(_) | UniqueBTreeI8(_) | UniqueHashI8(_) => from_slice(bytes).map(Self::I8),
            BTreeI16(_) | HashI16(_) | UniqueBTreeI16(_) | UniqueHashI16(_) => from_slice(bytes).map(Self::I16),
            BTreeI32(_) | HashI32(_) | UniqueBTreeI32(_) | UniqueHashI32(_) => from_slice(bytes).map(Self::I32),
            BTreeI64(_) | HashI64(_) | UniqueBTreeI64(_) | UniqueHashI64(_) => from_slice(bytes).map(Self::I64),
            BTreeI128(_) | HashI128(_) | UniqueBTreeI128(_) | UniqueHashI128(_) => from_slice(bytes).map(Self::I128),
            BTreeI256(_) | HashI256(_) | UniqueBTreeI256(_) | UniqueHashI256(_) => from_slice(bytes).map(Self::I256),

            BTreeF32(_) | HashF32(_) | UniqueBTreeF32(_) | UniqueHashF32(_) => from_slice(bytes).map(Self::F32),
            BTreeF64(_) | HashF64(_) | UniqueBTreeF64(_) | UniqueHashF64(_) => from_slice(bytes).map(Self::F64),

            BTreeString(_) | HashString(_) | UniqueBTreeString(_) | UniqueHashString(_) => {
                from_slice(bytes).map(BowStr::Borrowed).map(Self::String)
            }

            BTreeAV(_) | HashAV(_) | UniqueBTreeAV(_) | UniqueHashAV(_) => AlgebraicValue::decode(ty, &mut { bytes })
                .map(CowAV::Owned)
                .map(Self::AV),
        }
    }

    /// Derives a [`TypedIndexKey`] from a [`RowRef`]
    /// and a [`ColList`] that describes what columns are indexed by `index`.
    ///
    /// Assumes that `row_ref` projected to `cols`
    /// has the same type as the keys of `index`.
    ///
    /// # Safety
    ///
    /// 1. Caller promises that `cols` matches what was given at construction (`TableIndex::new`).
    /// 2. Caller promises that the projection of `row_ref`'s type's equals the index's key type.
    #[inline]
    unsafe fn from_row_ref(index: &TypedIndex, cols: &ColList, row_ref: RowRef<'_>) -> Self {
        fn proj<T: ReadColumn>(cols: &ColList, row_ref: RowRef<'_>) -> T {
            // Extract the column.
            let col_pos = cols.as_singleton();
            // SAFETY: Caller promised that `cols` matches what was given at construction (`Self::new`).
            // In the case of `.clone_structure()`, the structure is preserved,
            // so the promise is also preserved.
            // This entails, that because we reached here, that `cols` is singleton.
            let col_pos = unsafe { col_pos.unwrap_unchecked() }.idx();

            // Extract the layout of the column.
            let col_layouts = &row_ref.row_layout().product().elements;
            // SAFETY:
            // - Caller promised that projecting the `row_ref`'s type/layout to `self.indexed_columns`
            //   gives us the index's key type.
            //   This entails that each `ColId` in `self.indexed_columns`
            //   must be in-bounds of `row_ref`'s layout.
            let col_layout = unsafe { col_layouts.get_unchecked(col_pos) };

            // Read the column in `row_ref`.
            // SAFETY:
            // - `col_layout` was just derived from the row layout.
            // - Caller promised that the type-projection of the row type/layout
            //   equals the index's key type.
            //   We've reached here, so the index's key type is compatible with `T`.
            // - `self` is a valid row so offsetting to `col_layout` is valid.
            unsafe { T::unchecked_read_column(row_ref, col_layout) }
        }

        use TypedIndex::*;
        match index {
            BTreeBool(_) | HashBool(_) | UniqueBTreeBool(_) | UniqueHashBool(_) => Self::Bool(proj(cols, row_ref)),

            BTreeU8(_) | HashU8(_) | UniqueBTreeU8(_) | UniqueHashU8(_) | UniqueDirectU8(_) => {
                Self::U8(proj(cols, row_ref))
            }
            BTreeSumTag(_) | HashSumTag(_) | UniqueBTreeSumTag(_) | UniqueHashSumTag(_) | UniqueDirectSumTag(_) => {
                Self::SumTag(proj(cols, row_ref))
            }
            BTreeU16(_) | HashU16(_) | UniqueBTreeU16(_) | UniqueHashU16(_) | UniqueDirectU16(_) => {
                Self::U16(proj(cols, row_ref))
            }
            BTreeU32(_) | HashU32(_) | UniqueBTreeU32(_) | UniqueHashU32(_) | UniqueDirectU32(_) => {
                Self::U32(proj(cols, row_ref))
            }
            BTreeU64(_) | HashU64(_) | UniqueBTreeU64(_) | UniqueHashU64(_) | UniqueDirectU64(_) => {
                Self::U64(proj(cols, row_ref))
            }
            BTreeU128(_) | HashU128(_) | UniqueBTreeU128(_) | UniqueHashU128(_) => Self::U128(proj(cols, row_ref)),
            BTreeU256(_) | HashU256(_) | UniqueBTreeU256(_) | UniqueHashU256(_) => Self::U256(proj(cols, row_ref)),

            BTreeI8(_) | HashI8(_) | UniqueBTreeI8(_) | UniqueHashI8(_) => Self::I8(proj(cols, row_ref)),
            BTreeI16(_) | HashI16(_) | UniqueBTreeI16(_) | UniqueHashI16(_) => Self::I16(proj(cols, row_ref)),
            BTreeI32(_) | HashI32(_) | UniqueBTreeI32(_) | UniqueHashI32(_) => Self::I32(proj(cols, row_ref)),
            BTreeI64(_) | HashI64(_) | UniqueBTreeI64(_) | UniqueHashI64(_) => Self::I64(proj(cols, row_ref)),
            BTreeI128(_) | HashI128(_) | UniqueBTreeI128(_) | UniqueHashI128(_) => Self::I128(proj(cols, row_ref)),
            BTreeI256(_) | HashI256(_) | UniqueBTreeI256(_) | UniqueHashI256(_) => Self::I256(proj(cols, row_ref)),

            BTreeF32(_) | HashF32(_) | UniqueBTreeF32(_) | UniqueHashF32(_) => Self::F32(proj(cols, row_ref)),
            BTreeF64(_) | HashF64(_) | UniqueBTreeF64(_) | UniqueHashF64(_) => Self::F64(proj(cols, row_ref)),

            BTreeString(_) | HashString(_) | UniqueBTreeString(_) | UniqueHashString(_) => {
                Self::String(BowStr::Owned(proj(cols, row_ref)))
            }

            BTreeAV(_) | HashAV(_) | UniqueBTreeAV(_) | UniqueHashAV(_) => {
                // SAFETY: Caller promised that any `col` in `cols` is in-bounds of `row_ref`'s layout.
                let val = unsafe { row_ref.project_unchecked(cols) };
                Self::AV(CowAV::Owned(val))
            }
        }
    }

    /// Returns a borrowed version of the key.
    #[inline]
    fn borrowed(&self) -> TypedIndexKey<'_> {
        match self {
            Self::Bool(x) => TypedIndexKey::Bool(*x),
            Self::U8(x) => TypedIndexKey::U8(*x),
            Self::SumTag(x) => TypedIndexKey::SumTag(*x),
            Self::I8(x) => TypedIndexKey::I8(*x),
            Self::U16(x) => TypedIndexKey::U16(*x),
            Self::I16(x) => TypedIndexKey::I16(*x),
            Self::U32(x) => TypedIndexKey::U32(*x),
            Self::I32(x) => TypedIndexKey::I32(*x),
            Self::U64(x) => TypedIndexKey::U64(*x),
            Self::I64(x) => TypedIndexKey::I64(*x),
            Self::U128(x) => TypedIndexKey::U128(*x),
            Self::I128(x) => TypedIndexKey::I128(*x),
            Self::U256(x) => TypedIndexKey::U256(*x),
            Self::I256(x) => TypedIndexKey::I256(*x),
            Self::F32(x) => TypedIndexKey::F32(*x),
            Self::F64(x) => TypedIndexKey::F64(*x),
            Self::String(x) => TypedIndexKey::String(x.borrow().into()),
            Self::AV(x) => TypedIndexKey::AV(x.borrow().into()),
        }
    }

    /// Converts the key into an [`AlgebraicValue`].
    fn into_algebraic_value(self) -> AlgebraicValue {
        match self {
            Self::Bool(x) => x.into(),
            Self::U8(x) => x.into(),
            Self::SumTag(x) => x.into(),
            Self::I8(x) => x.into(),
            Self::U16(x) => x.into(),
            Self::I16(x) => x.into(),
            Self::U32(x) => x.into(),
            Self::I32(x) => x.into(),
            Self::U64(x) => x.into(),
            Self::I64(x) => x.into(),
            Self::U128(x) => x.into(),
            Self::I128(x) => x.into(),
            Self::U256(x) => x.into(),
            Self::I256(x) => x.into(),
            Self::F32(x) => x.into(),
            Self::F64(x) => x.into(),
            Self::String(x) => x.into_owned().into(),
            Self::AV(x) => x.into_owned(),
        }
    }
}

const WRONG_TYPE: &str = "key does not conform to key type of index";

/// An index from a key type determined at runtime to `RowPointer`(s).
///
/// See module docs for info about specialization.
#[derive(Debug, PartialEq, Eq, derive_more::From)]
enum TypedIndex {
    // All the non-unique btree index types.
    BTreeBool(BTreeIndex<bool>),
    BTreeU8(BTreeIndex<u8>),
    BTreeSumTag(BTreeIndex<SumTag>),
    BTreeI8(BTreeIndex<i8>),
    BTreeU16(BTreeIndex<u16>),
    BTreeI16(BTreeIndex<i16>),
    BTreeU32(BTreeIndex<u32>),
    BTreeI32(BTreeIndex<i32>),
    BTreeU64(BTreeIndex<u64>),
    BTreeI64(BTreeIndex<i64>),
    BTreeU128(BTreeIndex<u128>),
    BTreeI128(BTreeIndex<i128>),
    BTreeU256(BTreeIndex<u256>),
    BTreeI256(BTreeIndex<i256>),
    BTreeF32(BTreeIndex<F32>),
    BTreeF64(BTreeIndex<F64>),
    // TODO(perf, centril): consider `UmbraString` or some "German string".
    BTreeString(BTreeIndex<Box<str>>),
    BTreeAV(BTreeIndex<AlgebraicValue>),

    // All the non-unique hash index types.
    HashBool(HashIndex<bool>),
    HashU8(HashIndex<u8>),
    HashSumTag(HashIndex<SumTag>),
    HashI8(HashIndex<i8>),
    HashU16(HashIndex<u16>),
    HashI16(HashIndex<i16>),
    HashU32(HashIndex<u32>),
    HashI32(HashIndex<i32>),
    HashU64(HashIndex<u64>),
    HashI64(HashIndex<i64>),
    HashU128(HashIndex<u128>),
    HashI128(HashIndex<i128>),
    HashU256(HashIndex<u256>),
    HashI256(HashIndex<i256>),
    HashF32(HashIndex<F32>),
    HashF64(HashIndex<F64>),
    // TODO(perf, centril): consider `UmbraString` or some "German string".
    HashString(HashIndex<Box<str>>),
    HashAV(HashIndex<AlgebraicValue>),

    // All the unique btree index types.
    UniqueBTreeBool(UniqueBTreeIndex<bool>),
    UniqueBTreeU8(UniqueBTreeIndex<u8>),
    UniqueBTreeSumTag(UniqueBTreeIndex<SumTag>),
    UniqueBTreeI8(UniqueBTreeIndex<i8>),
    UniqueBTreeU16(UniqueBTreeIndex<u16>),
    UniqueBTreeI16(UniqueBTreeIndex<i16>),
    UniqueBTreeU32(UniqueBTreeIndex<u32>),
    UniqueBTreeI32(UniqueBTreeIndex<i32>),
    UniqueBTreeU64(UniqueBTreeIndex<u64>),
    UniqueBTreeI64(UniqueBTreeIndex<i64>),
    UniqueBTreeU128(UniqueBTreeIndex<u128>),
    UniqueBTreeI128(UniqueBTreeIndex<i128>),
    UniqueBTreeU256(UniqueBTreeIndex<u256>),
    UniqueBTreeI256(UniqueBTreeIndex<i256>),
    UniqueBTreeF32(UniqueBTreeIndex<F32>),
    UniqueBTreeF64(UniqueBTreeIndex<F64>),
    // TODO(perf, centril): consider `UmbraString` or some "German string".
    UniqueBTreeString(UniqueBTreeIndex<Box<str>>),
    UniqueBTreeAV(UniqueBTreeIndex<AlgebraicValue>),

    // All the unique hash index types.
    UniqueHashBool(UniqueHashIndex<bool>),
    UniqueHashU8(UniqueHashIndex<u8>),
    UniqueHashSumTag(UniqueHashIndex<SumTag>),
    UniqueHashI8(UniqueHashIndex<i8>),
    UniqueHashU16(UniqueHashIndex<u16>),
    UniqueHashI16(UniqueHashIndex<i16>),
    UniqueHashU32(UniqueHashIndex<u32>),
    UniqueHashI32(UniqueHashIndex<i32>),
    UniqueHashU64(UniqueHashIndex<u64>),
    UniqueHashI64(UniqueHashIndex<i64>),
    UniqueHashU128(UniqueHashIndex<u128>),
    UniqueHashI128(UniqueHashIndex<i128>),
    UniqueHashU256(UniqueHashIndex<u256>),
    UniqueHashI256(UniqueHashIndex<i256>),
    UniqueHashF32(UniqueHashIndex<F32>),
    UniqueHashF64(UniqueHashIndex<F64>),
    // TODO(perf, centril): consider `UmbraString` or some "German string".
    UniqueHashString(UniqueHashIndex<Box<str>>),
    UniqueHashAV(UniqueHashIndex<AlgebraicValue>),

    // All the unique direct index types.
    UniqueDirectU8(UniqueDirectIndex<u8>),
    UniqueDirectSumTag(UniqueDirectFixedCapIndex<SumTag>),
    UniqueDirectU16(UniqueDirectIndex<u16>),
    UniqueDirectU32(UniqueDirectIndex<u32>),
    UniqueDirectU64(UniqueDirectIndex<u64>),
}

static_assert_size!(TypedIndex, 64);

macro_rules! same_for_all_types {
    ($scrutinee:expr, $this:ident => $body:expr) => {
        match $scrutinee {
            Self::BTreeBool($this) => $body,
            Self::BTreeU8($this) => $body,
            Self::BTreeSumTag($this) => $body,
            Self::BTreeI8($this) => $body,
            Self::BTreeU16($this) => $body,
            Self::BTreeI16($this) => $body,
            Self::BTreeU32($this) => $body,
            Self::BTreeI32($this) => $body,
            Self::BTreeU64($this) => $body,
            Self::BTreeI64($this) => $body,
            Self::BTreeU128($this) => $body,
            Self::BTreeI128($this) => $body,
            Self::BTreeU256($this) => $body,
            Self::BTreeI256($this) => $body,
            Self::BTreeF32($this) => $body,
            Self::BTreeF64($this) => $body,
            Self::BTreeString($this) => $body,
            Self::BTreeAV($this) => $body,

            Self::HashBool($this) => $body,
            Self::HashU8($this) => $body,
            Self::HashSumTag($this) => $body,
            Self::HashI8($this) => $body,
            Self::HashU16($this) => $body,
            Self::HashI16($this) => $body,
            Self::HashU32($this) => $body,
            Self::HashI32($this) => $body,
            Self::HashU64($this) => $body,
            Self::HashI64($this) => $body,
            Self::HashU128($this) => $body,
            Self::HashI128($this) => $body,
            Self::HashU256($this) => $body,
            Self::HashI256($this) => $body,
            Self::HashF32($this) => $body,
            Self::HashF64($this) => $body,
            Self::HashString($this) => $body,
            Self::HashAV($this) => $body,

            Self::UniqueBTreeBool($this) => $body,
            Self::UniqueBTreeU8($this) => $body,
            Self::UniqueBTreeSumTag($this) => $body,
            Self::UniqueBTreeI8($this) => $body,
            Self::UniqueBTreeU16($this) => $body,
            Self::UniqueBTreeI16($this) => $body,
            Self::UniqueBTreeU32($this) => $body,
            Self::UniqueBTreeI32($this) => $body,
            Self::UniqueBTreeU64($this) => $body,
            Self::UniqueBTreeI64($this) => $body,
            Self::UniqueBTreeU128($this) => $body,
            Self::UniqueBTreeI128($this) => $body,
            Self::UniqueBTreeU256($this) => $body,
            Self::UniqueBTreeI256($this) => $body,
            Self::UniqueBTreeF32($this) => $body,
            Self::UniqueBTreeF64($this) => $body,
            Self::UniqueBTreeString($this) => $body,
            Self::UniqueBTreeAV($this) => $body,

            Self::UniqueHashBool($this) => $body,
            Self::UniqueHashU8($this) => $body,
            Self::UniqueHashSumTag($this) => $body,
            Self::UniqueHashI8($this) => $body,
            Self::UniqueHashU16($this) => $body,
            Self::UniqueHashI16($this) => $body,
            Self::UniqueHashU32($this) => $body,
            Self::UniqueHashI32($this) => $body,
            Self::UniqueHashU64($this) => $body,
            Self::UniqueHashI64($this) => $body,
            Self::UniqueHashU128($this) => $body,
            Self::UniqueHashI128($this) => $body,
            Self::UniqueHashU256($this) => $body,
            Self::UniqueHashI256($this) => $body,
            Self::UniqueHashF32($this) => $body,
            Self::UniqueHashF64($this) => $body,
            Self::UniqueHashString($this) => $body,
            Self::UniqueHashAV($this) => $body,

            Self::UniqueDirectSumTag($this) => $body,
            Self::UniqueDirectU8($this) => $body,
            Self::UniqueDirectU16($this) => $body,
            Self::UniqueDirectU32($this) => $body,
            Self::UniqueDirectU64($this) => $body,
        }
    };
}

impl MemoryUsage for TypedIndex {
    fn heap_usage(&self) -> usize {
        same_for_all_types!(self, this => this.heap_usage())
    }
}

#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum IndexKind {
    BTree,
    Hash,
    Direct,
}

impl IndexKind {
    pub(crate) fn from_algo(algo: &IndexAlgorithm) -> Self {
        match algo {
            IndexAlgorithm::BTree(_) => Self::BTree,
            IndexAlgorithm::Hash(_) => Self::Hash,
            IndexAlgorithm::Direct(_) => Self::Direct,
            // This is due to `#[non_exhaustive]`.
            _ => unreachable!(),
        }
    }
}

impl TypedIndex {
    /// Returns a new index with keys being of `key_type` and the index possibly `is_unique`.
    fn new(key_type: &AlgebraicType, kind: IndexKind, is_unique: bool) -> Self {
        match kind {
            IndexKind::BTree => Self::new_btree_index(key_type, is_unique),
            IndexKind::Hash => Self::new_hash_index(key_type, is_unique),
            IndexKind::Direct => Self::new_direct_index(key_type, is_unique)
                .unwrap_or_else(|| Self::new_btree_index(key_type, is_unique)),
        }
    }

    /// Returns a new direct index with key being of `key_type`.
    ///
    /// If the parameters passed are not compatible with a direct index,
    /// `None` is returned.
    fn new_direct_index(key_type: &AlgebraicType, is_unique: bool) -> Option<Self> {
        if !is_unique {
            return None;
        }

        use TypedIndex::*;
        Some(match key_type {
            AlgebraicType::U8 => UniqueDirectU8(<_>::default()),
            AlgebraicType::U16 => UniqueDirectU16(<_>::default()),
            AlgebraicType::U32 => UniqueDirectU32(<_>::default()),
            AlgebraicType::U64 => UniqueDirectU64(<_>::default()),
            // For a plain enum, use `u8` as the native type.
            AlgebraicType::Sum(sum) if sum.is_simple_enum() => {
                UniqueDirectSumTag(UniqueDirectFixedCapIndex::new(sum.variants.len()))
            }
            _ => return None,
        })
    }

    /// Returns a new btree index with key being of `key_type`.
    fn new_btree_index(key_type: &AlgebraicType, is_unique: bool) -> Self {
        use TypedIndex::*;

        // If the index is on a single column of a primitive type, string, or plain enum,
        // use a homogeneous map with a native key type.
        if is_unique {
            match key_type {
                AlgebraicType::Bool => UniqueBTreeBool(<_>::default()),
                AlgebraicType::I8 => UniqueBTreeI8(<_>::default()),
                AlgebraicType::U8 => UniqueBTreeU8(<_>::default()),
                AlgebraicType::I16 => UniqueBTreeI16(<_>::default()),
                AlgebraicType::U16 => UniqueBTreeU16(<_>::default()),
                AlgebraicType::I32 => UniqueBTreeI32(<_>::default()),
                AlgebraicType::U32 => UniqueBTreeU32(<_>::default()),
                AlgebraicType::I64 => UniqueBTreeI64(<_>::default()),
                AlgebraicType::U64 => UniqueBTreeU64(<_>::default()),
                AlgebraicType::I128 => UniqueBTreeI128(<_>::default()),
                AlgebraicType::U128 => UniqueBTreeU128(<_>::default()),
                AlgebraicType::I256 => UniqueBTreeI256(<_>::default()),
                AlgebraicType::U256 => UniqueBTreeU256(<_>::default()),
                AlgebraicType::F32 => UniqueBTreeF32(<_>::default()),
                AlgebraicType::F64 => UniqueBTreeF64(<_>::default()),
                AlgebraicType::String => UniqueBTreeString(<_>::default()),
                // For a plain enum, use `u8` as the native type.
                // We use a direct index here
                AlgebraicType::Sum(sum) if sum.is_simple_enum() => UniqueBTreeSumTag(<_>::default()),

                // The index is either multi-column,
                // or we don't care to specialize on the key type,
                // so use a map keyed on `AlgebraicValue`.
                _ => UniqueBTreeAV(<_>::default()),
            }
        } else {
            match key_type {
                AlgebraicType::Bool => BTreeBool(<_>::default()),
                AlgebraicType::I8 => BTreeI8(<_>::default()),
                AlgebraicType::U8 => BTreeU8(<_>::default()),
                AlgebraicType::I16 => BTreeI16(<_>::default()),
                AlgebraicType::U16 => BTreeU16(<_>::default()),
                AlgebraicType::I32 => BTreeI32(<_>::default()),
                AlgebraicType::U32 => BTreeU32(<_>::default()),
                AlgebraicType::I64 => BTreeI64(<_>::default()),
                AlgebraicType::U64 => BTreeU64(<_>::default()),
                AlgebraicType::I128 => BTreeI128(<_>::default()),
                AlgebraicType::U128 => BTreeU128(<_>::default()),
                AlgebraicType::I256 => BTreeI256(<_>::default()),
                AlgebraicType::U256 => BTreeU256(<_>::default()),
                AlgebraicType::F32 => BTreeF32(<_>::default()),
                AlgebraicType::F64 => BTreeF64(<_>::default()),
                AlgebraicType::String => BTreeString(<_>::default()),

                // For a plain enum, use `u8` as the native type.
                AlgebraicType::Sum(sum) if sum.is_simple_enum() => BTreeSumTag(<_>::default()),

                // The index is either multi-column,
                // or we don't care to specialize on the key type,
                // so use a map keyed on `AlgebraicValue`.
                _ => BTreeAV(<_>::default()),
            }
        }
    }

    /// Returns a new hash index with key being of `key_type`.
    fn new_hash_index(key_type: &AlgebraicType, is_unique: bool) -> Self {
        use TypedIndex::*;

        // If the index is on a single column of a primitive type, string, or plain enum,
        // use a homogeneous map with a native key type.
        if is_unique {
            match key_type {
                AlgebraicType::Bool => UniqueHashBool(<_>::default()),
                AlgebraicType::I8 => UniqueHashI8(<_>::default()),
                AlgebraicType::U8 => UniqueHashU8(<_>::default()),
                AlgebraicType::I16 => UniqueHashI16(<_>::default()),
                AlgebraicType::U16 => UniqueHashU16(<_>::default()),
                AlgebraicType::I32 => UniqueHashI32(<_>::default()),
                AlgebraicType::U32 => UniqueHashU32(<_>::default()),
                AlgebraicType::I64 => UniqueHashI64(<_>::default()),
                AlgebraicType::U64 => UniqueHashU64(<_>::default()),
                AlgebraicType::I128 => UniqueHashI128(<_>::default()),
                AlgebraicType::U128 => UniqueHashU128(<_>::default()),
                AlgebraicType::I256 => UniqueHashI256(<_>::default()),
                AlgebraicType::U256 => UniqueHashU256(<_>::default()),
                AlgebraicType::F32 => UniqueHashF32(<_>::default()),
                AlgebraicType::F64 => UniqueHashF64(<_>::default()),
                AlgebraicType::String => UniqueHashString(<_>::default()),
                // For a plain enum, use `u8` as the native type.
                // We use a direct index here
                AlgebraicType::Sum(sum) if sum.is_simple_enum() => UniqueHashSumTag(<_>::default()),

                // The index is either multi-column,
                // or we don't care to specialize on the key type,
                // so use a map keyed on `AlgebraicValue`.
                _ => UniqueHashAV(<_>::default()),
            }
        } else {
            match key_type {
                AlgebraicType::Bool => HashBool(<_>::default()),
                AlgebraicType::I8 => HashI8(<_>::default()),
                AlgebraicType::U8 => HashU8(<_>::default()),
                AlgebraicType::I16 => HashI16(<_>::default()),
                AlgebraicType::U16 => HashU16(<_>::default()),
                AlgebraicType::I32 => HashI32(<_>::default()),
                AlgebraicType::U32 => HashU32(<_>::default()),
                AlgebraicType::I64 => HashI64(<_>::default()),
                AlgebraicType::U64 => HashU64(<_>::default()),
                AlgebraicType::I128 => HashI128(<_>::default()),
                AlgebraicType::U128 => HashU128(<_>::default()),
                AlgebraicType::I256 => HashI256(<_>::default()),
                AlgebraicType::U256 => HashU256(<_>::default()),
                AlgebraicType::F32 => HashF32(<_>::default()),
                AlgebraicType::F64 => HashF64(<_>::default()),
                AlgebraicType::String => HashString(<_>::default()),

                // For a plain enum, use `u8` as the native type.
                AlgebraicType::Sum(sum) if sum.is_simple_enum() => HashSumTag(<_>::default()),

                // The index is either multi-column,
                // or we don't care to specialize on the key type,
                // so use a map keyed on `AlgebraicValue`.
                _ => HashAV(<_>::default()),
            }
        }
    }

    /// Clones the structure of this index but not the indexed elements,
    /// so the returned index is empty.
    fn clone_structure(&self) -> Self {
        same_for_all_types!(self, this => this.clone_structure().into())
    }

    /// Returns whether this is a unique index or not.
    fn is_unique(&self) -> bool {
        use TypedIndex::*;
        match self {
            BTreeBool(_) | BTreeU8(_) | BTreeSumTag(_) | BTreeI8(_) | BTreeU16(_) | BTreeI16(_) | BTreeU32(_)
            | BTreeI32(_) | BTreeU64(_) | BTreeI64(_) | BTreeU128(_) | BTreeI128(_) | BTreeU256(_) | BTreeI256(_)
            | BTreeF32(_) | BTreeF64(_) | BTreeString(_) | BTreeAV(_) | HashBool(_) | HashU8(_) | HashSumTag(_)
            | HashI8(_) | HashU16(_) | HashI16(_) | HashU32(_) | HashI32(_) | HashU64(_) | HashI64(_) | HashU128(_)
            | HashI128(_) | HashU256(_) | HashI256(_) | HashF32(_) | HashF64(_) | HashString(_) | HashAV(_) => false,
            UniqueBTreeBool(_)
            | UniqueBTreeU8(_)
            | UniqueBTreeSumTag(_)
            | UniqueBTreeI8(_)
            | UniqueBTreeU16(_)
            | UniqueBTreeI16(_)
            | UniqueBTreeU32(_)
            | UniqueBTreeI32(_)
            | UniqueBTreeU64(_)
            | UniqueBTreeI64(_)
            | UniqueBTreeU128(_)
            | UniqueBTreeI128(_)
            | UniqueBTreeU256(_)
            | UniqueBTreeI256(_)
            | UniqueBTreeF32(_)
            | UniqueBTreeF64(_)
            | UniqueBTreeString(_)
            | UniqueBTreeAV(_)
            | UniqueHashBool(_)
            | UniqueHashU8(_)
            | UniqueHashSumTag(_)
            | UniqueHashI8(_)
            | UniqueHashU16(_)
            | UniqueHashI16(_)
            | UniqueHashU32(_)
            | UniqueHashI32(_)
            | UniqueHashU64(_)
            | UniqueHashI64(_)
            | UniqueHashU128(_)
            | UniqueHashI128(_)
            | UniqueHashU256(_)
            | UniqueHashI256(_)
            | UniqueHashF32(_)
            | UniqueHashF64(_)
            | UniqueHashString(_)
            | UniqueHashAV(_)
            | UniqueDirectU8(_)
            | UniqueDirectSumTag(_)
            | UniqueDirectU16(_)
            | UniqueDirectU32(_)
            | UniqueDirectU64(_) => true,
        }
    }

    /// Add the relation `key -> ptr` to the index.
    ///
    /// Returns `Errs(existing_row)` if this index was a unique index that was violated.
    /// The index is not inserted to in that case.
    #[inline]
    fn insert(&mut self, key: TypedIndexKey<'_>, ptr: RowPointer) -> Result<(), RowPointer> {
        /// Avoid inlining the closure into the common path.
        #[cold]
        #[inline(never)]
        fn outlined_call<R>(work: impl FnOnce() -> R) -> R {
            work()
        }

        fn direct<K: KeySize + Ord + ToFromUsize>(
            index: &mut UniqueDirectIndex<K>,
            key: K,
            ptr: RowPointer,
        ) -> (Result<(), RowPointer>, Option<TypedIndex>)
        where
            TypedIndex: From<UniqueBTreeIndex<K>>,
        {
            match index.insert_maybe_despecialize(key, ptr) {
                Ok(res) => (res, None),
                Err(Despecialize) => outlined_call(|| {
                    let mut index = index.into_btree();
                    let res = index.insert(key, ptr);
                    (res, Some(index.into()))
                }),
            }
        }

        use TypedIndex::*;
        use TypedIndexKey::*;
        let (res, new) = match (&mut *self, key) {
            (BTreeBool(i), Bool(k)) => (i.insert(k, ptr), None),
            (BTreeU8(i), U8(k)) => (i.insert(k, ptr), None),
            (BTreeSumTag(i), SumTag(k)) => (i.insert(k, ptr), None),
            (BTreeI8(i), I8(k)) => (i.insert(k, ptr), None),
            (BTreeU16(i), U16(k)) => (i.insert(k, ptr), None),
            (BTreeI16(i), I16(k)) => (i.insert(k, ptr), None),
            (BTreeU32(i), U32(k)) => (i.insert(k, ptr), None),
            (BTreeI32(i), I32(k)) => (i.insert(k, ptr), None),
            (BTreeU64(i), U64(k)) => (i.insert(k, ptr), None),
            (BTreeI64(i), I64(k)) => (i.insert(k, ptr), None),
            (BTreeU128(i), U128(k)) => (i.insert(k, ptr), None),
            (BTreeI128(i), I128(k)) => (i.insert(k, ptr), None),
            (BTreeU256(i), U256(k)) => (i.insert(k, ptr), None),
            (BTreeI256(i), I256(k)) => (i.insert(k, ptr), None),
            (BTreeF32(i), F32(k)) => (i.insert(k, ptr), None),
            (BTreeF64(i), F64(k)) => (i.insert(k, ptr), None),
            (BTreeString(i), String(k)) => (i.insert(k.into_owned(), ptr), None),
            (BTreeAV(i), AV(k)) => (i.insert(k.into_owned(), ptr), None),
            (HashBool(i), Bool(k)) => (i.insert(k, ptr), None),
            (HashU8(i), U8(k)) => (i.insert(k, ptr), None),
            (HashSumTag(i), SumTag(k)) => (i.insert(k, ptr), None),
            (HashI8(i), I8(k)) => (i.insert(k, ptr), None),
            (HashU16(i), U16(k)) => (i.insert(k, ptr), None),
            (HashI16(i), I16(k)) => (i.insert(k, ptr), None),
            (HashU32(i), U32(k)) => (i.insert(k, ptr), None),
            (HashI32(i), I32(k)) => (i.insert(k, ptr), None),
            (HashU64(i), U64(k)) => (i.insert(k, ptr), None),
            (HashI64(i), I64(k)) => (i.insert(k, ptr), None),
            (HashU128(i), U128(k)) => (i.insert(k, ptr), None),
            (HashI128(i), I128(k)) => (i.insert(k, ptr), None),
            (HashU256(i), U256(k)) => (i.insert(k, ptr), None),
            (HashI256(i), I256(k)) => (i.insert(k, ptr), None),
            (HashF32(i), F32(k)) => (i.insert(k, ptr), None),
            (HashF64(i), F64(k)) => (i.insert(k, ptr), None),
            (HashString(i), String(k)) => (i.insert(k.into_owned(), ptr), None),
            (HashAV(i), AV(k)) => (i.insert(k.into_owned(), ptr), None),
            (UniqueBTreeBool(i), Bool(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeU8(i), U8(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeSumTag(i), SumTag(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeI8(i), I8(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeU16(i), U16(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeI16(i), I16(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeU32(i), U32(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeI32(i), I32(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeU64(i), U64(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeI64(i), I64(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeU128(i), U128(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeI128(i), I128(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeU256(i), U256(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeI256(i), I256(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeF32(i), F32(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeF64(i), F64(k)) => (i.insert(k, ptr), None),
            (UniqueBTreeString(i), String(k)) => (i.insert(k.into_owned(), ptr), None),
            (UniqueBTreeAV(i), AV(k)) => (i.insert(k.into_owned(), ptr), None),
            (UniqueHashBool(i), Bool(k)) => (i.insert(k, ptr), None),
            (UniqueHashU8(i), U8(k)) => (i.insert(k, ptr), None),
            (UniqueHashSumTag(i), SumTag(k)) => (i.insert(k, ptr), None),
            (UniqueHashI8(i), I8(k)) => (i.insert(k, ptr), None),
            (UniqueHashU16(i), U16(k)) => (i.insert(k, ptr), None),
            (UniqueHashI16(i), I16(k)) => (i.insert(k, ptr), None),
            (UniqueHashU32(i), U32(k)) => (i.insert(k, ptr), None),
            (UniqueHashI32(i), I32(k)) => (i.insert(k, ptr), None),
            (UniqueHashU64(i), U64(k)) => (i.insert(k, ptr), None),
            (UniqueHashI64(i), I64(k)) => (i.insert(k, ptr), None),
            (UniqueHashU128(i), U128(k)) => (i.insert(k, ptr), None),
            (UniqueHashI128(i), I128(k)) => (i.insert(k, ptr), None),
            (UniqueHashU256(i), U256(k)) => (i.insert(k, ptr), None),
            (UniqueHashI256(i), I256(k)) => (i.insert(k, ptr), None),
            (UniqueHashF32(i), F32(k)) => (i.insert(k, ptr), None),
            (UniqueHashF64(i), F64(k)) => (i.insert(k, ptr), None),
            (UniqueHashString(i), String(k)) => (i.insert(k.into_owned(), ptr), None),
            (UniqueHashAV(i), AV(k)) => (i.insert(k.into_owned(), ptr), None),
            (UniqueDirectSumTag(i), SumTag(k)) => (i.insert(k, ptr), None),

            (UniqueDirectU8(i), U8(k)) => direct(i, k, ptr),
            (UniqueDirectU16(i), U16(k)) => direct(i, k, ptr),
            (UniqueDirectU32(i), U32(k)) => direct(i, k, ptr),
            (UniqueDirectU64(i), U64(k)) => direct(i, k, ptr),

            _ => panic!("{}", WRONG_TYPE),
        };

        if let Some(new) = new {
            *self = new;
        }

        res
    }

    /// Remove the relation `key -> ptr` from the index.
    ///
    /// Returns whether the row was present and has now been deleted.
    ///
    /// Panics if `key` is inconsistent with `self`.
    #[inline]
    fn delete(&mut self, key: &TypedIndexKey<'_>, ptr: RowPointer) -> bool {
        use TypedIndex::*;
        use TypedIndexKey::*;
        match (self, key) {
            (BTreeBool(i), Bool(k)) => i.delete(k, ptr),
            (BTreeU8(i), U8(k)) => i.delete(k, ptr),
            (BTreeSumTag(i), SumTag(k)) => i.delete(k, ptr),
            (BTreeI8(i), I8(k)) => i.delete(k, ptr),
            (BTreeU16(i), U16(k)) => i.delete(k, ptr),
            (BTreeI16(i), I16(k)) => i.delete(k, ptr),
            (BTreeU32(i), U32(k)) => i.delete(k, ptr),
            (BTreeI32(i), I32(k)) => i.delete(k, ptr),
            (BTreeU64(i), U64(k)) => i.delete(k, ptr),
            (BTreeI64(i), I64(k)) => i.delete(k, ptr),
            (BTreeU128(i), U128(k)) => i.delete(k, ptr),
            (BTreeI128(i), I128(k)) => i.delete(k, ptr),
            (BTreeU256(i), U256(k)) => i.delete(k, ptr),
            (BTreeI256(i), I256(k)) => i.delete(k, ptr),
            (BTreeF32(i), F32(k)) => i.delete(k, ptr),
            (BTreeF64(i), F64(k)) => i.delete(k, ptr),
            (BTreeString(i), String(k)) => i.delete(k.borrow(), ptr),
            (BTreeAV(i), AV(k)) => i.delete(k.borrow(), ptr),
            (HashBool(i), Bool(k)) => i.delete(k, ptr),
            (HashU8(i), U8(k)) => i.delete(k, ptr),
            (HashSumTag(i), SumTag(k)) => i.delete(k, ptr),
            (HashI8(i), I8(k)) => i.delete(k, ptr),
            (HashU16(i), U16(k)) => i.delete(k, ptr),
            (HashI16(i), I16(k)) => i.delete(k, ptr),
            (HashU32(i), U32(k)) => i.delete(k, ptr),
            (HashI32(i), I32(k)) => i.delete(k, ptr),
            (HashU64(i), U64(k)) => i.delete(k, ptr),
            (HashI64(i), I64(k)) => i.delete(k, ptr),
            (HashU128(i), U128(k)) => i.delete(k, ptr),
            (HashI128(i), I128(k)) => i.delete(k, ptr),
            (HashU256(i), U256(k)) => i.delete(k, ptr),
            (HashI256(i), I256(k)) => i.delete(k, ptr),
            (HashF32(i), F32(k)) => i.delete(k, ptr),
            (HashF64(i), F64(k)) => i.delete(k, ptr),
            (HashString(i), String(k)) => i.delete(k.borrow(), ptr),
            (HashAV(i), AV(k)) => i.delete(k.borrow(), ptr),
            (UniqueBTreeBool(i), Bool(k)) => i.delete(k, ptr),
            (UniqueBTreeU8(i), U8(k)) => i.delete(k, ptr),
            (UniqueBTreeSumTag(i), SumTag(k)) => i.delete(k, ptr),
            (UniqueBTreeI8(i), I8(k)) => i.delete(k, ptr),
            (UniqueBTreeU16(i), U16(k)) => i.delete(k, ptr),
            (UniqueBTreeI16(i), I16(k)) => i.delete(k, ptr),
            (UniqueBTreeU32(i), U32(k)) => i.delete(k, ptr),
            (UniqueBTreeI32(i), I32(k)) => i.delete(k, ptr),
            (UniqueBTreeU64(i), U64(k)) => i.delete(k, ptr),
            (UniqueBTreeI64(i), I64(k)) => i.delete(k, ptr),
            (UniqueBTreeU128(i), U128(k)) => i.delete(k, ptr),
            (UniqueBTreeI128(i), I128(k)) => i.delete(k, ptr),
            (UniqueBTreeU256(i), U256(k)) => i.delete(k, ptr),
            (UniqueBTreeI256(i), I256(k)) => i.delete(k, ptr),
            (UniqueBTreeF32(i), F32(k)) => i.delete(k, ptr),
            (UniqueBTreeF64(i), F64(k)) => i.delete(k, ptr),
            (UniqueBTreeString(i), String(k)) => i.delete(k.borrow(), ptr),
            (UniqueBTreeAV(i), AV(k)) => i.delete(k.borrow(), ptr),
            (UniqueHashBool(i), Bool(k)) => i.delete(k, ptr),
            (UniqueHashU8(i), U8(k)) => i.delete(k, ptr),
            (UniqueHashSumTag(i), SumTag(k)) => i.delete(k, ptr),
            (UniqueHashI8(i), I8(k)) => i.delete(k, ptr),
            (UniqueHashU16(i), U16(k)) => i.delete(k, ptr),
            (UniqueHashI16(i), I16(k)) => i.delete(k, ptr),
            (UniqueHashU32(i), U32(k)) => i.delete(k, ptr),
            (UniqueHashI32(i), I32(k)) => i.delete(k, ptr),
            (UniqueHashU64(i), U64(k)) => i.delete(k, ptr),
            (UniqueHashI64(i), I64(k)) => i.delete(k, ptr),
            (UniqueHashU128(i), U128(k)) => i.delete(k, ptr),
            (UniqueHashI128(i), I128(k)) => i.delete(k, ptr),
            (UniqueHashU256(i), U256(k)) => i.delete(k, ptr),
            (UniqueHashI256(i), I256(k)) => i.delete(k, ptr),
            (UniqueHashF32(i), F32(k)) => i.delete(k, ptr),
            (UniqueHashF64(i), F64(k)) => i.delete(k, ptr),
            (UniqueHashString(i), String(k)) => i.delete(k.borrow(), ptr),
            (UniqueHashAV(i), AV(k)) => i.delete(k.borrow(), ptr),
            (UniqueDirectSumTag(i), SumTag(k)) => i.delete(k, ptr),
            (UniqueDirectU8(i), U8(k)) => i.delete(k, ptr),
            (UniqueDirectU16(i), U16(k)) => i.delete(k, ptr),
            (UniqueDirectU32(i), U32(k)) => i.delete(k, ptr),
            (UniqueDirectU64(i), U64(k)) => i.delete(k, ptr),
            _ => panic!("{}", WRONG_TYPE),
        }
    }

    #[inline]
    fn seek_point(&self, key: &TypedIndexKey<'_>) -> TypedIndexPointIter<'_> {
        use TypedIndex::*;
        use TypedIndexKey::*;
        use TypedIndexPointIter::*;
        match (self, key) {
            (BTreeBool(this), Bool(key)) => NonUnique(this.seek_point(key)),
            (BTreeU8(this), U8(key)) => NonUnique(this.seek_point(key)),
            (BTreeSumTag(this), SumTag(key)) => NonUnique(this.seek_point(key)),
            (BTreeI8(this), I8(key)) => NonUnique(this.seek_point(key)),
            (BTreeU16(this), U16(key)) => NonUnique(this.seek_point(key)),
            (BTreeI16(this), I16(key)) => NonUnique(this.seek_point(key)),
            (BTreeU32(this), U32(key)) => NonUnique(this.seek_point(key)),
            (BTreeI32(this), I32(key)) => NonUnique(this.seek_point(key)),
            (BTreeU64(this), U64(key)) => NonUnique(this.seek_point(key)),
            (BTreeI64(this), I64(key)) => NonUnique(this.seek_point(key)),
            (BTreeU128(this), U128(key)) => NonUnique(this.seek_point(key)),
            (BTreeI128(this), I128(key)) => NonUnique(this.seek_point(key)),
            (BTreeU256(this), U256(key)) => NonUnique(this.seek_point(key)),
            (BTreeI256(this), I256(key)) => NonUnique(this.seek_point(key)),
            (BTreeF32(this), F32(key)) => NonUnique(this.seek_point(key)),
            (BTreeF64(this), F64(key)) => NonUnique(this.seek_point(key)),
            (BTreeString(this), String(key)) => NonUnique(this.seek_point(key.borrow())),
            (BTreeAV(this), AV(key)) => NonUnique(this.seek_point(key.borrow())),
            (HashBool(this), Bool(key)) => NonUnique(this.seek_point(key)),
            (HashU8(this), U8(key)) => NonUnique(this.seek_point(key)),
            (HashSumTag(this), SumTag(key)) => NonUnique(this.seek_point(key)),
            (HashI8(this), I8(key)) => NonUnique(this.seek_point(key)),
            (HashU16(this), U16(key)) => NonUnique(this.seek_point(key)),
            (HashI16(this), I16(key)) => NonUnique(this.seek_point(key)),
            (HashU32(this), U32(key)) => NonUnique(this.seek_point(key)),
            (HashI32(this), I32(key)) => NonUnique(this.seek_point(key)),
            (HashU64(this), U64(key)) => NonUnique(this.seek_point(key)),
            (HashI64(this), I64(key)) => NonUnique(this.seek_point(key)),
            (HashU128(this), U128(key)) => NonUnique(this.seek_point(key)),
            (HashI128(this), I128(key)) => NonUnique(this.seek_point(key)),
            (HashU256(this), U256(key)) => NonUnique(this.seek_point(key)),
            (HashI256(this), I256(key)) => NonUnique(this.seek_point(key)),
            (HashF32(this), F32(key)) => NonUnique(this.seek_point(key)),
            (HashF64(this), F64(key)) => NonUnique(this.seek_point(key)),
            (HashString(this), String(key)) => NonUnique(this.seek_point(key.borrow())),
            (HashAV(this), AV(key)) => NonUnique(this.seek_point(key.borrow())),
            (UniqueBTreeBool(this), Bool(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeU8(this), U8(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeSumTag(this), SumTag(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeI8(this), I8(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeU16(this), U16(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeI16(this), I16(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeU32(this), U32(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeI32(this), I32(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeU64(this), U64(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeI64(this), I64(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeU128(this), U128(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeI128(this), I128(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeU256(this), U256(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeI256(this), I256(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeF32(this), F32(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeF64(this), F64(key)) => Unique(this.seek_point(key)),
            (UniqueBTreeString(this), String(key)) => Unique(this.seek_point(key.borrow())),
            (UniqueBTreeAV(this), AV(key)) => Unique(this.seek_point(key.borrow())),
            (UniqueHashBool(this), Bool(key)) => Unique(this.seek_point(key)),
            (UniqueHashU8(this), U8(key)) => Unique(this.seek_point(key)),
            (UniqueHashSumTag(this), SumTag(key)) => Unique(this.seek_point(key)),
            (UniqueHashI8(this), I8(key)) => Unique(this.seek_point(key)),
            (UniqueHashU16(this), U16(key)) => Unique(this.seek_point(key)),
            (UniqueHashI16(this), I16(key)) => Unique(this.seek_point(key)),
            (UniqueHashU32(this), U32(key)) => Unique(this.seek_point(key)),
            (UniqueHashI32(this), I32(key)) => Unique(this.seek_point(key)),
            (UniqueHashU64(this), U64(key)) => Unique(this.seek_point(key)),
            (UniqueHashI64(this), I64(key)) => Unique(this.seek_point(key)),
            (UniqueHashU128(this), U128(key)) => Unique(this.seek_point(key)),
            (UniqueHashI128(this), I128(key)) => Unique(this.seek_point(key)),
            (UniqueHashU256(this), U256(key)) => Unique(this.seek_point(key)),
            (UniqueHashI256(this), I256(key)) => Unique(this.seek_point(key)),
            (UniqueHashF32(this), F32(key)) => Unique(this.seek_point(key)),
            (UniqueHashF64(this), F64(key)) => Unique(this.seek_point(key)),
            (UniqueHashString(this), String(key)) => Unique(this.seek_point(key.borrow())),
            (UniqueHashAV(this), AV(key)) => Unique(this.seek_point(key.borrow())),
            (UniqueDirectSumTag(this), SumTag(key)) => Unique(this.seek_point(key)),
            (UniqueDirectU8(this), U8(key)) => Unique(this.seek_point(key)),
            (UniqueDirectU16(this), U16(key)) => Unique(this.seek_point(key)),
            (UniqueDirectU32(this), U32(key)) => Unique(this.seek_point(key)),
            (UniqueDirectU64(this), U64(key)) => Unique(this.seek_point(key)),
            _ => panic!("{}", WRONG_TYPE),
        }
    }

    #[inline]
    fn seek_range<'a>(
        &self,
        range: &impl RangeBounds<TypedIndexKey<'a>>,
    ) -> IndexSeekRangeResult<TypedIndexRangeIter<'_>> {
        // Copied from `RangeBounds::is_empty` as it's unstable.
        // TODO(centril): replace once stable.
        fn is_empty<T: PartialOrd>(bounds: &impl RangeBounds<T>) -> bool {
            use core::ops::Bound::*;
            !match (bounds.start_bound(), bounds.end_bound()) {
                (Unbounded, _) | (_, Unbounded) => true,
                (Included(start), Excluded(end))
                | (Excluded(start), Included(end))
                | (Excluded(start), Excluded(end)) => start < end,
                (Included(start), Included(end)) => start <= end,
            }
        }

        fn map<'a, 'b: 'a, T: 'a + ?Sized>(
            range: &'a impl RangeBounds<TypedIndexKey<'b>>,
            map: impl Copy + FnOnce(&'a TypedIndexKey<'b>) -> Option<&'a T>,
        ) -> impl RangeBounds<T> + 'a {
            let as_key = |v| map(v).expect(WRONG_TYPE);
            let start = range.start_bound().map(as_key);
            let end = range.end_bound().map(as_key);
            (start, end)
        }

        use TypedIndexRangeIter::*;
        Ok(match self {
            // Hash indices are not `RangeIndex`.
            Self::HashBool(_)
            | Self::HashU8(_)
            | Self::HashSumTag(_)
            | Self::HashI8(_)
            | Self::HashU16(_)
            | Self::HashI16(_)
            | Self::HashU32(_)
            | Self::HashI32(_)
            | Self::HashU64(_)
            | Self::HashI64(_)
            | Self::HashU128(_)
            | Self::HashI128(_)
            | Self::HashF32(_)
            | Self::HashF64(_)
            | Self::HashU256(_)
            | Self::HashI256(_)
            | Self::HashString(_)
            | Self::HashAV(_)
            | Self::UniqueHashBool(_)
            | Self::UniqueHashU8(_)
            | Self::UniqueHashSumTag(_)
            | Self::UniqueHashI8(_)
            | Self::UniqueHashU16(_)
            | Self::UniqueHashI16(_)
            | Self::UniqueHashU32(_)
            | Self::UniqueHashI32(_)
            | Self::UniqueHashU64(_)
            | Self::UniqueHashI64(_)
            | Self::UniqueHashU128(_)
            | Self::UniqueHashI128(_)
            | Self::UniqueHashF32(_)
            | Self::UniqueHashF64(_)
            | Self::UniqueHashU256(_)
            | Self::UniqueHashI256(_)
            | Self::UniqueHashString(_)
            | Self::UniqueHashAV(_) => return Err(IndexCannotSeekRange),

            // Ensure we don't panic inside `BTreeMap::seek_range`.
            _ if is_empty(range) => RangeEmpty,

            Self::BTreeBool(this) => BTreeBool(this.seek_range(&map(range, TypedIndexKey::as_bool))),
            Self::BTreeU8(this) => BTreeU8(this.seek_range(&map(range, TypedIndexKey::as_u8))),
            Self::BTreeSumTag(this) => BTreeSumTag(this.seek_range(&map(range, TypedIndexKey::as_sum_tag))),
            Self::BTreeI8(this) => BTreeI8(this.seek_range(&map(range, TypedIndexKey::as_i8))),
            Self::BTreeU16(this) => BTreeU16(this.seek_range(&map(range, TypedIndexKey::as_u16))),
            Self::BTreeI16(this) => BTreeI16(this.seek_range(&map(range, TypedIndexKey::as_i16))),
            Self::BTreeU32(this) => BTreeU32(this.seek_range(&map(range, TypedIndexKey::as_u32))),
            Self::BTreeI32(this) => BTreeI32(this.seek_range(&map(range, TypedIndexKey::as_i32))),
            Self::BTreeU64(this) => BTreeU64(this.seek_range(&map(range, TypedIndexKey::as_u64))),
            Self::BTreeI64(this) => BTreeI64(this.seek_range(&map(range, TypedIndexKey::as_i64))),
            Self::BTreeU128(this) => BTreeU128(this.seek_range(&map(range, TypedIndexKey::as_u128))),
            Self::BTreeI128(this) => BTreeI128(this.seek_range(&map(range, TypedIndexKey::as_i128))),
            Self::BTreeU256(this) => BTreeU256(this.seek_range(&map(range, TypedIndexKey::as_u256))),
            Self::BTreeI256(this) => BTreeI256(this.seek_range(&map(range, TypedIndexKey::as_i256))),
            Self::BTreeF32(this) => BTreeF32(this.seek_range(&map(range, TypedIndexKey::as_f32))),
            Self::BTreeF64(this) => BTreeF64(this.seek_range(&map(range, TypedIndexKey::as_f64))),
            Self::BTreeString(this) => {
                let range = map(range, |k| k.as_string().map(|s| s.borrow()));
                BTreeString(this.seek_range(&range))
            }
            Self::BTreeAV(this) => BTreeAV(this.seek_range(&map(range, |k| k.as_av().map(|s| s.borrow())))),

            Self::UniqueBTreeBool(this) => UniqueBTreeBool(this.seek_range(&map(range, TypedIndexKey::as_bool))),
            Self::UniqueBTreeU8(this) => UniqueBTreeU8(this.seek_range(&map(range, TypedIndexKey::as_u8))),
            Self::UniqueBTreeSumTag(this) => UniqueBTreeSumTag(this.seek_range(&map(range, TypedIndexKey::as_sum_tag))),
            Self::UniqueBTreeI8(this) => UniqueBTreeI8(this.seek_range(&map(range, TypedIndexKey::as_i8))),
            Self::UniqueBTreeU16(this) => UniqueBTreeU16(this.seek_range(&map(range, TypedIndexKey::as_u16))),
            Self::UniqueBTreeI16(this) => UniqueBTreeI16(this.seek_range(&map(range, TypedIndexKey::as_i16))),
            Self::UniqueBTreeU32(this) => UniqueBTreeU32(this.seek_range(&map(range, TypedIndexKey::as_u32))),
            Self::UniqueBTreeI32(this) => UniqueBTreeI32(this.seek_range(&map(range, TypedIndexKey::as_i32))),
            Self::UniqueBTreeU64(this) => UniqueBTreeU64(this.seek_range(&map(range, TypedIndexKey::as_u64))),
            Self::UniqueBTreeI64(this) => UniqueBTreeI64(this.seek_range(&map(range, TypedIndexKey::as_i64))),
            Self::UniqueBTreeU128(this) => UniqueBTreeU128(this.seek_range(&map(range, TypedIndexKey::as_u128))),
            Self::UniqueBTreeI128(this) => UniqueBTreeI128(this.seek_range(&map(range, TypedIndexKey::as_i128))),
            Self::UniqueBTreeU256(this) => UniqueBTreeU256(this.seek_range(&map(range, TypedIndexKey::as_u256))),
            Self::UniqueBTreeI256(this) => UniqueBTreeI256(this.seek_range(&map(range, TypedIndexKey::as_i256))),
            Self::UniqueBTreeF32(this) => UniqueBTreeF32(this.seek_range(&map(range, TypedIndexKey::as_f32))),
            Self::UniqueBTreeF64(this) => UniqueBTreeF64(this.seek_range(&map(range, TypedIndexKey::as_f64))),
            Self::UniqueBTreeString(this) => {
                let range = map(range, |k| k.as_string().map(|s| s.borrow()));
                UniqueBTreeString(this.seek_range(&range))
            }
            Self::UniqueBTreeAV(this) => UniqueBTreeAV(this.seek_range(&map(range, |k| k.as_av().map(|s| s.borrow())))),

            Self::UniqueDirectSumTag(this) => UniqueDirectU8(this.seek_range(&map(range, TypedIndexKey::as_sum_tag))),
            Self::UniqueDirectU8(this) => UniqueDirect(this.seek_range(&map(range, TypedIndexKey::as_u8))),
            Self::UniqueDirectU16(this) => UniqueDirect(this.seek_range(&map(range, TypedIndexKey::as_u16))),
            Self::UniqueDirectU32(this) => UniqueDirect(this.seek_range(&map(range, TypedIndexKey::as_u32))),
            Self::UniqueDirectU64(this) => UniqueDirect(this.seek_range(&map(range, TypedIndexKey::as_u64))),
        })
    }

    fn clear(&mut self) {
        same_for_all_types!(self, this => this.clear())
    }

    #[allow(unused)] // used only by tests
    fn is_empty(&self) -> bool {
        self.num_rows() == 0
    }

    /// The number of rows stored in this index.
    ///
    /// Note that, for non-unique indexes, this may be larger than [`Self::num_keys`].
    ///
    /// This method runs in constant time.
    fn num_rows(&self) -> usize {
        same_for_all_types!(self, this => this.num_rows())
    }

    fn num_keys(&self) -> usize {
        same_for_all_types!(self, this => this.num_keys())
    }

    /// The number of bytes stored in keys in this index.
    ///
    /// For non-unique indexes, duplicate keys are counted once for each row that refers to them,
    /// even though the internal storage may deduplicate them as an optimization.
    ///
    /// This method runs in constant time.
    ///
    /// See the [`KeySize`] trait for more details on how this method computes its result.
    pub fn num_key_bytes(&self) -> u64 {
        same_for_all_types!(self, this => this.num_key_bytes())
    }
}

/// A key into a [`TableIndex`].
pub struct IndexKey<'a> {
    key: TypedIndexKey<'a>,
}

impl IndexKey<'_> {
    /// Converts the key into an [`AlgebraicValue`].
    pub fn into_algebraic_value(self) -> AlgebraicValue {
        self.key.into_algebraic_value()
    }
}

/// An index on a set of [`ColId`]s of a table.
#[derive(Debug, PartialEq, Eq)]
pub struct TableIndex {
    /// The actual index, specialized for the appropriate key type.
    idx: TypedIndex,
    /// The key type of this index.
    /// This is the projection of the row type to the types of the columns indexed.
    // NOTE(centril): This is accessed in index scan ABIs for decoding, so don't `Box<_>` it.
    pub key_type: AlgebraicType,

    /// Given a full row, typed at some `ty: ProductType`,
    /// these columns are the ones that this index indexes.
    /// Projecting the `ty` to `self.indexed_columns` yields the index's type `self.key_type`.
    pub indexed_columns: ColList,
}

impl MemoryUsage for TableIndex {
    fn heap_usage(&self) -> usize {
        let Self {
            idx,
            key_type,
            indexed_columns,
        } = self;
        idx.heap_usage() + key_type.heap_usage() + indexed_columns.heap_usage()
    }
}

static_assert_size!(TableIndex, 96);

impl TableIndex {
    /// Returns a new possibly unique index, with `index_id` for a choice of indexing algorithm.
    pub fn new(
        row_type: &ProductType,
        indexed_columns: ColList,
        index_kind: IndexKind,
        is_unique: bool,
    ) -> Result<Self, InvalidFieldError> {
        let key_type = row_type.project(&indexed_columns)?;
        let typed_index = TypedIndex::new(&key_type, index_kind, is_unique);
        Ok(Self {
            idx: typed_index,
            key_type,
            indexed_columns,
        })
    }

    /// Clones the structure of this index but not the indexed elements,
    /// so the returned index is empty.
    pub fn clone_structure(&self) -> Self {
        let key_type = self.key_type.clone();
        let idx = self.idx.clone_structure();
        let indexed_columns = self.indexed_columns.clone();
        Self {
            idx,
            key_type,
            indexed_columns,
        }
    }

    /// Returns whether this is a unique index or not.
    pub fn is_unique(&self) -> bool {
        self.idx.is_unique()
    }

    /// Derives a key for this index from `value`.
    ///
    /// Panics if `value` is not consistent with this index's key type.
    #[inline]
    pub fn key_from_algebraic_value<'a>(&self, value: &'a AlgebraicValue) -> IndexKey<'a> {
        let key = TypedIndexKey::from_algebraic_value(&self.idx, value);
        IndexKey { key }
    }

    /// Derives a key for this index from BSATN-encoded `bytes`.
    ///
    /// Returns an error if `bytes` is not properly encoded for this index's key type.
    #[inline]
    pub fn key_from_bsatn<'de>(&self, bytes: &'de [u8]) -> Result<IndexKey<'de>, DecodeError> {
        let key = TypedIndexKey::from_bsatn(&self.idx, &self.key_type, bytes)?;
        Ok(IndexKey { key })
    }

    /// Derives a key for this index from `row_ref`.
    ///
    /// # Safety
    ///
    /// Caller promises that the projection of `row_ref`'s type's
    /// to the indexed column equals the index's key type.
    #[inline]
    pub unsafe fn key_from_row<'a>(&self, row_ref: RowRef<'a>) -> IndexKey<'a> {
        // SAFETY:
        // 1. We're passing the same `ColList` that was provided during construction.
        // 2. Forward caller requirements.
        let key = unsafe { TypedIndexKey::from_row_ref(&self.idx, &self.indexed_columns, row_ref) };
        IndexKey { key }
    }

    /// Inserts `ptr` with the value `row` to this index.
    /// This index will extract the necessary values from `row` based on `self.indexed_columns`.
    ///
    /// Returns `Err(existing_row)` if this insertion would violate a unique constraint.
    ///
    /// # Safety
    ///
    /// Caller promises that projecting the `row_ref`'s type
    /// to the index's columns equals the index's key type.
    /// This is entailed by an index belonging to the table's schema.
    /// It also follows from `row_ref`'s type/layout
    /// being the same as passed in on `self`'s construction.
    pub unsafe fn check_and_insert(&mut self, row_ref: RowRef<'_>) -> Result<(), RowPointer> {
        // SAFETY: Forward the caller's proof obligation.
        let key = unsafe { self.key_from_row(row_ref).key };
        self.idx.insert(key, row_ref.pointer())
    }

    /// Deletes `row_ref` with its indexed value `row_ref.project(&self.indexed_columns)` from this index.
    ///
    /// Returns whether `ptr` was present.
    ///
    /// # Safety
    ///
    /// Caller promises that projecting the `row_ref`'s type
    /// to the index's columns equals the index's key type.
    /// This is entailed by an index belonging to the table's schema.
    /// It also follows from `row_ref`'s type/layout
    /// being the same as passed in on `self`'s construction.
    pub unsafe fn delete(&mut self, row_ref: RowRef<'_>) -> bool {
        // SAFETY: Forward the caller's proof obligation.
        let key = unsafe { self.key_from_row(row_ref).key };
        self.idx.delete(&key.borrowed(), row_ref.pointer())
    }

    /// Returns whether `value` is in this index.
    pub fn contains_any(&self, value: &AlgebraicValue) -> bool {
        let key = self.key_from_algebraic_value(value);
        self.seek_point(&key).next().is_some()
    }

    /// Returns the number of rows associated with this `value`.
    /// Returns `None` if 0.
    /// Returns `Some(1)` if the index is unique.
    pub fn count(&self, value: &AlgebraicValue) -> Option<usize> {
        let key = self.key_from_algebraic_value(value);
        match self.seek_point(&key).count() {
            0 => None,
            n => Some(n),
        }
    }

    /// Returns an iterator that yields all the `RowPointer`s for the given `key`.
    #[inline]
    pub fn seek_point(&self, key: &IndexKey<'_>) -> TableIndexPointIter<'_> {
        let iter = self.idx.seek_point(&key.key);
        TableIndexPointIter { iter }
    }

    /// Returns an iterator over the [TableIndex],
    /// that yields all the `RowPointer`s,
    /// that fall within the specified `range`,
    /// if the index is [`RangedIndex`].
    pub fn seek_range<'a>(
        &self,
        range: &impl RangeBounds<IndexKey<'a>>,
    ) -> IndexSeekRangeResult<TableIndexRangeIter<'_>> {
        let start = range.start_bound().map(|v| &v.key);
        let end = range.end_bound().map(|v| &v.key);
        let range = (start, end);
        let iter = self.idx.seek_range(&range)?;
        Ok(TableIndexRangeIter { iter })
    }

    /// Extends [`TableIndex`] with `rows`.
    ///
    /// Returns the first unique constraint violation caused when adding this index, if any.
    ///
    /// # Safety
    ///
    /// Caller promises that projecting any of the `row_ref`'s type
    /// to the index's columns equals the index's key type.
    /// This is entailed by an index belonging to the table's schema.
    /// It also follows from `row_ref`'s type/layout
    /// being the same as passed in on `self`'s construction.
    pub unsafe fn build_from_rows<'table>(
        &mut self,
        rows: impl IntoIterator<Item = RowRef<'table>>,
    ) -> Result<(), RowPointer> {
        rows.into_iter()
            // SAFETY: Forward caller proof obligation.
            .try_for_each(|row_ref| unsafe { self.check_and_insert(row_ref) })
    }

    /// Returns an error with the first unique constraint violation that
    /// would occur if `self` and `other` were to be merged.
    ///
    /// The closure `ignore` indicates whether a row in `self` should be ignored.
    pub fn can_merge(&self, other: &Self, ignore: impl Fn(&RowPointer) -> bool) -> Result<(), RowPointer> {
        use TypedIndex::*;
        match (&self.idx, &other.idx) {
            // For non-unique indices, it's always possible to merge.
            (BTreeBool(_), BTreeBool(_))
            | (BTreeU8(_), BTreeU8(_))
            | (BTreeSumTag(_), BTreeSumTag(_))
            | (BTreeI8(_), BTreeI8(_))
            | (BTreeU16(_), BTreeU16(_))
            | (BTreeI16(_), BTreeI16(_))
            | (BTreeU32(_), BTreeU32(_))
            | (BTreeI32(_), BTreeI32(_))
            | (BTreeU64(_), BTreeU64(_))
            | (BTreeI64(_), BTreeI64(_))
            | (BTreeU128(_), BTreeU128(_))
            | (BTreeI128(_), BTreeI128(_))
            | (BTreeU256(_), BTreeU256(_))
            | (BTreeI256(_), BTreeI256(_))
            | (BTreeF32(_), BTreeF32(_))
            | (BTreeF64(_), BTreeF64(_))
            | (BTreeString(_), BTreeString(_))
            | (BTreeAV(_), BTreeAV(_))
            | (HashBool(_), HashBool(_))
            | (HashU8(_), HashU8(_))
            | (HashSumTag(_), HashSumTag(_))
            | (HashI8(_), HashI8(_))
            | (HashU16(_), HashU16(_))
            | (HashI16(_), HashI16(_))
            | (HashU32(_), HashU32(_))
            | (HashI32(_), HashI32(_))
            | (HashU64(_), HashU64(_))
            | (HashI64(_), HashI64(_))
            | (HashU128(_), HashU128(_))
            | (HashI128(_), HashI128(_))
            | (HashU256(_), HashU256(_))
            | (HashI256(_), HashI256(_))
            | (HashF32(_), HashF32(_))
            | (HashF64(_), HashF64(_))
            | (HashString(_), HashString(_))
            | (HashAV(_), HashAV(_)) => Ok(()),
            // For unique indices, we'll need to see if everything in `other` can be added to `idx`.
            (UniqueBTreeBool(idx), UniqueBTreeBool(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeU8(idx), UniqueBTreeU8(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeSumTag(idx), UniqueBTreeSumTag(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeI8(idx), UniqueBTreeI8(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeU16(idx), UniqueBTreeU16(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeI16(idx), UniqueBTreeI16(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeU32(idx), UniqueBTreeU32(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeI32(idx), UniqueBTreeI32(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeU64(idx), UniqueBTreeU64(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeI64(idx), UniqueBTreeI64(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeU128(idx), UniqueBTreeU128(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeI128(idx), UniqueBTreeI128(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeU256(idx), UniqueBTreeU256(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeI256(idx), UniqueBTreeI256(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeF32(idx), UniqueBTreeF32(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeF64(idx), UniqueBTreeF64(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeString(idx), UniqueBTreeString(other)) => idx.can_merge(other, ignore),
            (UniqueBTreeAV(idx), UniqueBTreeAV(other)) => idx.can_merge(other, ignore),
            (UniqueHashBool(idx), UniqueHashBool(other)) => idx.can_merge(other, ignore),
            (UniqueHashU8(idx), UniqueHashU8(other)) => idx.can_merge(other, ignore),
            (UniqueHashSumTag(idx), UniqueHashSumTag(other)) => idx.can_merge(other, ignore),
            (UniqueHashI8(idx), UniqueHashI8(other)) => idx.can_merge(other, ignore),
            (UniqueHashU16(idx), UniqueHashU16(other)) => idx.can_merge(other, ignore),
            (UniqueHashI16(idx), UniqueHashI16(other)) => idx.can_merge(other, ignore),
            (UniqueHashU32(idx), UniqueHashU32(other)) => idx.can_merge(other, ignore),
            (UniqueHashI32(idx), UniqueHashI32(other)) => idx.can_merge(other, ignore),
            (UniqueHashU64(idx), UniqueHashU64(other)) => idx.can_merge(other, ignore),
            (UniqueHashI64(idx), UniqueHashI64(other)) => idx.can_merge(other, ignore),
            (UniqueHashU128(idx), UniqueHashU128(other)) => idx.can_merge(other, ignore),
            (UniqueHashI128(idx), UniqueHashI128(other)) => idx.can_merge(other, ignore),
            (UniqueHashU256(idx), UniqueHashU256(other)) => idx.can_merge(other, ignore),
            (UniqueHashI256(idx), UniqueHashI256(other)) => idx.can_merge(other, ignore),
            (UniqueHashF32(idx), UniqueHashF32(other)) => idx.can_merge(other, ignore),
            (UniqueHashF64(idx), UniqueHashF64(other)) => idx.can_merge(other, ignore),
            (UniqueHashString(idx), UniqueHashString(other)) => idx.can_merge(other, ignore),
            (UniqueHashAV(idx), UniqueHashAV(other)) => idx.can_merge(other, ignore),
            (UniqueDirectU8(idx), UniqueDirectU8(other)) => idx.can_merge(other, ignore),
            (UniqueDirectSumTag(idx), UniqueDirectSumTag(other)) => idx.can_merge(other, ignore),
            (UniqueDirectU16(idx), UniqueDirectU16(other)) => idx.can_merge(other, ignore),
            (UniqueDirectU32(idx), UniqueDirectU32(other)) => idx.can_merge(other, ignore),
            (UniqueDirectU64(idx), UniqueDirectU64(other)) => idx.can_merge(other, ignore),

            _ => unreachable!("non-matching index kinds"),
        }
    }

    /// Deletes all entries from the index, leaving it empty.
    ///
    /// When inserting a newly-created index into the committed state,
    /// we clear the tx state's index and insert it,
    /// rather than constructing a new `TableIndex`.
    pub fn clear(&mut self) {
        self.idx.clear();
    }

    /// The number of unique keys in this index.
    pub fn num_keys(&self) -> usize {
        self.idx.num_keys()
    }

    /// The number of rows stored in this index.
    ///
    /// Note that, for non-unique indexes, this may be larger than [`Self::num_keys`].
    ///
    /// This method runs in constant time.
    pub fn num_rows(&self) -> u64 {
        self.idx.num_rows() as u64
    }

    /// The number of bytes stored in keys in this index.
    ///
    /// For non-unique indexes, duplicate keys are counted once for each row that refers to them,
    /// even though the internal storage may deduplicate them as an optimization.
    ///
    /// This method runs in constant time.
    ///
    /// See the [`KeySize`] trait for more details on how this method computes its result.
    pub fn num_key_bytes(&self) -> u64 {
        self.idx.num_key_bytes()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::page_pool::PagePool;
    use crate::{blob_store::HashMapBlobStore, table::test::table};
    use core::ops::Bound::*;
    use decorum::Total;
    use proptest::prelude::*;
    use proptest::{
        collection::{hash_set, vec},
        test_runner::TestCaseResult,
    };
    use spacetimedb_data_structures::map::HashMap;
    use spacetimedb_lib::ProductTypeElement;
    use spacetimedb_primitives::ColId;
    use spacetimedb_sats::algebraic_value::Packed;
    use spacetimedb_sats::proptest::{generate_algebraic_value, generate_primitive_algebraic_type};
    use spacetimedb_sats::{
        product,
        proptest::{generate_product_value, generate_row_type},
        AlgebraicType, ProductType, ProductValue,
    };

    fn gen_cols(ty_len: usize) -> impl Strategy<Value = ColList> {
        vec((0..ty_len as u16).prop_map_into::<ColId>(), 1..=ty_len)
            .prop_map(|cols| cols.into_iter().collect::<ColList>())
    }

    fn gen_row_and_cols() -> impl Strategy<Value = (ProductType, ColList, ProductValue)> {
        generate_row_type(1..16).prop_flat_map(|ty| {
            (
                Just(ty.clone()),
                gen_cols(ty.elements.len()),
                generate_product_value(ty),
            )
        })
    }

    impl IndexKind {
        /// Returns a strategy generating a ranged index kind.
        fn gen_for_ranged() -> impl Strategy<Value = Self> {
            any::<bool>().prop_map(|is_direct| if is_direct { Self::Direct } else { Self::BTree })
        }
    }

    fn new_index(row_type: &ProductType, cols: &ColList, is_unique: bool, kind: IndexKind) -> TableIndex {
        TableIndex::new(row_type, cols.clone(), kind, is_unique).unwrap()
    }

    /// Extracts from `row` the relevant column values according to what columns are indexed.
    fn get_fields(cols: &ColList, row: &ProductValue) -> AlgebraicValue {
        row.project(cols).unwrap()
    }

    /// Returns whether indexing `row` again would violate a unique constraint, if any.
    fn violates_unique_constraint(index: &TableIndex, cols: &ColList, row: &ProductValue) -> bool {
        !index.is_unique() || index.contains_any(&get_fields(cols, row))
    }

    /// Returns an iterator over the rows that would violate the unique constraint of this index,
    /// if `row` were inserted,
    /// or `None`, if this index doesn't have a unique constraint.
    fn get_rows_that_violate_unique_constraint<'a>(
        index: &'a TableIndex,
        row: &'a AlgebraicValue,
    ) -> Option<TableIndexPointIter<'a>> {
        index
            .is_unique()
            .then(|| index.seek_point(&index.key_from_algebraic_value(row)))
    }

    fn successor_of_primitive(av: &AlgebraicValue) -> Option<AlgebraicValue> {
        use AlgebraicValue::*;
        match av {
            Min | Max | Sum(_) | Product(_) | Array(_) | String(_) => unimplemented!(),

            Bool(false) => Some(Bool(true)),
            Bool(true) => None,
            I8(x) => x.checked_add(1).map(I8),
            U8(x) => x.checked_add(1).map(U8),
            I16(x) => x.checked_add(1).map(I16),
            U16(x) => x.checked_add(1).map(U16),
            I32(x) => x.checked_add(1).map(I32),
            U32(x) => x.checked_add(1).map(U32),
            I64(x) => x.checked_add(1).map(I64),
            U64(x) => x.checked_add(1).map(U64),
            I128(x) => x.0.checked_add(1).map(Packed).map(I128),
            U128(x) => x.0.checked_add(1).map(Packed).map(U128),
            I256(x) => x.checked_add(1.into()).map(Box::new).map(I256),
            U256(x) => x.checked_add(1u8.into()).map(Box::new).map(U256),
            F32(x) => Some(F32(Total::from_inner(x.into_inner().next_up()))),
            F64(x) => Some(F64(Total::from_inner(x.into_inner().next_up()))),
        }
    }

    fn gen_primitive_ty_and_val() -> impl Strategy<Value = (AlgebraicType, AlgebraicValue)> {
        generate_primitive_algebraic_type().prop_flat_map(|ty| (Just(ty.clone()), generate_algebraic_value(ty)))
    }

    fn seek_range<'a>(
        index: &'a TableIndex,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> IndexSeekRangeResult<TableIndexRangeIter<'a>> {
        let start = range.start_bound().map(|v| index.key_from_algebraic_value(v));
        let end = range.end_bound().map(|v| index.key_from_algebraic_value(v));
        index.seek_range(&(start, end))
    }

    proptest! {
        #![proptest_config(ProptestConfig { max_shrink_iters: 0x10000000, ..Default::default() })]

        #[test]
        fn hash_index_cannot_seek_range((ty, cols, pv) in gen_row_and_cols(), is_unique: bool) {
            let index = TableIndex::new(&ty, cols.clone(), IndexKind::Hash, is_unique).unwrap();

            let key = pv.project(&cols).unwrap();
            assert_eq!(seek_range(&index, &(key.clone()..=key)).unwrap_err(), IndexCannotSeekRange);
        }

        #[test]
        fn remove_nonexistent_noop((ty, cols, pv) in gen_row_and_cols(), kind: IndexKind, is_unique: bool) {
            let mut index = new_index(&ty, &cols, is_unique, kind);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&pool, &mut blob_store, &pv).unwrap().1;
            prop_assert_eq!(unsafe { index.delete(row_ref) }, false);
            prop_assert!(index.idx.is_empty());
            prop_assert_eq!(index.num_keys(), 0);
            prop_assert_eq!(index.num_key_bytes(), 0);
            prop_assert_eq!(index.num_rows(), 0);
        }

        #[test]
        fn insert_delete_noop((ty, cols, pv) in gen_row_and_cols(), kind: IndexKind, is_unique: bool) {
            let mut index = new_index(&ty, &cols, is_unique, kind);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&pool, &mut blob_store, &pv).unwrap().1;
            let value = get_fields(&cols, &pv);

            prop_assert_eq!(index.num_keys(), 0);
            prop_assert_eq!(index.num_rows(), 0);
            prop_assert_eq!(index.contains_any(&value), false);

            prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Ok(()));
            prop_assert_eq!(index.num_keys(), 1);
            prop_assert_eq!(index.num_rows(), 1);
            prop_assert_eq!(index.contains_any(&value), true);

            prop_assert_eq!(unsafe { index.delete(row_ref) }, true);
            prop_assert_eq!(index.num_keys(), 0);
            prop_assert_eq!(index.num_rows(), 0);
            prop_assert_eq!(index.contains_any(&value), false);
        }

        #[test]
        fn non_unique_allows_key_twice(
            (ty, cols, key) in gen_row_and_cols(),
            kind: IndexKind,
            vals in hash_set(any::<i32>(), 1..10)
        ) {
            // Add a field to `ty` so we can use the same key more than once.
            let mut ty = Vec::from(ty.elements);
            ty.push(ProductTypeElement::new_named(AlgebraicType::I32, "extra"));
            let ty = ProductType::from(ty.into_boxed_slice());

            let mut index = new_index(&ty, &cols, false, kind);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();

            let num_vals = vals.len();
            for val in vals {
                let mut key = Vec::from(key.clone().elements);
                key.push(val.into());
                let key = ProductValue::from(key);

                let row_ref = table.insert(&pool, &mut blob_store, &key).unwrap().1;

                // SAFETY: `row_ref` has the same type as was passed in when constructing `index`.
                prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Ok(()));
            }

            assert_eq!(index.num_keys(), 1);
            assert_eq!(index.num_rows() as usize, num_vals);
        }

        #[test]
        fn insert_again_violates_unique_constraint((ty, cols, pv) in gen_row_and_cols(), kind: IndexKind) {
            let mut index = new_index(&ty, &cols, true, kind);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&pool, &mut blob_store, &pv).unwrap().1;
            let value = get_fields(&cols, &pv);

            // Nothing in the index yet.
            prop_assert_eq!(index.num_rows(), 0);
            prop_assert_eq!(violates_unique_constraint(&index, &cols, &pv), false);
            prop_assert_eq!(
                get_rows_that_violate_unique_constraint(&index, &value).unwrap().collect::<Vec<_>>(),
                []
            );

            // Insert.
            // SAFETY: `row_ref` has the same type as was passed in when constructing `index`.
            prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Ok(()));

            // Inserting again would be a problem.
            prop_assert_eq!(index.num_keys(), 1);
            prop_assert_eq!(index.num_rows(), 1);
            prop_assert_eq!(violates_unique_constraint(&index, &cols, &pv), true);
            prop_assert_eq!(
                get_rows_that_violate_unique_constraint(&index, &value).unwrap().collect::<Vec<_>>(),
                [row_ref.pointer()]
            );
            // SAFETY: `row_ref` has the same type as was passed in when constructing `index`.
            prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Err(row_ref.pointer()));
            prop_assert_eq!(index.num_keys(), 1);
            prop_assert_eq!(index.num_rows(), 1);
        }

        #[test]
        fn seek_various_ranges(needle in 1..u64::MAX, is_unique: bool, kind in IndexKind::gen_for_ranged()) {
            use AlgebraicValue::U64 as V;

            let cols = 0.into();
            let ty = ProductType::from_iter([AlgebraicType::U64]);
            let mut index = new_index(&ty, &cols, is_unique, kind);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();

            let prev = needle - 1;
            let next = needle + 1;
            let range = prev..=next;

            let mut val_to_ptr = HashMap::default();

            // Insert `prev`, `needle`, and `next`.
            for x in range.clone() {
                let row = product![x];
                let row_ref = table.insert(&pool, &mut blob_store, &row).unwrap().1;
                val_to_ptr.insert(x, row_ref.pointer());
                // SAFETY: `row_ref` has the same type as was passed in when constructing `index`.
                prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Ok(()));
            }

            assert_eq!(index.num_keys(), 3);
            assert_eq!(index.num_rows(), 3);
            assert_eq!(index.num_key_bytes() as usize, 3 * size_of::<u64>());

            fn test_seek(index: &TableIndex, val_to_ptr: &HashMap<u64, RowPointer>, range: impl RangeBounds<AlgebraicValue>, expect: impl IntoIterator<Item = u64>) -> TestCaseResult {
                check_seek(seek_range(index, &range).unwrap().collect(), val_to_ptr, expect)
            }

            fn check_seek(mut ptrs_in_index: Vec<RowPointer>, val_to_ptr: &HashMap<u64, RowPointer>, expect: impl IntoIterator<Item = u64>) -> TestCaseResult {
                ptrs_in_index.sort();
                let mut expected_ptrs = expect.into_iter().map(|expected| val_to_ptr.get(&expected).unwrap()).copied().collect::<Vec<_>>();
                expected_ptrs.sort();
                prop_assert_eq!(
                    ptrs_in_index,
                    expected_ptrs
                );
                Ok(())
            }

            // Test point ranges.
            for x in range.clone() {
                test_seek(&index, &val_to_ptr, V(x), [x])?;
                check_seek(index.seek_point(&index.key_from_algebraic_value(&V(x))).collect(), &val_to_ptr, [x])?;
            }

            // Test `..` (`RangeFull`).
            test_seek(&index, &val_to_ptr, .., [prev, needle, next])?;

            // Test `x..` (`RangeFrom`).
            test_seek(&index, &val_to_ptr, V(prev).., [prev, needle, next])?;
            test_seek(&index, &val_to_ptr, V(needle).., [needle, next])?;
            test_seek(&index, &val_to_ptr, V(next).., [next])?;

            // Test `..x` (`RangeTo`).
            test_seek(&index, &val_to_ptr, ..V(prev), [])?;
            test_seek(&index, &val_to_ptr, ..V(needle), [prev])?;
            test_seek(&index, &val_to_ptr, ..V(next), [prev, needle])?;

            // Test `..=x` (`RangeToInclusive`).
            test_seek(&index, &val_to_ptr, ..=V(prev), [prev])?;
            test_seek(&index, &val_to_ptr, ..=V(needle), [prev, needle])?;
            test_seek(&index, &val_to_ptr, ..=V(next), [prev, needle, next])?;

            // Test `x..y` (`Range`).
            test_seek(&index, &val_to_ptr, V(prev)..V(prev), [])?;
            test_seek(&index, &val_to_ptr, V(prev)..V(needle), [prev])?;
            test_seek(&index, &val_to_ptr, V(prev)..V(next), [prev, needle])?;
            test_seek(&index, &val_to_ptr, V(needle)..V(next), [needle])?;

            // Test `x..=y` (`RangeInclusive`).
            test_seek(&index, &val_to_ptr, V(prev)..=V(prev), [prev])?;
            test_seek(&index, &val_to_ptr, V(prev)..=V(needle), [prev, needle])?;
            test_seek(&index, &val_to_ptr, V(prev)..=V(next), [prev, needle, next])?;
            test_seek(&index, &val_to_ptr, V(needle)..=V(next), [needle, next])?;
            test_seek(&index, &val_to_ptr, V(next)..=V(next), [next])?;

            // Test `(x, y]` (Exclusive start, inclusive end).
            test_seek(&index, &val_to_ptr, (Excluded(V(prev)), Included(V(prev))), [])?;
            test_seek(&index, &val_to_ptr, (Excluded(V(prev)), Included(V(needle))), [needle])?;
            test_seek(&index, &val_to_ptr, (Excluded(V(prev)), Included(V(next))), [needle, next])?;

            // Test `(x, inf]` (Exclusive start, unbounded end).
            test_seek(&index, &val_to_ptr, (Excluded(V(prev)), Unbounded), [needle, next])?;
            test_seek(&index, &val_to_ptr, (Excluded(V(needle)), Unbounded), [next])?;
            test_seek(&index, &val_to_ptr, (Excluded(V(next)), Unbounded), [])?;

            // Test `(x, y)` (Exclusive start, exclusive end).
            test_seek(&index, &val_to_ptr, (Excluded(V(prev)), Excluded(V(needle))), [])?;
            test_seek(&index, &val_to_ptr, (Excluded(V(prev)), Excluded(V(next))), [needle])?;
        }

        #[test]
        fn empty_range_scans_dont_panic((ty, val) in gen_primitive_ty_and_val(), is_unique: bool, kind in IndexKind::gen_for_ranged()) {
            let succ = successor_of_primitive(&val);
            prop_assume!(succ.is_some());
            let succ = succ.unwrap();

            // Construct the index.
            let row_ty = ProductType::from([ty.clone()]);
            let mut index = new_index(&row_ty, &[0].into(), is_unique, kind);

            // Construct the table and add `val` as a row.
            let mut table = table(row_ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let pv = product![val.clone()];
            let row_ref = table.insert(&pool, &mut blob_store, &pv).unwrap().1;

            // Add the row to the index.
            assert_eq!(index.num_keys(), 0);
            assert_eq!(index.num_rows(), 0);
            assert_eq!(index.num_key_bytes(), 0);
            unsafe { index.check_and_insert(row_ref).unwrap(); }
            assert_eq!(index.num_keys(), 1);
            assert_eq!(index.num_rows(), 1);

            // Seek the empty ranges.
            let rows = seek_range(&index, &(&succ..&val)).unwrap().collect::<Vec<_>>();
            assert_eq!(rows, []);
            let rows = seek_range(&index, &(&succ..=&val)).unwrap().collect::<Vec<_>>();
            assert_eq!(rows, []);
            let rows = seek_range(&index, &(Excluded(&succ), Included(&val))).unwrap().collect::<Vec<_>>();
            assert_eq!(rows, []);
            let rows = seek_range(&index, &(Excluded(&succ), Excluded(&val))).unwrap().collect::<Vec<_>>();
            assert_eq!(rows, []);
            let rows = seek_range(&index, &(Excluded(&val), Excluded(&val))).unwrap().collect::<Vec<_>>();
            assert_eq!(rows, []);
        }
    }
}
