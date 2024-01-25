use crate::ser::{self, ForwardNamedToSeqProduct, Serialize};
use crate::{AlgebraicType, AlgebraicValue, ArrayValue, MapValue, F32, F64};
use core::convert::Infallible;
use core::ptr;
use std::alloc::{self, Layout};

/// Serialize `x` as an [`AlgebraicValue`].
pub fn value_serialize(x: &(impl Serialize + ?Sized)) -> AlgebraicValue {
    x.serialize(ValueSerializer).unwrap_or_else(|e| match e {})
}

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
    type Error = Infallible;

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
        Ok(AlgebraicValue::String(v.to_owned()))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::Bytes(v.to_owned()))
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

    unsafe fn serialize_bsatn(self, ty: &AlgebraicType, mut bsatn: &[u8]) -> Result<Self::Ok, Self::Error> {
        let res = AlgebraicValue::decode(ty, &mut bsatn);
        // SAFETY: Caller promised that `res.is_ok()`.
        Ok(unsafe { res.unwrap_unchecked() })
    }

    unsafe fn serialize_bsatn_in_chunks<'a, I: Iterator<Item = &'a [u8]>>(
        self,
        ty: &crate::AlgebraicType,
        total_bsatn_len: usize,
        chunks: I,
    ) -> Result<Self::Ok, Self::Error> {
        // SAFETY: Caller promised `total_bsatn_len == chunks.map(|c| c.len()).sum() <= isize::MAX`.
        let bsatn = unsafe { concat_byte_chunks(total_bsatn_len, chunks) };

        // SAFETY: Caller promised `AlgebraicValue::decode(ty, &mut bytes).is_ok()`.
        unsafe { self.serialize_bsatn(ty, &bsatn) }
    }

    unsafe fn serialize_str_in_chunks<'a, I: Iterator<Item = &'a [u8]>>(
        self,
        total_len: usize,
        string: I,
    ) -> Result<Self::Ok, Self::Error> {
        // SAFETY: Caller promised `total_len == string.map(|c| c.len()).sum() <= isize::MAx`.
        let bytes = unsafe { concat_byte_chunks(total_len, string) };

        // SAFETY: Caller promised `bytes` is UTF-8.
        let string = unsafe { String::from_utf8_unchecked(bytes) };
        Ok(string.into())
    }
}

