use spacetimedb_primitives::ColList;
use std::collections::BTreeMap;

use crate::{
    AlgebraicType, AlgebraicValue, ArrayValue, MapType, MapValue, ProductValue, SumValue, ValueWithType, F32, F64,
};

use super::{Serialize, SerializeArray, SerializeMap, SerializeNamedProduct, SerializeSeqProduct, Serializer};

/// Implements [`Serialize`] for a type in a simplified manner.
///
/// An example:
/// ```ignore
/// struct Foo<'a, T: Copy>(&'a T, u8);
/// impl_serialize!(
/// //     Type parameters  Optional where  Impl type
/// //            v               v             v
/// //   ----------------  --------------- ----------
///     ['a, T: Serialize] where [T: Copy] Foo<'a, T>,
/// //  The `serialize` implementation where `self` is serialized into `ser`
/// //  and the expression right of `=>` is the body of `serialize`.
///     (self, ser) => {
///         let mut prod = ser.serialize_seq_product(2)?;
///         prod.serialize_element(&self.0)?;
///         prod.serialize_element(&self.1)?;
///         prod.end()
///     }
/// );
/// ```
#[macro_export]
macro_rules! impl_serialize {
    ([$($generics:tt)*] $(where [$($wc:tt)*])? $typ:ty, ($self:ident, $ser:ident) => $body:expr) => {
        impl<$($generics)*> $crate::ser::Serialize for $typ $(where $($wc)*)? {
            fn serialize<S: $crate::ser::Serializer>($self: &Self, $ser: S) -> Result<S::Ok, S::Error> {
                $body
            }
        }
    };
}

macro_rules! impl_prim {
    ($(($prim:ty, $method:ident))*) => {
        $(impl_serialize!([] $prim, (self, ser) => ser.$method((*self).into()));)*
    };
}

impl_serialize!([] (), (self, ser) => ser.serialize_seq_product(0)?.end());

impl_prim! {
    (bool, serialize_bool) /*(u8, serialize_u8)*/ (u16, serialize_u16)
    (u32, serialize_u32) (u64, serialize_u64) (u128, serialize_u128) (i8, serialize_i8)
    (i16, serialize_i16) (i32, serialize_i32) (i64, serialize_i64) (i128, serialize_i128)
    (f32, serialize_f32) (f64, serialize_f64) (str, serialize_str)
}

impl Serialize for u8 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self)
    }

    fn __serialize_array<S: Serializer>(this: &[Self], serializer: S) -> Result<S::Ok, S::Error>
    where
        Self: Sized,
    {
        serializer.serialize_bytes(this)
    }
}

