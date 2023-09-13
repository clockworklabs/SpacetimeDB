use super::AlgebraicValue;
use crate::ser::{self, ForwardNamedToSeqProduct};
use crate::slim_slice::{try_into, LenTooLong};
use crate::{ArrayValue, MapValue, SatsSlice, SatsString, F32, F64};

/// An implementation of [`Serializer`](ser::Serializer)
/// where the output of serialization is an `AlgebraicValue`.
pub struct ValueSerializer;

macro_rules! method {
    ($name:ident -> $t:ty) => {
        fn $name(self, v: $t) -> Result<Self::Ok, Self::Error> {
            Ok(v.into())
        }
    };
}

impl ser::Serializer for ValueSerializer {
    type Ok = AlgebraicValue;
    type Error = LenTooLong;

    type SerializeArray = SerializeArrayValue;
    type SerializeMap = SerializeMapValue;
    type SerializeSeqProduct = SerializeProductValue;
    type SerializeNamedProduct = ForwardNamedToSeqProduct<SerializeProductValue>;

    method!(serialize_bool -> bool);
    method!(serialize_u8 -> u8);
    method!(serialize_u16 -> u16);
    method!(serialize_u32 -> u32);
    method!(serialize_u64 -> u64);
    method!(serialize_u128 -> u128);
    method!(serialize_i8 -> i8);
    method!(serialize_i16 -> i16);
    method!(serialize_i32 -> i32);
    method!(serialize_i64 -> i64);
    method!(serialize_i128 -> i128);
    method!(serialize_f32 -> f32);
    method!(serialize_f64 -> f64);

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        try_into(v).map(AlgebraicValue::String)
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        try_into(v).map(|v: SatsSlice<_>| AlgebraicValue::Bytes(v.into()))
    }

    fn serialize_array(self, len: usize) -> Result<Self::SerializeArray, Self::Error> {
        Ok(SerializeArrayValue {
            len: Some(len),
            array: Default::default(),
        })
    }

    fn serialize_map(self, len: usize) -> Result<Self::SerializeMap, Self::Error> {
        Ok(SerializeMapValue {
            entries: Vec::with_capacity(len),
        })
    }

    fn serialize_seq_product(self, len: usize) -> Result<Self::SerializeSeqProduct, Self::Error> {
        Ok(SerializeProductValue {
            elements: Vec::with_capacity(len),
        })
    }

    fn serialize_named_product(self, len: usize) -> Result<Self::SerializeNamedProduct, Self::Error> {
        ForwardNamedToSeqProduct::forward(self, len)
    }

    fn serialize_variant<T: ser::Serialize + ?Sized>(
        self,
        tag: u8,
        _name: Option<&str>,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        value.serialize(self).map(|v| AlgebraicValue::sum(tag, v))
    }
}

/// Continuation for serializing an array.
pub struct SerializeArrayValue {
    /// For efficiency, the first time `serialize_element` is done,
    /// this is used to allocate with capacity.
    len: Option<usize>,
    /// The array being built.
    array: ArrayValueBuilder,
}

impl ser::SerializeArray for SerializeArrayValue {
    type Ok = AlgebraicValue;
    type Error = <ValueSerializer as ser::Serializer>::Error;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        self.array
            .push(elem.serialize(ValueSerializer)?, self.len.take())
            .expect("heterogeneous array");
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        try_into(self.array).map(AlgebraicValue::Array)
    }
}

/// A builder for [`ArrayValue`]s
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
enum ArrayValueBuilder {
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
    String(Vec<SatsString>),
    /// An array of arrays.
    Array(Vec<ArrayValue>),
    /// An array of maps.
    Map(Vec<MapValue>),
}

impl ArrayValueBuilder {
    /// Returns the length of the array.
    fn len(&self) -> usize {
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
    fn is_empty(&self) -> bool {
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
            AlgebraicValue::Map(val) => vec(*val, capacity).into(),
            AlgebraicValue::Bool(x) => vec(x, capacity).into(),
            AlgebraicValue::I8(x) => vec(x, capacity).into(),
            AlgebraicValue::U8(x) => vec(x, capacity).into(),
            AlgebraicValue::I16(x) => vec(x, capacity).into(),
            AlgebraicValue::U16(x) => vec(x, capacity).into(),
            AlgebraicValue::I32(x) => vec(x, capacity).into(),
            AlgebraicValue::U32(x) => vec(x, capacity).into(),
            AlgebraicValue::I64(x) => vec(x, capacity).into(),
            AlgebraicValue::U64(x) => vec(x, capacity).into(),
            AlgebraicValue::I128(x) => vec(*x, capacity).into(),
            AlgebraicValue::U128(x) => vec(*x, capacity).into(),
            AlgebraicValue::F32(x) => vec(x, capacity).into(),
            AlgebraicValue::F64(x) => vec(x, capacity).into(),
            AlgebraicValue::String(x) => vec(x, capacity).into(),
            AlgebraicValue::Array(x) => vec(x, capacity).into(),
        }
    }

