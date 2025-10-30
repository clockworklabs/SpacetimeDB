use super::{Serialize, SerializeArray, SerializeSeqProduct, Serializer};
use crate::{i256, u256};
use crate::{AlgebraicType, AlgebraicValue, ArrayValue, ProductValue, SumValue, ValueWithType, F32, F64};
use core::ops::Bound;
use smallvec::SmallVec;
use spacetimedb_primitives::{ColList, ColSet};
use std::rc::Rc;
use std::sync::Arc;

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

// All the tuple types:
#[macro_export]
macro_rules! count {
    () => (0usize);
    ( $x:tt $($xs:tt)* ) => (1usize + $crate::count!($($xs)*));
}
macro_rules! impl_serialize_tuple {
    ($($ty_name:ident),*) => {
        impl_serialize!([$($ty_name: Serialize),*] ($($ty_name,)*), (self, ser) => {
            let mut _tup = ser.serialize_seq_product(count!($($ty_name)*))?;
            #[allow(non_snake_case)]
            let ($($ty_name,)*) = self;
            $(_tup.serialize_element($ty_name)?;)*
            _tup.end()
        });
    };
}
impl_serialize_tuple!();
impl_serialize_tuple!(T0);
impl_serialize_tuple!(T0, T1);
impl_serialize_tuple!(T0, T1, T2);
impl_serialize_tuple!(T0, T1, T2, T3);
impl_serialize_tuple!(T0, T1, T2, T3, T4);
impl_serialize_tuple!(T0, T1, T2, T3, T4, T5);
impl_serialize_tuple!(T0, T1, T2, T3, T4, T5, T6);
impl_serialize_tuple!(T0, T1, T2, T3, T4, T5, T6, T7);
impl_serialize_tuple!(T0, T1, T2, T3, T4, T5, T6, T7, T8);
impl_serialize_tuple!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_serialize_tuple!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_serialize_tuple!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);

// `u8` is implemented below as we wish to provide different `__serialize_array` impl (see below).
impl_prim! {
    (bool, serialize_bool)
                       (u16, serialize_u16) (u32, serialize_u32) (u64, serialize_u64) (u128, serialize_u128) (u256, serialize_u256)
    (i8, serialize_i8) (i16, serialize_i16) (i32, serialize_i32) (i64, serialize_i64) (i128, serialize_i128) (i256, serialize_i256)
    (f32, serialize_f32) (f64, serialize_f64) (str, serialize_str)
}