impl_serialize!([] F32, (self, ser) => f32::from(*self).serialize(ser));
impl_serialize!([] F64, (self, ser) => f64::from(*self).serialize(ser));
impl_serialize!([T: Serialize] Vec<T>, (self, ser)  => (**self).serialize(ser));
impl_serialize!([T: Serialize] [T], (self, ser) => T::__serialize_array(self, ser));
impl_serialize!([T: Serialize, const N: usize] [T; N], (self, ser) => T::__serialize_array(self, ser));
impl_serialize!([T: Serialize + ?Sized] Box<T>, (self, ser) => (**self).serialize(ser));
impl_serialize!([T: Serialize + ?Sized] &T, (self, ser) => (**self).serialize(ser));
impl_serialize!([] String, (self, ser) => ser.serialize_str(self));
impl_serialize!([T: Serialize] Option<T>, (self, ser) => match self {
    Some(v) => ser.serialize_variant(0, Some("some"), v),
    None => ser.serialize_variant(1, Some("none"), &()),
});
impl_serialize!([T: Serialize, E: Serialize] Result<T, E>, (self, ser) => match self {
    Ok(v) => ser.serialize_variant(0, Some("ok"), v),
    Err(e) => ser.serialize_variant(1, Some("err"), e),
});
impl_serialize!([K: Serialize, V: Serialize] BTreeMap<K, V>, (self, ser) => {
    let mut map = ser.serialize_map(self.len())?;
    for (k, v) in self {
        map.serialize_entry(k, v)?;
    }
    map.end()
});
impl_serialize!([] AlgebraicValue, (self, ser) => match self {
    Self::Sum(sum) => sum.serialize(ser),
    Self::Product(prod) => prod.serialize(ser),
    Self::Array(arr) => arr.serialize(ser),
    Self::Map(map) => map.serialize(ser),
    Self::Bool(v) => ser.serialize_bool(*v),
    Self::I8(v) => ser.serialize_i8(*v),
    Self::U8(v) => ser.serialize_u8(*v),
    Self::I16(v) => ser.serialize_i16(*v),
    Self::U16(v) => ser.serialize_u16(*v),
    Self::I32(v) => ser.serialize_i32(*v),
    Self::U32(v) => ser.serialize_u32(*v),
    Self::I64(v) => ser.serialize_i64(*v),
    Self::U64(v) => ser.serialize_u64(*v),
    Self::I128(v) => ser.serialize_i128(*v),
    Self::U128(v) => ser.serialize_u128(*v),
    Self::F32(v) => ser.serialize_f32((*v).into()),
    Self::F64(v) => ser.serialize_f64((*v).into()),
    // Self::Bytes(v) => ser.serialize_bytes(v),
    Self::String(v) => ser.serialize_str(v),
});
impl_serialize!([] ProductValue, (self, ser) => {
    let mut tup = ser.serialize_seq_product(self.elements.len())?;
    for elem in &*self.elements {
        tup.serialize_element(elem)?;
    }
    tup.end()
});
impl_serialize!([] SumValue, (self, ser) => ser.serialize_variant(self.tag, None, &*self.value));
impl_serialize!([] ArrayValue, (self, ser) => match self {
    Self::Sum(v) => v.serialize(ser),
    Self::Product(v) => v.serialize(ser),
    Self::Bool(v) => v.serialize(ser),
    Self::I8(v) => v.serialize(ser),
    Self::U8(v) => v.serialize(ser),
    Self::I16(v) => v.serialize(ser),
    Self::U16(v) => v.serialize(ser),
    Self::I32(v) => v.serialize(ser),
    Self::U32(v) => v.serialize(ser),
    Self::I64(v) => v.serialize(ser),
    Self::U64(v) => v.serialize(ser),
    Self::I128(v) => v.serialize(ser),
    Self::U128(v) => v.serialize(ser),
    Self::F32(v) => v.serialize(ser),
    Self::F64(v) => v.serialize(ser),
    Self::String(v) => v.serialize(ser),
    Self::Array(v) => v.serialize(ser),
    Self::Map(v) => v.serialize(ser),
});
impl_serialize!([] ValueWithType<'_, AlgebraicValue>, (self, ser) => {
    let mut ty = self.ty();
    loop { // We're doing this because of `Ref`s.
        break match (self.value(), ty) {
            (_, &AlgebraicType::Ref(r)) => {
                ty = &self.typespace()[r];
                continue;
            }
            (AlgebraicValue::Sum(val), AlgebraicType::Sum(ty)) => self.with(ty, val).serialize(ser),
            (AlgebraicValue::Product(val), AlgebraicType::Product(ty)) => self.with(ty, val).serialize(ser),
            (AlgebraicValue::Array(val), AlgebraicType::Builtin(crate::BuiltinType::Array(ty))) => self.with(ty, val).serialize(ser),
            (AlgebraicValue::Map(val), AlgebraicType::Builtin(crate::BuiltinType::Map(ty))) => self.with(&**ty, val).serialize(ser),
            (AlgebraicValue::Bool(v), &AlgebraicType::Bool) => ser.serialize_bool(*v),
            (AlgebraicValue::I8(v), &AlgebraicType::I8) => ser.serialize_i8(*v),
            (AlgebraicValue::U8(v), &AlgebraicType::U8) => ser.serialize_u8(*v),
            (AlgebraicValue::I16(v), &AlgebraicType::I16) => ser.serialize_i16(*v),
            (AlgebraicValue::U16(v), &AlgebraicType::U16) => ser.serialize_u16(*v),
            (AlgebraicValue::I32(v), &AlgebraicType::I32) => ser.serialize_i32(*v),
            (AlgebraicValue::U32(v), &AlgebraicType::U32) => ser.serialize_u32(*v),
            (AlgebraicValue::I64(v), &AlgebraicType::I64) => ser.serialize_i64(*v),
            (AlgebraicValue::U64(v), &AlgebraicType::U64) => ser.serialize_u64(*v),
            (AlgebraicValue::I128(v), &AlgebraicType::I128) => ser.serialize_i128(*v),
            (AlgebraicValue::U128(v), &AlgebraicType::U128) => ser.serialize_u128(*v),
            (AlgebraicValue::F32(v), &AlgebraicType::F32) => ser.serialize_f32((*v).into()),
            (AlgebraicValue::F64(v), &AlgebraicType::F64) => ser.serialize_f64((*v).into()),
            (AlgebraicValue::String(s), &AlgebraicType::String) => ser.serialize_str(s),
            (val, ty) => panic!("mismatched value and schema : {val:?} {ty:?}"),
        };
    }
});
impl_serialize!(
    [T: crate::Value] where [for<'a> ValueWithType<'a, T>: Serialize]
    ValueWithType<'_, Vec<T>>,
    (self, ser) => {
        let mut vec = ser.serialize_array(self.value().len())?;
        for val in self.iter() {
            vec.serialize_element(&val)?;
        }
        vec.end()
    }
);
impl_serialize!([] ValueWithType<'_, SumValue>, (self, ser) => {
    let sv = self.value();
    let (tag, val) = (sv.tag, &*sv.value);
    let var_ty = &self.ty().variants[tag as usize]; // Extract the variant type by tag.
    ser.serialize_variant(tag, var_ty.name(), &self.with(&var_ty.algebraic_type, val))
});
impl_serialize!([] ValueWithType<'_, ProductValue>, (self, ser) => {
    let val = &self.value().elements;
    assert_eq!(val.len(), self.ty().elements.len());
    let mut prod = ser.serialize_named_product(val.len())?;
    for (val, el_ty) in val.iter().zip(&*self.ty().elements) {
        prod.serialize_element(el_ty.name(), &self.with(&el_ty.algebraic_type, val))?
    }
    prod.end()
});
impl_serialize!([] ValueWithType<'_, ArrayValue>, (self, ser) => match (self.value(), &*self.ty().elem_ty) {
    (ArrayValue::Sum(v), AlgebraicType::Sum(ty)) => self.with(ty, v).serialize(ser),
    (ArrayValue::Product(v), AlgebraicType::Product(ty)) => self.with(ty, v).serialize(ser),
    (ArrayValue::Map(v), AlgebraicType::Builtin(crate::BuiltinType::Map(m))) => self.with(&**m, v).serialize(ser),
    (ArrayValue::Bool(v), &AlgebraicType::Bool) => v.serialize(ser),
    (ArrayValue::I8(v), &AlgebraicType::I8) => v.serialize(ser),
    (ArrayValue::U8(v), &AlgebraicType::U8) => v.serialize(ser),
    (ArrayValue::I16(v), &AlgebraicType::I16) => v.serialize(ser),
    (ArrayValue::U16(v), &AlgebraicType::U16) => v.serialize(ser),
    (ArrayValue::I32(v), &AlgebraicType::I32) => v.serialize(ser),
    (ArrayValue::U32(v), &AlgebraicType::U32) => v.serialize(ser),
    (ArrayValue::I64(v), &AlgebraicType::I64) => v.serialize(ser),
    (ArrayValue::U64(v), &AlgebraicType::U64) => v.serialize(ser),
    (ArrayValue::I128(v), &AlgebraicType::I128) => v.serialize(ser),
    (ArrayValue::U128(v), &AlgebraicType::U128) => v.serialize(ser),
    (ArrayValue::F32(v), &AlgebraicType::F32) => v.serialize(ser),
    (ArrayValue::F64(v), &AlgebraicType::F64) => v.serialize(ser),
    (ArrayValue::String(v), &AlgebraicType::String) => v.serialize(ser),
    (ArrayValue::Array(v), AlgebraicType::Builtin(crate::BuiltinType::Array(ty))) => self.with(ty, v).serialize(ser),
    (val, _) if val.is_empty() => ser.serialize_array(0)?.end(),
    (val, ty) => panic!("mismatched value and schema: {val:?} {ty:?}"),
});
impl_serialize!([] ValueWithType<'_, MapValue>, (self, ser) => {
    let val = self.value();
    let MapType { key_ty, ty } = self.ty();
    let mut map = ser.serialize_map(val.len())?;
    for (key, val) in val {
        map.serialize_entry(&self.with(key_ty, key), &self.with(ty, val))?;
    }
    map.end()
});

impl_serialize!([] spacetimedb_primitives::ColId, (self, ser) => ser.serialize_u32(self.0));
impl_serialize!([] spacetimedb_primitives::TableId, (self, ser) => ser.serialize_u32(self.0));
impl_serialize!([] spacetimedb_primitives::IndexId, (self, ser) => ser.serialize_u32(self.0));
impl_serialize!([] spacetimedb_primitives::SequenceId, (self, ser) => ser.serialize_u32(self.0));

impl_serialize!([] ColList, (self, ser) => {
    let mut arr = ser.serialize_array(self.len() as usize)?;
       for x in self.iter() {
           arr.serialize_element(&x)?;
       }
       arr.end()
});
