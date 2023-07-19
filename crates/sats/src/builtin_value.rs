use crate::algebraic_value::AlgebraicValue;
use crate::builtin_type::BuiltinType;
use crate::{AlgebraicType, ArrayType};
use enum_as_inner::EnumAsInner;
use std::collections::BTreeMap;
use std::fmt;

/// Totally ordered [`f32`].
pub type F32 = decorum::Total<f32>;

/// Totally ordered [`f64`].
pub type F64 = decorum::Total<f64>;

/// A built-in value of a [`BuiltinType`].
#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum BuiltinValue {
    /// A [`bool`] value.
    Bool(bool),
    /// An [`i8`] value.
    I8(i8),
    /// A [`u8`] value.
    U8(u8),
    /// An [`i16`] value.
    I16(i16),
    /// A [`u16`] value.
    U16(u16),
    /// An [`i32`] value.
    I32(i32),
    /// A [`u32`] value.
    U32(u32),
    /// An [`i64`] value.
    I64(i64),
    /// A [`u64`] value.
    U64(u64),
    /// An [`i128`] value.
    I128(i128),
    /// A [`u128`] value.
    U128(u128),
    /// A totally ordered [`F32`] value.
    F32(F32),
    /// A totally ordered [`F64`] value.
    F64(F64),
    /// A UTF-8 string value.
    ///
    /// Uses Rust's standard representation of strings.
    String(String),
    /// A homogeneous array of `AlgebraicValue`s.
    ///
    /// The contained values are stored packed in a representation appropriate for their type.
    /// See [`ArrayValue`] for details on the representation.
    Array { val: ArrayValue },
    /// An ordered map value of `key: AlgebraicValue`s mapped to `value: AlgebraicValue`s.
    /// Each `key` must be of the same [`AlgebraicType`] as all the others
    /// and the same applies to each `value`.
    ///
    /// Maps are implemented internally as `BTreeMap<AlgebraicValue, AlgebraicValue>`.
    Map { val: MapValue },
}

/// A map value `AlgebraicValue` → `AlgebraicValue`.
pub type MapValue = BTreeMap<AlgebraicValue, AlgebraicValue>;

impl crate::Value for MapValue {
    type Type = crate::MapType;
}

impl BuiltinValue {
    /// Returns the byte string `v` as a [`BuiltinValue`].
    #[allow(non_snake_case)]
    pub const fn Bytes(v: Vec<u8>) -> Self {
        Self::Array { val: ArrayValue::U8(v) }
    }

    /// Returns `self` as a borrowed byte string, if applicable.
    pub fn as_bytes(&self) -> Option<&Vec<u8>> {
        match self {
            BuiltinValue::Array { val: ArrayValue::U8(v) } => Some(v),
            _ => None,
        }
    }

    /// Converts `self` into a byte string, if applicable.
    pub fn into_bytes(self) -> Result<Vec<u8>, Self> {
        match self {
            BuiltinValue::Array { val: ArrayValue::U8(v) } => Ok(v),
            _ => Err(self),
        }
    }
}

impl crate::Value for BuiltinValue {
    type Type = BuiltinType;
}

