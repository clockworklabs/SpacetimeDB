use crate::{AlgebraicType, AlgebraicValue, ArrayType, MapValue, ProductValue, SumValue, F32, F64};
use std::fmt;

/// An array value in "monomorphized form".
///
/// Arrays are represented in this way monomorphized fashion for efficiency
/// rather than unnecessary indirections and tags of `AlgebraicValue`.
/// We can do this as we know statically that the type of each element is the same
/// as arrays are homogenous dynamically sized product types.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ArrayValue {
    /// An array of [`SumValue`](crate::SumValue)s.
    Sum(Vec<SumValue>),
    /// An array of [`ProductValue`](crate::ProductValue)s.
    Product(Vec<ProductValue>),
    /// An array of [`bool`]s.
    Bool(Vec<bool>),
    /// An array of [`i8`]s.
    I8(Vec<i8>),
    /// An array of [`u8`]s.
    U8(Vec<u8>),
    /// An array of [`i16`]s.
    I16(Vec<i16>),
    /// An array of [`u16`]s.
    U16(Vec<u16>),
    /// An array of [`i32`]s.
    I32(Vec<i32>),
    /// An array of [`u32`]s.
    U32(Vec<u32>),
    /// An array of [`i64`]s.
    I64(Vec<i64>),
    /// An array of [`u64`]s.
    U64(Vec<u64>),
    /// An array of [`i128`]s.
    I128(Vec<i128>),
    /// An array of [`u128`]s.
    U128(Vec<u128>),
    /// An array of totally ordered [`F32`]s.
    F32(Vec<F32>),
    /// An array of totally ordered [`F64`]s.
    F64(Vec<F64>),
    /// An array of UTF-8 strings.
    String(Vec<String>),
    /// An array of arrays.
    Array(Vec<ArrayValue>),
    /// An array of maps.
    Map(Vec<MapValue>),
}

impl crate::Value for ArrayValue {
    type Type = ArrayType;
}

impl ArrayValue {
    /// Determines (infers / synthesises) the type of the value.
    pub(crate) fn type_of(&self) -> ArrayType {
        let elem_ty = Box::new(match self {
            Self::Sum(v) => Self::first_type_of(v, AlgebraicValue::type_of_sum),
            Self::Product(v) => Self::first_type_of(v, AlgebraicValue::type_of_product),
            Self::Bool(_) => AlgebraicType::Bool,
            Self::I8(_) => AlgebraicType::I8,
            Self::U8(_) => AlgebraicType::U8,
            Self::I16(_) => AlgebraicType::I16,
            Self::U16(_) => AlgebraicType::U16,
            Self::I32(_) => AlgebraicType::I32,
            Self::U32(_) => AlgebraicType::U32,
            Self::I64(_) => AlgebraicType::I64,
            Self::U64(_) => AlgebraicType::U64,
            Self::I128(_) => AlgebraicType::I128,
            Self::U128(_) => AlgebraicType::U128,
            Self::F32(_) => AlgebraicType::F32,
            Self::F64(_) => AlgebraicType::F64,
            Self::String(_) => AlgebraicType::String,
            Self::Array(v) => Self::first_type_of(v, |a| a.type_of().into()),
            Self::Map(v) => Self::first_type_of(v, AlgebraicValue::type_of_map),
        });
        ArrayType { elem_ty }
    }

    /// Helper for `type_of` above.
    /// Infers the `AlgebraicType` from the first element by running `then` on it.
    ///
    /// The result of `first_type_of(&[])` is an empty sum type ("never"),
    /// that is, a type that has no values.
    /// This leads to e.g., an empty array of products having the type "never".
    /// This is the most conservative choice
    /// and has the consequence that no values can be added to such an array.
    fn first_type_of<T>(arr: &[T], then: impl FnOnce(&T) -> AlgebraicType) -> AlgebraicType {
        arr.first().map(then).unwrap_or_else(AlgebraicType::never)
    }