// TODO(Centril): this special case doesn't seem well motivated.
// Consider generalizing this to apply to all primitive types
// so that we can move this into `impl_prim!`.
// This will make BSATN-serializing `[u32]` faster for example.
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
impl_serialize!([T: Serialize, const N: usize] SmallVec<[T; N]>, (self, ser)  => (**self).serialize(ser));
impl_serialize!([T: Serialize] [T], (self, ser) => T::__serialize_array(self, ser));
impl_serialize!([T: Serialize, const N: usize] [T; N], (self, ser) => T::__serialize_array(self, ser));
impl_serialize!([T: Serialize + ?Sized] Box<T>, (self, ser) => (**self).serialize(ser));
impl_serialize!([T: Serialize + ?Sized] Rc<T>, (self, ser) => (**self).serialize(ser));
impl_serialize!([T: Serialize + ?Sized] Arc<T>, (self, ser) => (**self).serialize(ser));
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
impl_serialize!([T: Serialize] Bound<T>, (self, ser) => match self {
    Bound::Included(x) => ser.serialize_variant(0, Some("included"), x),
    Bound::Excluded(x) => ser.serialize_variant(1, Some("excluded"), x),
    Bound::Unbounded => ser.serialize_variant(2, Some("unbounded"), &()),
});
impl_serialize!([] AlgebraicValue, (self, ser) => match self {
    Self::Sum(sum) => sum.serialize(ser),
    Self::Product(prod) => prod.serialize(ser),
    Self::Array(arr) => arr.serialize(ser),
    Self::Bool(v) => ser.serialize_bool(*v),
    Self::I8(v) => ser.serialize_i8(*v),
    Self::U8(v) => ser.serialize_u8(*v),
    Self::I16(v) => ser.serialize_i16(*v),
    Self::U16(v) => ser.serialize_u16(*v),
    Self::I32(v) => ser.serialize_i32(*v),
    Self::U32(v) => ser.serialize_u32(*v),
    Self::I64(v) => ser.serialize_i64(*v),
    Self::U64(v) => ser.serialize_u64(*v),
    Self::I128(v) => ser.serialize_i128(v.0),
    Self::U128(v) => ser.serialize_u128(v.0),
    Self::I256(v) => ser.serialize_i256(**v),
    Self::U256(v) => ser.serialize_u256(**v),
    Self::F32(v) => ser.serialize_f32((*v).into()),
    Self::F64(v) => ser.serialize_f64((*v).into()),
    // Self::Bytes(v) => ser.serialize_bytes(v),
    Self::String(v) => ser.serialize_str(v),
    Self::Min | Self::Max => panic!("not defined for Min/Max"),
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
    Self::I256(v) => v.serialize(ser),
    Self::U256(v) => v.serialize(ser),
    Self::F32(v) => v.serialize(ser),
    Self::F64(v) => v.serialize(ser),
    Self::String(v) => v.serialize(ser),
    Self::Array(v) => v.serialize(ser),
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
            (AlgebraicValue::Array(val), AlgebraicType::Array(ty)) => self.with(ty, val).serialize(ser),
            (AlgebraicValue::Bool(v), AlgebraicType::Bool) => ser.serialize_bool(*v),
            (AlgebraicValue::I8(v), AlgebraicType::I8) => ser.serialize_i8(*v),
            (AlgebraicValue::U8(v), AlgebraicType::U8) => ser.serialize_u8(*v),
            (AlgebraicValue::I16(v), AlgebraicType::I16) => ser.serialize_i16(*v),
            (AlgebraicValue::U16(v), AlgebraicType::U16) => ser.serialize_u16(*v),
            (AlgebraicValue::I32(v), AlgebraicType::I32) => ser.serialize_i32(*v),
            (AlgebraicValue::U32(v), AlgebraicType::U32) => ser.serialize_u32(*v),
            (AlgebraicValue::I64(v), AlgebraicType::I64) => ser.serialize_i64(*v),
            (AlgebraicValue::U64(v), AlgebraicType::U64) => ser.serialize_u64(*v),
            (AlgebraicValue::I128(v), AlgebraicType::I128) => ser.serialize_i128(v.0),
            (AlgebraicValue::U128(v), AlgebraicType::U128) => ser.serialize_u128(v.0),
            (AlgebraicValue::I256(v), AlgebraicType::I256) => ser.serialize_i256(**v),
            (AlgebraicValue::U256(v), AlgebraicType::U256) => ser.serialize_u256(**v),
            (AlgebraicValue::F32(v), AlgebraicType::F32) => ser.serialize_f32((*v).into()),
            (AlgebraicValue::F64(v), AlgebraicType::F64) => ser.serialize_f64((*v).into()),
            (AlgebraicValue::String(s), AlgebraicType::String) => ser.serialize_str(s),
            (val, ty) => panic!("mismatched value and schema : {val:?} {ty:?}"),
        };
    }
});
impl_serialize!(
    [T: crate::Value] where [for<'a> ValueWithType<'a, T>: Serialize]
    ValueWithType<'_, Box<[T]>>,
    (self, ser) => {
        let mut vec = ser.serialize_array(self.value().len())?;
        for val in self.iter() {
            vec.serialize_element(&val)?;
        }
        vec.end()
    }
);
impl_serialize!([] ValueWithType<'_, SumValue>, (self, ser) => {
   ser.serialize_variant_raw(self)
});
impl_serialize!([] ValueWithType<'_, ProductValue>, (self, ser) => {
    ser.serialize_named_product_raw(self)
});
impl_serialize!([] ValueWithType<'_, ArrayValue>, (self, ser) => {
    let mut ty = &*self.ty().elem_ty;
    loop { // We're doing this because of `Ref`s.
        break match (self.value(), ty) {
            (_, &AlgebraicType::Ref(r)) => {
                ty = &self.typespace()[r];
                continue;
            }
            (ArrayValue::Sum(v), AlgebraicType::Sum(ty)) => self.with(ty, v).serialize(ser),
            (ArrayValue::Product(v), AlgebraicType::Product(ty)) => self.with(ty, v).serialize(ser),
            (ArrayValue::Bool(v), AlgebraicType::Bool) => v.serialize(ser),
            (ArrayValue::I8(v), AlgebraicType::I8) => v.serialize(ser),
            (ArrayValue::U8(v), AlgebraicType::U8) => v.serialize(ser),
            (ArrayValue::I16(v), AlgebraicType::I16) => v.serialize(ser),
            (ArrayValue::U16(v), AlgebraicType::U16) => v.serialize(ser),
            (ArrayValue::I32(v), AlgebraicType::I32) => v.serialize(ser),
            (ArrayValue::U32(v), AlgebraicType::U32) => v.serialize(ser),
            (ArrayValue::I64(v), AlgebraicType::I64) => v.serialize(ser),
            (ArrayValue::U64(v), AlgebraicType::U64) => v.serialize(ser),
            (ArrayValue::I128(v), AlgebraicType::I128) => v.serialize(ser),
            (ArrayValue::U128(v), AlgebraicType::U128) => v.serialize(ser),
            (ArrayValue::I256(v), AlgebraicType::I256) => v.serialize(ser),
            (ArrayValue::U256(v), AlgebraicType::U256) => v.serialize(ser),
            (ArrayValue::F32(v), AlgebraicType::F32) => v.serialize(ser),
            (ArrayValue::F64(v), AlgebraicType::F64) => v.serialize(ser),
            (ArrayValue::String(v), AlgebraicType::String) => v.serialize(ser),
            (ArrayValue::Array(v), AlgebraicType::Array(ty)) => self.with(ty, v).serialize(ser),
            (val, _) if val.is_empty() => ser.serialize_array(0)?.end(),
            (val, ty) => panic!("mismatched value and schema: {val:?} {ty:?}"),
        }
    }
});