/// Returns the concatenation of `chunks` that must be of `total_len` as a `Vec<u8>`.
///
/// # Safety
///
/// - `total_len == chunks.map(|c| c.len()).sum() <= isize::MAX`
unsafe fn concat_byte_chunks<'a>(total_len: usize, chunks: impl Iterator<Item = &'a [u8]>) -> Vec<u8> {
    if total_len == 0 {
        return Vec::new();
    }

    // Allocate space for `[u8; total_len]` on the heap.
    let layout = Layout::array::<u8>(total_len);
    // SAFETY: Caller promised that `total_len <= isize`.
    let layout = unsafe { layout.unwrap_unchecked() };
    // SAFETY: We checked above that `layout.size() != 0`.
    let ptr = unsafe { alloc::alloc(layout) };
    if ptr.is_null() {
        alloc::handle_alloc_error(layout);
    }

    // Copy over each `chunk`, moving `dst` by `chunk.len()` time.
    let mut dst = ptr;
    for chunk in chunks {
        let len = chunk.len();
        // SAFETY:
        // - `chunk` is valid for reads for `len` bytes.
        // - `dst` is valid for writes as we own it
        //    and as (1) caller promised that all `chunk`s will fit in `total_len`,
        //    this entails that `dst..dst + len` is always in bounds of the allocation.
        // - `chunk` and `dst` are trivially properly aligned (`align_of::<u8>() == 1`).
        // - The allocation `ptr` points to is new so derived pointers cannot overlap with `chunk`.
        unsafe {
            ptr::copy_nonoverlapping(chunk.as_ptr(), dst, len);
        }
        // SAFETY: Same as (1).
        dst = unsafe { dst.add(len) };
    }

    // Convert allocation to a `Vec<u8>`.
    // SAFETY:
    // - `ptr` was allocated using global allocator.
    // - `u8` and `ptr`'s allocation both have alignment of 1.
    // - `ptr`'s allocation is `total_len <= isize::MAX`.
    // - `total_len <= total_len` holds.
    // - `total_len` values were initialized at type `u8`
    //    as we know `total_len == chunks.map(|c| c.len()).sum()`.
    unsafe { Vec::from_raw_parts(ptr, total_len, total_len) }
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
            .push(value_serialize(elem), self.len.take())
            .expect("heterogeneous array");
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let array: ArrayValue = self.array.try_into().unwrap_or_else(|e| match e {});
        Ok(array.into())
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
    String(Vec<String>),
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
            AlgebraicValue::Map(x) => vec(x, capacity).into(),
            AlgebraicValue::Bool(x) => vec(x, capacity).into(),
            AlgebraicValue::I8(x) => vec(x, capacity).into(),
            AlgebraicValue::U8(x) => vec(x, capacity).into(),
            AlgebraicValue::I16(x) => vec(x, capacity).into(),
            AlgebraicValue::U16(x) => vec(x, capacity).into(),
            AlgebraicValue::I32(x) => vec(x, capacity).into(),
            AlgebraicValue::U32(x) => vec(x, capacity).into(),
            AlgebraicValue::I64(x) => vec(x, capacity).into(),
            AlgebraicValue::U64(x) => vec(x, capacity).into(),
            AlgebraicValue::I128(x) => vec(x, capacity).into(),
            AlgebraicValue::U128(x) => vec(x, capacity).into(),
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
            (Self::Map(v), AlgebraicValue::Map(val)) => v.push(val),
            (Self::Bool(v), AlgebraicValue::Bool(val)) => v.push(val),
            (Self::I8(v), AlgebraicValue::I8(val)) => v.push(val),
            (Self::U8(v), AlgebraicValue::U8(val)) => v.push(val),
            (Self::I16(v), AlgebraicValue::I16(val)) => v.push(val),
            (Self::U16(v), AlgebraicValue::U16(val)) => v.push(val),
            (Self::I32(v), AlgebraicValue::I32(val)) => v.push(val),
            (Self::U32(v), AlgebraicValue::U32(val)) => v.push(val),
            (Self::I64(v), AlgebraicValue::I64(val)) => v.push(val),
            (Self::U64(v), AlgebraicValue::U64(val)) => v.push(val),
            (Self::I128(v), AlgebraicValue::I128(val)) => v.push(val),
            (Self::U128(v), AlgebraicValue::U128(val)) => v.push(val),
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

impl From<ArrayValueBuilder> for ArrayValue {
    fn from(value: ArrayValueBuilder) -> Self {
        use ArrayValueBuilder::*;
        match value {
            Sum(v) => Self::Sum(v),
            Product(v) => Self::Product(v),
            Bool(v) => Self::Bool(v),
            I8(v) => Self::I8(v),
            U8(v) => Self::U8(v),
            I16(v) => Self::I16(v),
            U16(v) => Self::U16(v),
            I32(v) => Self::I32(v),
            U32(v) => Self::U32(v),
            I64(v) => Self::I64(v),
            U64(v) => Self::U64(v),
            I128(v) => Self::I128(v),
            U128(v) => Self::U128(v),
            F32(v) => Self::F32(v),
            F64(v) => Self::F64(v),
            String(v) => Self::String(v),
            Array(v) => Self::Array(v),
            Map(v) => Self::Map(v),
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
impl_from_array!(String, String);
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
        self.entries.push((value_serialize(key), value_serialize(value)));
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
        self.elements.push(value_serialize(elem));
        Ok(())
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::product(self.elements))
    }
}