    /// Returns the length of the array.
    pub fn len(&self) -> usize {
        match self {
            Self::Sum(v) => v.len(),
            Self::Product(v) => v.len(),
            Self::Bool(v) => v.len(),
            Self::I8(v) => v.len(),
            Self::U8(v) => v.len(),
            Self::I16(v) => v.len(),
            Self::U16(v) => v.len(),
            Self::I32(v) => v.len(),
            Self::U32(v) => v.len(),
            Self::I64(v) => v.len(),
            Self::U64(v) => v.len(),
            Self::I128(v) => v.len(),
            Self::U128(v) => v.len(),
            Self::F32(v) => v.len(),
            Self::F64(v) => v.len(),
            Self::String(v) => v.len(),
            Self::Array(v) => v.len(),
            Self::Map(v) => v.len(),
        }
    }

    /// Returns whether the array is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a cloning iterator on the elements of `self` as `AlgebraicValue`s.
    pub fn iter_cloned(&self) -> ArrayValueIterCloned {
        match self {
            ArrayValue::Sum(v) => ArrayValueIterCloned::Sum(v.iter()),
            ArrayValue::Product(v) => ArrayValueIterCloned::Product(v.iter()),
            ArrayValue::Bool(v) => ArrayValueIterCloned::Bool(v.iter()),
            ArrayValue::I8(v) => ArrayValueIterCloned::I8(v.iter()),
            ArrayValue::U8(v) => ArrayValueIterCloned::U8(v.iter()),
            ArrayValue::I16(v) => ArrayValueIterCloned::I16(v.iter()),
            ArrayValue::U16(v) => ArrayValueIterCloned::U16(v.iter()),
            ArrayValue::I32(v) => ArrayValueIterCloned::I32(v.iter()),
            ArrayValue::U32(v) => ArrayValueIterCloned::U32(v.iter()),
            ArrayValue::I64(v) => ArrayValueIterCloned::I64(v.iter()),
            ArrayValue::U64(v) => ArrayValueIterCloned::U64(v.iter()),
            ArrayValue::I128(v) => ArrayValueIterCloned::I128(v.iter()),
            ArrayValue::U128(v) => ArrayValueIterCloned::U128(v.iter()),
            ArrayValue::F32(v) => ArrayValueIterCloned::F32(v.iter()),
            ArrayValue::F64(v) => ArrayValueIterCloned::F64(v.iter()),
            ArrayValue::String(v) => ArrayValueIterCloned::String(v.iter()),
            ArrayValue::Array(v) => ArrayValueIterCloned::Array(v.iter()),
            ArrayValue::Map(v) => ArrayValueIterCloned::Map(v.iter()),
        }
    }
}

impl Default for ArrayValue {
    /// The default `ArrayValue` is an empty array of sum values.
    fn default() -> Self {
        Self::from(<[crate::SumValue; 0]>::default())
    }
}

macro_rules! impl_from_array {
    ($el:ty, $var:ident) => {
        impl<const N: usize> From<[$el; N]> for ArrayValue {
            fn from(v: [$el; N]) -> Self {
                let vec: Vec<_> = v.into();
                vec.into()
            }
        }

        // Exists for convenience.
        impl From<Vec<$el>> for ArrayValue {
            fn from(v: Vec<$el>) -> Self {
                Self::$var(v)
            }
        }
    };
}

impl_from_array!(crate::SumValue, Sum);
impl_from_array!(crate::ProductValue, Product);
impl_from_array!(bool, Bool);
impl_from_array!(i8, I8);
impl_from_array!(u8, U8);
impl_from_array!(i16, I16);
impl_from_array!(u16, U16);
impl_from_array!(i32, I32);
impl_from_array!(u32, U32);
impl_from_array!(i64, I64);
impl_from_array!(u64, U64);
impl_from_array!(i128, I128);
impl_from_array!(u128, U128);
impl_from_array!(F32, F32);
impl_from_array!(F64, F64);
impl_from_array!(String, String);
impl_from_array!(ArrayValue, Array);
impl_from_array!(MapValue, Map);