impl_serialize!([] spacetimedb_primitives::TableId, (self, ser) => ser.serialize_u32(self.0));
impl_serialize!([] spacetimedb_primitives::ViewId, (self, ser) => ser.serialize_u32(self.0));
impl_serialize!([] spacetimedb_primitives::SequenceId, (self, ser) => ser.serialize_u32(self.0));
impl_serialize!([] spacetimedb_primitives::IndexId, (self, ser) => ser.serialize_u32(self.0));
impl_serialize!([] spacetimedb_primitives::ConstraintId, (self, ser) => ser.serialize_u32(self.0));
impl_serialize!([] spacetimedb_primitives::ColId, (self, ser) => ser.serialize_u16(self.0));
impl_serialize!([] spacetimedb_primitives::ScheduleId, (self, ser) => ser.serialize_u32(self.0));

impl_serialize!([] ColList, (self, ser) => {
    let mut arr = ser.serialize_array(self.len() as usize)?;
       for x in self.iter() {
           arr.serialize_element(&x)?;
       }
       arr.end()
});
impl_serialize!([] ColSet, (self, ser) => {
    let list: &ColList = self;
    list.serialize(ser)
});

#[cfg(feature = "blake3")]
impl_serialize!([] blake3::Hash, (self, ser) => self.as_bytes().serialize(ser));

impl_serialize!([] bytes::Bytes, (self, ser) => ser.serialize_bytes(self));

#[cfg(feature = "bytestring")]
impl_serialize!([] bytestring::ByteString, (self, ser) => ser.serialize_str(self));