/// An array value in "monomorphized form".
///
/// Arrays are represented in this way monomorphized fashion for efficiency
/// rather than unnecessary indirections and tags of `Vec<AlgebraicValue>`.
/// We can do this as we know statically that the type of each element is the same
/// as arrays are homogenous dynamically sized product types.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ArrayValue {
    /// An array of [`SumValue`](crate::SumValue)s.
    Sum(Vec<crate::SumValue>),
    /// An array of [`ProductValue`](crate::ProductValue)s.
    Product(Vec<crate::ProductValue>),
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
            ArrayValue::Sum(v) => Self::first_type_of(v, AlgebraicValue::type_of_sum),
            ArrayValue::Product(v) => Self::first_type_of(v, AlgebraicValue::type_of_product),
            ArrayValue::Bool(_) => AlgebraicType::Bool,
            ArrayValue::I8(_) => AlgebraicType::I8,
            ArrayValue::U8(_) => AlgebraicType::U8,
            ArrayValue::I16(_) => AlgebraicType::I16,
            ArrayValue::U16(_) => AlgebraicType::U16,
            ArrayValue::I32(_) => AlgebraicType::I32,
            ArrayValue::U32(_) => AlgebraicType::U32,
            ArrayValue::I64(_) => AlgebraicType::I64,
            ArrayValue::U64(_) => AlgebraicType::U64,
            ArrayValue::I128(_) => AlgebraicType::I128,
            ArrayValue::U128(_) => AlgebraicType::U128,
            ArrayValue::F32(_) => AlgebraicType::F32,
            ArrayValue::F64(_) => AlgebraicType::F64,
            ArrayValue::String(_) => AlgebraicType::String,
            ArrayValue::Array(v) => Self::first_type_of(v, |a| AlgebraicType::Builtin(BuiltinType::Array(a.type_of()))),
            ArrayValue::Map(v) => Self::first_type_of(v, AlgebraicValue::type_of_map),
        });
        ArrayType { elem_ty }
    }

    /// Helper for `type_of` above.
    /// Infers the `AlgebraicType` from the first element by running `then` on it.
    fn first_type_of<T>(arr: &[T], then: impl FnOnce(&T) -> AlgebraicType) -> AlgebraicType {
        arr.first().map(then).unwrap_or_else(|| AlgebraicType::NEVER_TYPE)
    }

    /// Returns the length of the array.
    pub fn len(&self) -> usize {
        match self {
            ArrayValue::Sum(v) => v.len(),
            ArrayValue::Product(v) => v.len(),
            ArrayValue::Bool(v) => v.len(),
            ArrayValue::I8(v) => v.len(),
            ArrayValue::U8(v) => v.len(),
            ArrayValue::I16(v) => v.len(),
            ArrayValue::U16(v) => v.len(),
            ArrayValue::I32(v) => v.len(),
            ArrayValue::U32(v) => v.len(),
            ArrayValue::I64(v) => v.len(),
            ArrayValue::U64(v) => v.len(),
            ArrayValue::I128(v) => v.len(),
            ArrayValue::U128(v) => v.len(),
            ArrayValue::F32(v) => v.len(),
            ArrayValue::F64(v) => v.len(),
            ArrayValue::String(v) => v.len(),
            ArrayValue::Array(v) => v.len(),
            ArrayValue::Map(v) => v.len(),
        }
    }

    /// Returns whether the array is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a singleton array with `val` as its only element.
    ///
    /// Optionally allocates the backing `Vec<_>`s with `capacity`.
    fn from_one_with_capacity(val: AlgebraicValue, capacity: Option<usize>) -> Self {
        fn vec<T>(e: T, c: Option<usize>) -> Vec<T> {
            let mut vec = c.map_or(Vec::new(), Vec::with_capacity);
            vec.push(e);
            vec
        }

        match val {
            AlgebraicValue::Sum(x) => vec(x, capacity).into(),
            AlgebraicValue::Product(x) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::Bool(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::I8(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::U8(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::I16(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::U16(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::I32(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::U32(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::I64(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::U64(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::I128(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::U128(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::F32(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::F64(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::String(x)) => vec(x, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::Array { val }) => vec(val, capacity).into(),
            AlgebraicValue::Builtin(BuiltinValue::Map { val }) => vec(val, capacity).into(),
        }
    }

    /// Pushes the value `val` onto the array `self`
    /// or returns back `Err(val)` if there was a type mismatch
    /// between the base type of the array and `val`.
    ///
    /// Optionally allocates the backing `Vec<_>`s with `capacity`.
    pub fn push(&mut self, val: AlgebraicValue, capacity: Option<usize>) -> Result<(), AlgebraicValue> {
        match (self, val) {
            (ArrayValue::Sum(v), AlgebraicValue::Sum(val)) => v.push(val),
            (ArrayValue::Product(v), AlgebraicValue::Product(val)) => v.push(val),
            (ArrayValue::Bool(v), AlgebraicValue::Builtin(BuiltinValue::Bool(val))) => v.push(val),
            (ArrayValue::I8(v), AlgebraicValue::Builtin(BuiltinValue::I8(val))) => v.push(val),
            (ArrayValue::U8(v), AlgebraicValue::Builtin(BuiltinValue::U8(val))) => v.push(val),
            (ArrayValue::I16(v), AlgebraicValue::Builtin(BuiltinValue::I16(val))) => v.push(val),
            (ArrayValue::U16(v), AlgebraicValue::Builtin(BuiltinValue::U16(val))) => v.push(val),
            (ArrayValue::I32(v), AlgebraicValue::Builtin(BuiltinValue::I32(val))) => v.push(val),
            (ArrayValue::U32(v), AlgebraicValue::Builtin(BuiltinValue::U32(val))) => v.push(val),
            (ArrayValue::I64(v), AlgebraicValue::Builtin(BuiltinValue::I64(val))) => v.push(val),
            (ArrayValue::U64(v), AlgebraicValue::Builtin(BuiltinValue::U64(val))) => v.push(val),
            (ArrayValue::I128(v), AlgebraicValue::Builtin(BuiltinValue::I128(val))) => v.push(val),
            (ArrayValue::U128(v), AlgebraicValue::Builtin(BuiltinValue::U128(val))) => v.push(val),
            (ArrayValue::F32(v), AlgebraicValue::Builtin(BuiltinValue::F32(val))) => v.push(val),
            (ArrayValue::F64(v), AlgebraicValue::Builtin(BuiltinValue::F64(val))) => v.push(val),
            (ArrayValue::String(v), AlgebraicValue::Builtin(BuiltinValue::String(val))) => v.push(val),
            (ArrayValue::Array(v), AlgebraicValue::Builtin(BuiltinValue::Array { val })) => v.push(val),
            (ArrayValue::Map(v), AlgebraicValue::Builtin(BuiltinValue::Map { val })) => v.push(val),
            (me, val) if me.is_empty() => *me = Self::from_one_with_capacity(val, capacity),
            (_, val) => return Err(val),
        }
        Ok(())
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
        Self::from(Vec::<crate::SumValue>::default())
    }
}

macro_rules! impl_from_array {
    ($el:ty, $var:ident) => {
        impl From<Vec<$el>> for ArrayValue {
            fn from(v: Vec<$el>) -> Self {
                ArrayValue::$var(v)
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
    Sum(std::vec::IntoIter<crate::SumValue>),
    /// An iterator on a product value array.
    Product(std::vec::IntoIter<crate::ProductValue>),
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
            ArrayValueIntoIter::Sum(it) => it.next().map(AlgebraicValue::Sum),
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
            ArrayValueIntoIter::F32(it) => it.next().map(|f| f32::from(f).into()),
            ArrayValueIntoIter::F64(it) => it.next().map(|f| f64::from(f).into()),
            ArrayValueIntoIter::String(it) => it.next().map(Into::into),
            ArrayValueIntoIter::Array(it) => it.next().map(AlgebraicValue::ArrayOf),
            ArrayValueIntoIter::Map(it) => it.next().map(AlgebraicValue::map),
        }
    }
}

pub enum ArrayValueIterCloned<'a> {
    Sum(std::slice::Iter<'a, crate::SumValue>),
    Product(std::slice::Iter<'a, crate::ProductValue>),
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
            ArrayValueIterCloned::Sum(it) => it.next().cloned().map(AlgebraicValue::Sum),
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
            ArrayValueIterCloned::F32(it) => it.next().map(|f| f32::from(*f).into()),
            ArrayValueIterCloned::F64(it) => it.next().map(|f| f64::from(*f).into()),
            ArrayValueIterCloned::String(it) => it.next().cloned().map(Into::into),
            ArrayValueIterCloned::Array(it) => it.next().cloned().map(AlgebraicValue::ArrayOf),
            ArrayValueIterCloned::Map(it) => it.next().cloned().map(AlgebraicValue::map),
        }
    }
}