impl ArrayValue {
    /// Returns `self` as `&dyn Debug`.
    fn as_dyn_debug(&self) -> &dyn fmt::Debug {
        match self {
            Self::Sum(v) => v,
            Self::Product(v) => v,
            Self::Bool(v) => v,
            Self::I8(v) => v,
            Self::U8(v) => v,
            Self::I16(v) => v,
            Self::U16(v) => v,
            Self::I32(v) => v,
            Self::U32(v) => v,
            Self::I64(v) => v,
            Self::U64(v) => v,
            Self::I128(v) => v,
            Self::U128(v) => v,
            Self::F32(v) => v,
            Self::F64(v) => v,
            Self::String(v) => v,
            Self::Array(v) => v,
            Self::Map(v) => v,
        }
    }
}
impl fmt::Debug for ArrayValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_dyn_debug().fmt(f)
    }
}

impl IntoIterator for ArrayValue {
    type Item = AlgebraicValue;

    type IntoIter = ArrayValueIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            ArrayValue::Sum(v) => ArrayValueIntoIter::Sum(v.into_iter()),
            ArrayValue::Product(v) => ArrayValueIntoIter::Product(v.into_iter()),
            ArrayValue::Bool(v) => ArrayValueIntoIter::Bool(v.into_iter()),
            ArrayValue::I8(v) => ArrayValueIntoIter::I8(v.into_iter()),
            ArrayValue::U8(v) => ArrayValueIntoIter::U8(v.into_iter()),
            ArrayValue::I16(v) => ArrayValueIntoIter::I16(v.into_iter()),
            ArrayValue::U16(v) => ArrayValueIntoIter::U16(v.into_iter()),
            ArrayValue::I32(v) => ArrayValueIntoIter::I32(v.into_iter()),
            ArrayValue::U32(v) => ArrayValueIntoIter::U32(v.into_iter()),
            ArrayValue::I64(v) => ArrayValueIntoIter::I64(v.into_iter()),
            ArrayValue::U64(v) => ArrayValueIntoIter::U64(v.into_iter()),
            ArrayValue::I128(v) => ArrayValueIntoIter::I128(v.into_iter()),
            ArrayValue::U128(v) => ArrayValueIntoIter::U128(v.into_iter()),
            ArrayValue::F32(v) => ArrayValueIntoIter::F32(v.into_iter()),
            ArrayValue::F64(v) => ArrayValueIntoIter::F64(v.into_iter()),
            ArrayValue::String(v) => ArrayValueIntoIter::String(v.into_iter()),
            ArrayValue::Array(v) => ArrayValueIntoIter::Array(v.into_iter()),
            ArrayValue::Map(v) => ArrayValueIntoIter::Map(v.into_iter()),
        }
    }
}

/// A by-value iterator on the elements of an `ArrayValue` as `AlgebraicValue`s.
pub enum ArrayValueIntoIter {
    /// An iterator on a sum value array.
    Sum(std::vec::IntoIter<SumValue>),
    /// An iterator on a product value array.
    Product(std::vec::IntoIter<ProductValue>),
    /// An iterator on a [`bool`] array.
    Bool(std::vec::IntoIter<bool>),
    /// An iterator on an [`i8`] array.
    I8(std::vec::IntoIter<i8>),
    /// An iterator on a [`u8`] array.
    U8(std::vec::IntoIter<u8>),
    /// An iterator on an [`i16`] array.
    I16(std::vec::IntoIter<i16>),
    /// An iterator on a [`u16`] array.
    U16(std::vec::IntoIter<u16>),
    /// An iterator on an [`i32`] array.
    I32(std::vec::IntoIter<i32>),
    /// An iterator on a [`u32`] array.
    U32(std::vec::IntoIter<u32>),
    /// An iterator on an [`i64`] array.
    I64(std::vec::IntoIter<i64>),
    /// An iterator on a [`u64`] array.
    U64(std::vec::IntoIter<u64>),
    /// An iterator on an [`i128`] array.
    I128(std::vec::IntoIter<i128>),
    /// An iterator on a [`u128`] array.
    U128(std::vec::IntoIter<u128>),
    /// An iterator on a [`F32`] array.
    F32(std::vec::IntoIter<F32>),
    /// An iterator on a [`F64`] array.
    F64(std::vec::IntoIter<F64>),
    /// An iterator on an array of UTF-8 strings.
    String(std::vec::IntoIter<String>),
    /// An iterator on an array of arrays.
    Array(std::vec::IntoIter<ArrayValue>),
    /// An iterator on an array of maps.
    Map(std::vec::IntoIter<MapValue>),
}