    /// Pushes the value `val` onto the array `self`
    /// or returns back `Err(val)` if there was a type mismatch
    /// between the base type of the array and `val`.
    ///
    /// Optionally allocates the backing `Vec<_>`s with `capacity`.
    fn push(&mut self, val: AlgebraicValue, capacity: Option<usize>) -> Result<(), AlgebraicValue> {
        match (self, val) {
            (Self::Sum(v), AlgebraicValue::Sum(val)) => v.push(val),
            (Self::Product(v), AlgebraicValue::Product(val)) => v.push(val),
            (Self::Map(v), AlgebraicValue::Map(val)) => v.push(*val),
            (Self::Bool(v), AlgebraicValue::Bool(val)) => v.push(val),
            (Self::I8(v), AlgebraicValue::I8(val)) => v.push(val),
            (Self::U8(v), AlgebraicValue::U8(val)) => v.push(val),
            (Self::I16(v), AlgebraicValue::I16(val)) => v.push(val),
            (Self::U16(v), AlgebraicValue::U16(val)) => v.push(val),
            (Self::I32(v), AlgebraicValue::I32(val)) => v.push(val),
            (Self::U32(v), AlgebraicValue::U32(val)) => v.push(val),
            (Self::I64(v), AlgebraicValue::I64(val)) => v.push(val),
            (Self::U64(v), AlgebraicValue::U64(val)) => v.push(val),
            (Self::I128(v), AlgebraicValue::I128(val)) => v.push(*val),
            (Self::U128(v), AlgebraicValue::U128(val)) => v.push(*val),
            (Self::F32(v), AlgebraicValue::F32(val)) => v.push(val),
            (Self::F64(v), AlgebraicValue::F64(val)) => v.push(val),
            (Self::String(v), AlgebraicValue::String(val)) => v.push(val),
            (Self::Array(v), AlgebraicValue::Array(val)) => v.push(val),
            (me, val) if me.is_empty() => *me = Self::from_one_with_capacity(val, capacity),
            (_, val) => return Err(val),
        }
        Ok(())
    }
}

impl TryFrom<ArrayValueBuilder> for ArrayValue {
    type Error = LenTooLong<ArrayValueBuilder>;

    fn try_from(value: ArrayValueBuilder) -> Result<Self, Self::Error> {
        use ArrayValueBuilder::*;
        match value {
            Sum(v) => v.try_into().map(Self::Sum).map_err(|e| e.map(Sum)),
            Product(v) => v.try_into().map(Self::Product).map_err(|e| e.map(Product)),
            Bool(v) => v.try_into().map(Self::Bool).map_err(|e| e.map(Bool)),
            I8(v) => v.try_into().map(Self::I8).map_err(|e| e.map(I8)),
            U8(v) => v.try_into().map(Self::U8).map_err(|e| e.map(U8)),
            I16(v) => v.try_into().map(Self::I16).map_err(|e| e.map(I16)),
            U16(v) => v.try_into().map(Self::U16).map_err(|e| e.map(U16)),
            I32(v) => v.try_into().map(Self::I32).map_err(|e| e.map(I32)),
            U32(v) => v.try_into().map(Self::U32).map_err(|e| e.map(U32)),
            I64(v) => v.try_into().map(Self::I64).map_err(|e| e.map(I64)),
            U64(v) => v.try_into().map(Self::U64).map_err(|e| e.map(U64)),
            I128(v) => v.try_into().map(Self::I128).map_err(|e| e.map(I128)),
            U128(v) => v.try_into().map(Self::U128).map_err(|e| e.map(U128)),
            F32(v) => v.try_into().map(Self::F32).map_err(|e| e.map(F32)),
            F64(v) => v.try_into().map(Self::F64).map_err(|e| e.map(F64)),
            String(v) => v.try_into().map(Self::String).map_err(|e| e.map(String)),
            Array(v) => v.try_into().map(Self::Array).map_err(|e| e.map(Array)),
            Map(v) => v.try_into().map(Self::Map).map_err(|e| e.map(Map)),
        }
    }
}

impl Default for ArrayValueBuilder {
    /// The default `ArrayValue` is an empty array of sum values.
    fn default() -> Self {
        Self::from(Vec::<crate::SumValue>::default())
    }
}

macro_rules! impl_from_array {
    ($el:ty, $var:ident) => {
        impl From<Vec<$el>> for ArrayValueBuilder {
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
impl_from_array!(SatsString, String);
impl_from_array!(ArrayValue, Array);
impl_from_array!(MapValue, Map);

/// Continuation for serializing a map value.
pub struct SerializeMapValue {
    /// The entry pairs to collect and convert into a map.
    entries: Vec<(AlgebraicValue, AlgebraicValue)>,
}

impl ser::SerializeMap for SerializeMapValue {
    type Ok = AlgebraicValue;
    type Error = <ValueSerializer as ser::Serializer>::Error;

    fn serialize_entry<K: ser::Serialize + ?Sized, V: ser::Serialize + ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error> {
        self.entries
            .push((key.serialize(ValueSerializer)?, value.serialize(ValueSerializer)?));
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::map(self.entries.into_iter().collect()))
    }
}

/// Continuation for serializing a map value.
pub struct SerializeProductValue {
    /// The elements serialized so far.
    elements: Vec<AlgebraicValue>,
}

impl ser::SerializeSeqProduct for SerializeProductValue {
    type Ok = AlgebraicValue;
    type Error = <ValueSerializer as ser::Serializer>::Error;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        self.elements.push(elem.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        try_into(self.elements).map(AlgebraicValue::product)
    }
}
