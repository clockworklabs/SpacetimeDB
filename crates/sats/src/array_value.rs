use crate::{i256, u256};
use crate::{AlgebraicType, AlgebraicValue, ArrayType, ProductValue, SumValue, F32, F64};
use core::fmt;

/// An array value in "monomorphized form".
///
/// Arrays are represented in this way monomorphized fashion for efficiency
/// rather than unnecessary indirections and tags of `AlgebraicValue`.
/// We can do this as we know statically that the type of each element is the same
/// as arrays are homogenous dynamically sized product types.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum ArrayValue {
    /// An array of [`SumValue`]s.
    Sum(Box<[SumValue]>),
    /// An array of [`ProductValue`]s.
    Product(Box<[ProductValue]>),
    /// An array of [`bool`]s.
    Bool(Box<[bool]>),
    /// An array of [`i8`]s.
    I8(Box<[i8]>),
    /// An array of [`u8`]s.
    U8(Box<[u8]>),
    /// An array of [`i16`]s.
    I16(Box<[i16]>),
    /// An array of [`u16`]s.
    U16(Box<[u16]>),
    /// An array of [`i32`]s.
    I32(Box<[i32]>),
    /// An array of [`u32`]s.
    U32(Box<[u32]>),
    /// An array of [`i64`]s.
    I64(Box<[i64]>),
    /// An array of [`u64`]s.
    U64(Box<[u64]>),
    /// An array of [`i128`]s.
    I128(Box<[i128]>),
    /// An array of [`u128`]s.
    U128(Box<[u128]>),
    /// An array of [`i256`]s.
    I256(Box<[i256]>),
    /// An array of [`u256`]s.
    U256(Box<[u256]>),
    /// An array of totally ordered [`F32`]s.
    F32(Box<[F32]>),
    /// An array of totally ordered [`F64`]s.
    F64(Box<[F64]>),
    /// An array of UTF-8 strings.
    String(Box<[Box<str>]>),
    /// An array of arrays.
    Array(Box<[ArrayValue]>),
}

impl crate::Value for ArrayValue {
    type Type = ArrayType;
}

