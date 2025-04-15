use crate::{ConnectionId, Identity};
use core::ops;
use spacetimedb_sats::bsatn;
use spacetimedb_sats::{hash::Hash, i256, u256, Serialize};

/// Types which can appear as an argument to an index filtering operation
/// for a column of type `Column`.
///
/// Types which can appear specifically as a terminating bound in a BTree index,
/// which may be a range, instead use [`IndexScanRangeBoundsTerminator`].
/// Because SpacetimeDB supports a only restricted set of types as index keys,
/// only a small set of `Column` types have corresponding `FilterableValue` implementations.
/// Specifically, these types are:
/// - Signed and unsigned integers of various widths.
/// - [`bool`].
/// - [`String`], which is also filterable with `&str`.
/// - [`Identity`].
/// - [`ConnectionId`].
/// - [`Hash`](struct@Hash).
/// - No-payload enums annotated with `#[derive(SpacetimeType)]`.
///   No-payload enums are sometimes called "plain," "simple" or "C-style."
///   They are enums where no variant has any payload data.
//
// General rules for implementors of this type:
// - It should only be implemented for types that have
//   simple-to-implement consistent total equality and ordering
//   on all languages SpacetimeDB supports in both client and module SDKs.
//   This means that user-defined compound types other than C-style enums,
//   and arrays thereof,
//   should not implement it, as C# and TypeScript use reference equality for those types.
// - It should only be implemented for owned values if those values are `Copy`.
//   Otherwise it should only be implemented for references.
//   This is so that rustc and IDEs will recommend rewriting `x` to `&x` rather than `x.clone()`.
// - `Arg: FilterableValue<Column = Col>`
//   for any pair of types `(Arg, Col)` which meet the above criteria
//   is desirable if `Arg` and `Col` have the same BSATN layout.
//   E.g. `&str: FilterableValue<Column = String>` is desirable.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot appear as an argument to an index filtering operation",
    label = "should be an integer type, `bool`, `String`, `&str`, `Identity`, `ConnectionId`, `Hash` or a no-payload enum which derives `SpacetimeType`, not `{Self}`",
    note = "The allowed set of types are limited to integers, bool, strings, `Identity`, `ConnectionId`, `Hash` and no-payload enums which derive `SpacetimeType`,"
)]
pub trait FilterableValue: Serialize + Private {
    type Column;
}

/// Hidden supertrait for [`FilterableValue`],
/// to discourage users from hand-writing implementations.
///
/// We want to expose [`FilterableValue`] in the docs, but to prevent users from implementing it.
/// Normally, we would just make this `Private` trait inaccessible,
/// but we need to macro-generate implementations, so it must be `pub`.
/// We mark it `doc(hidden)` to discourage use.
#[doc(hidden)]
pub trait Private {}

macro_rules! impl_filterable_value {
    (@one $arg:ty => $col:ty) => {
        impl Private for $arg {}
        impl FilterableValue for $arg {
            type Column = $col;
        }
    };
    (@one $arg:ty: Copy) => {
        impl_filterable_value!(@one $arg => $arg);
        impl_filterable_value!(@one &$arg => $arg);
    };
    (@one $arg:ty) => {
        impl_filterable_value!(@one &$arg => $arg);
    };
    ($($arg:ty $(: $copy:ident)? $(=> $col:ty)?),* $(,)?) => {
        $(impl_filterable_value!(@one $arg $(: $copy)? $(=> $col)?);)*
    };
}

impl_filterable_value! {
    u8: Copy,
    u16: Copy,
    u32: Copy,
    u64: Copy,
    u128: Copy,
    u256: Copy,

    i8: Copy,
    i16: Copy,
    i32: Copy,
    i64: Copy,
    i128: Copy,
    i256: Copy,

    bool: Copy,

    String,
    &str => String,

    Identity: Copy,
    ConnectionId: Copy,
    Hash: Copy,

    // Some day we will likely also want to support `Vec<u8>` and `[u8]`,
    // as they have trivial portable equality and ordering,
    // but @RReverser's proposed filtering rules do not include them.
    // Vec<u8>,
    // &[u8] => Vec<u8>,
}

pub enum TermBound<T> {
    Single(ops::Bound<T>),
    Range(ops::Bound<T>, ops::Bound<T>),
}
impl<Bound: FilterableValue> TermBound<&Bound> {
    #[inline]
    /// If `self` is [`TermBound::Range`], returns the `rend_idx` value for `IndexScanRangeArgs`,
    /// i.e. the index in `buf` of the first byte in the end range
    pub fn serialize_into(&self, buf: &mut Vec<u8>) -> Option<usize> {
        let (start, end) = match self {
            TermBound::Single(elem) => (elem, None),
            TermBound::Range(start, end) => (start, Some(end)),
        };
        bsatn::to_writer(buf, start).unwrap();
        end.map(|end| {
            let rend_idx = buf.len();
            bsatn::to_writer(buf, end).unwrap();
            rend_idx
        })
    }
}
pub trait IndexScanRangeBoundsTerminator {
    type Arg;
    fn bounds(&self) -> TermBound<&Self::Arg>;
}

impl<Col, Arg: FilterableValue<Column = Col>> IndexScanRangeBoundsTerminator for Arg {
    type Arg = Arg;
    fn bounds(&self) -> TermBound<&Arg> {
        TermBound::Single(ops::Bound::Included(self))
    }
}

macro_rules! impl_terminator {
    ($($range:ty),* $(,)?) => {
        $(impl<T: FilterableValue> IndexScanRangeBoundsTerminator for $range {
            type Arg = T;
            fn bounds(&self) -> TermBound<&T> {
                TermBound::Range(
                    ops::RangeBounds::start_bound(self),
                    ops::RangeBounds::end_bound(self),
                )
            }
        })*
    };
}

impl_terminator!(
    ops::Range<T>,
    ops::RangeFrom<T>,
    ops::RangeInclusive<T>,
    ops::RangeTo<T>,
    ops::RangeToInclusive<T>,
    (ops::Bound<T>, ops::Bound<T>),
);