impl Iterator for ArrayValueIntoIter {
    type Item = AlgebraicValue;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ArrayValueIntoIter::Sum(it) => it.next().map(Into::into),
            ArrayValueIntoIter::Product(it) => it.next().map(Into::into),
            ArrayValueIntoIter::Bool(it) => it.next().map(Into::into),
            ArrayValueIntoIter::I8(it) => it.next().map(Into::into),
            ArrayValueIntoIter::U8(it) => it.next().map(Into::into),
            ArrayValueIntoIter::I16(it) => it.next().map(Into::into),
            ArrayValueIntoIter::U16(it) => it.next().map(Into::into),
            ArrayValueIntoIter::I32(it) => it.next().map(Into::into),
            ArrayValueIntoIter::U32(it) => it.next().map(Into::into),
            ArrayValueIntoIter::I64(it) => it.next().map(Into::into),
            ArrayValueIntoIter::U64(it) => it.next().map(Into::into),
            ArrayValueIntoIter::I128(it) => it.next().map(Into::into),
            ArrayValueIntoIter::U128(it) => it.next().map(Into::into),
            ArrayValueIntoIter::F32(it) => it.next().map(Into::into),
            ArrayValueIntoIter::F64(it) => it.next().map(Into::into),
            ArrayValueIntoIter::String(it) => it.next().map(Into::into),
            ArrayValueIntoIter::Array(it) => it.next().map(Into::into),
            ArrayValueIntoIter::Map(it) => it.next().map(Into::into),
        }
    }
}

pub enum ArrayValueIterCloned<'a> {
    Sum(std::slice::Iter<'a, SumValue>),
    Product(std::slice::Iter<'a, ProductValue>),
    Bool(std::slice::Iter<'a, bool>),
    I8(std::slice::Iter<'a, i8>),
    U8(std::slice::Iter<'a, u8>),
    I16(std::slice::Iter<'a, i16>),
    U16(std::slice::Iter<'a, u16>),
    I32(std::slice::Iter<'a, i32>),
    U32(std::slice::Iter<'a, u32>),
    I64(std::slice::Iter<'a, i64>),
    U64(std::slice::Iter<'a, u64>),
    I128(std::slice::Iter<'a, i128>),
    U128(std::slice::Iter<'a, u128>),
    F32(std::slice::Iter<'a, F32>),
    F64(std::slice::Iter<'a, F64>),
    String(std::slice::Iter<'a, String>),
    Array(std::slice::Iter<'a, ArrayValue>),
    Map(std::slice::Iter<'a, MapValue>),
}

impl Iterator for ArrayValueIterCloned<'_> {
    type Item = AlgebraicValue;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ArrayValueIterCloned::Sum(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::Product(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::Bool(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::I8(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::U8(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::I16(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::U16(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::I32(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::U32(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::I64(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::U64(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::I128(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::U128(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::F32(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::F64(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::String(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::Array(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::Map(it) => it.next().cloned().map(Into::into),
        }
    }
}