impl ArrayValue {
    /// Determines (infers / synthesises) the type of the value.
    pub(crate) fn type_of(&self) -> Option<ArrayType> {
        let elem_ty = Box::new(match self {
            Self::Sum(_) => None,
            Self::Product(v) => AlgebraicValue::type_of_product(v.first()?),
            Self::Bool(_) => Some(AlgebraicType::Bool),
            Self::I8(_) => Some(AlgebraicType::I8),
            Self::U8(_) => Some(AlgebraicType::U8),
            Self::I16(_) => Some(AlgebraicType::I16),
            Self::U16(_) => Some(AlgebraicType::U16),
            Self::I32(_) => Some(AlgebraicType::I32),
            Self::U32(_) => Some(AlgebraicType::U32),
            Self::I64(_) => Some(AlgebraicType::I64),
            Self::U64(_) => Some(AlgebraicType::U64),
            Self::I128(_) => Some(AlgebraicType::I128),
            Self::U128(_) => Some(AlgebraicType::U128),
            Self::I256(_) => Some(AlgebraicType::I256),
            Self::U256(_) => Some(AlgebraicType::U256),
            Self::F32(_) => Some(AlgebraicType::F32),
            Self::F64(_) => Some(AlgebraicType::F64),
            Self::String(_) => Some(AlgebraicType::String),
            Self::Array(v) => Some(v.first()?.type_of()?.into()),
        }?);
        Some(ArrayType { elem_ty })
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
            Self::I256(v) => v.len(),
            Self::U256(v) => v.len(),
            Self::F32(v) => v.len(),
            Self::F64(v) => v.len(),
            Self::String(v) => v.len(),
            Self::Array(v) => v.len(),
        }
    }

    /// Returns whether the array is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a cloning iterator on the elements of `self` as `AlgebraicValue`s.
    pub fn iter_cloned(&self) -> ArrayValueIterCloned<'_> {
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
            ArrayValue::I256(v) => ArrayValueIterCloned::I256(v.iter()),
            ArrayValue::U256(v) => ArrayValueIterCloned::U256(v.iter()),
            ArrayValue::F32(v) => ArrayValueIterCloned::F32(v.iter()),
            ArrayValue::F64(v) => ArrayValueIterCloned::F64(v.iter()),
            ArrayValue::String(v) => ArrayValueIterCloned::String(v.iter()),
            ArrayValue::Array(v) => ArrayValueIterCloned::Array(v.iter()),
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
                let vec: Box<[_]> = v.into();
                vec.into()
            }
        }

        // Exists for convenience.
        impl From<Box<[$el]>> for ArrayValue {
            fn from(v: Box<[$el]>) -> Self {
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
impl_from_array!(i256, I256);
impl_from_array!(u256, U256);
impl_from_array!(F32, F32);
impl_from_array!(F64, F64);
impl_from_array!(Box<str>, String);
impl_from_array!(ArrayValue, Array);

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
            Self::I256(v) => v,
            Self::U256(v) => v,
            Self::F32(v) => v,
            Self::F64(v) => v,
            Self::String(v) => v,
            Self::Array(v) => v,
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
            ArrayValue::Sum(v) => ArrayValueIntoIter::Sum(Vec::from(v).into_iter()),
            ArrayValue::Product(v) => ArrayValueIntoIter::Product(Vec::from(v).into_iter()),
            ArrayValue::Bool(v) => ArrayValueIntoIter::Bool(Vec::from(v).into_iter()),
            ArrayValue::I8(v) => ArrayValueIntoIter::I8(Vec::from(v).into_iter()),
            ArrayValue::U8(v) => ArrayValueIntoIter::U8(Vec::from(v).into_iter()),
            ArrayValue::I16(v) => ArrayValueIntoIter::I16(Vec::from(v).into_iter()),
            ArrayValue::U16(v) => ArrayValueIntoIter::U16(Vec::from(v).into_iter()),
            ArrayValue::I32(v) => ArrayValueIntoIter::I32(Vec::from(v).into_iter()),
            ArrayValue::U32(v) => ArrayValueIntoIter::U32(Vec::from(v).into_iter()),
            ArrayValue::I64(v) => ArrayValueIntoIter::I64(Vec::from(v).into_iter()),
            ArrayValue::U64(v) => ArrayValueIntoIter::U64(Vec::from(v).into_iter()),
            ArrayValue::I128(v) => ArrayValueIntoIter::I128(Vec::from(v).into_iter()),
            ArrayValue::U128(v) => ArrayValueIntoIter::U128(Vec::from(v).into_iter()),
            ArrayValue::I256(v) => ArrayValueIntoIter::I256(Vec::from(v).into_iter()),
            ArrayValue::U256(v) => ArrayValueIntoIter::U256(Vec::from(v).into_iter()),
            ArrayValue::F32(v) => ArrayValueIntoIter::F32(Vec::from(v).into_iter()),
            ArrayValue::F64(v) => ArrayValueIntoIter::F64(Vec::from(v).into_iter()),
            ArrayValue::String(v) => ArrayValueIntoIter::String(Vec::from(v).into_iter()),
            ArrayValue::Array(v) => ArrayValueIntoIter::Array(Vec::from(v).into_iter()),
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
    /// An iterator on an [`i256`] array.
    I256(std::vec::IntoIter<i256>),
    /// An iterator on a [`u256`] array.
    U256(std::vec::IntoIter<u256>),
    /// An iterator on a [`F32`] array.
    F32(std::vec::IntoIter<F32>),
    /// An iterator on a [`F64`] array.
    F64(std::vec::IntoIter<F64>),
    /// An iterator on an array of UTF-8 strings.
    String(std::vec::IntoIter<Box<str>>),
    /// An iterator on an array of arrays.
    Array(std::vec::IntoIter<ArrayValue>),
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
            ArrayValueIntoIter::I256(it) => it.next().map(Into::into),
            ArrayValueIntoIter::U256(it) => it.next().map(Into::into),
            ArrayValueIntoIter::F32(it) => it.next().map(Into::into),
            ArrayValueIntoIter::F64(it) => it.next().map(Into::into),
            ArrayValueIntoIter::String(it) => it.next().map(Into::into),
            ArrayValueIntoIter::Array(it) => it.next().map(Into::into),
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
    I256(std::slice::Iter<'a, i256>),
    U256(std::slice::Iter<'a, u256>),
    F32(std::slice::Iter<'a, F32>),
    F64(std::slice::Iter<'a, F64>),
    String(std::slice::Iter<'a, Box<str>>),
    Array(std::slice::Iter<'a, ArrayValue>),
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
            ArrayValueIterCloned::I256(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::U256(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::F32(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::F64(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::String(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::Array(it) => it.next().cloned().map(Into::into),
        }
    }
}
