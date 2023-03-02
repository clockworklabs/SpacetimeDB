use std::collections::BTreeMap;

use crate::{AlgebraicType, AlgebraicValue, BuiltinType, BuiltinValue, MapType, ProductValue, SumValue, ValueWithType};

use super::{Serialize, SerializeArray, SerializeMap, SerializeNamedProduct, SerializeSeqProduct, Serializer};

macro_rules! impl_prim {
    ($(($prim:ty, $method:ident))*) => {
        $(impl Serialize for $prim {
            fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                ser.$method((*self).into())
            }
        })*
    };
}

impl Serialize for () {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_seq_product(0)?.end()
    }
}

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

impl<T: Serialize> Serialize for Vec<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(serializer)
    }
}
impl<T: Serialize> Serialize for [T] {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        T::__serialize_array(self, serializer)
    }
}

impl<T: Serialize + ?Sized> Serialize for Box<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(serializer)
    }
}
impl<T: Serialize + ?Sized> Serialize for &T {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(serializer)
    }
}

impl Serialize for String {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self)
    }
}

impl<T: Serialize> Serialize for Option<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Some(v) => serializer.serialize_variant(0, Some("some"), v),
            None => serializer.serialize_variant(1, Some("none"), &()),
        }
    }
}

impl<K: Serialize, V: Serialize> Serialize for BTreeMap<K, V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(self.len())?;
        for (k, v) in self {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl Serialize for AlgebraicValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            AlgebraicValue::Sum(sum) => sum.serialize(serializer),
            AlgebraicValue::Product(prod) => prod.serialize(serializer),
            AlgebraicValue::Builtin(b) => b.serialize(serializer),
        }
    }
}

impl Serialize for BuiltinValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            BuiltinValue::Bool(v) => serializer.serialize_bool(*v),
            BuiltinValue::I8(v) => serializer.serialize_i8(*v),
            BuiltinValue::U8(v) => serializer.serialize_u8(*v),
            BuiltinValue::I16(v) => serializer.serialize_i16(*v),
            BuiltinValue::U16(v) => serializer.serialize_u16(*v),
            BuiltinValue::I32(v) => serializer.serialize_i32(*v),
            BuiltinValue::U32(v) => serializer.serialize_u32(*v),
            BuiltinValue::I64(v) => serializer.serialize_i64(*v),
            BuiltinValue::U64(v) => serializer.serialize_u64(*v),
            BuiltinValue::I128(v) => serializer.serialize_i128(*v),
            BuiltinValue::U128(v) => serializer.serialize_u128(*v),
            BuiltinValue::F32(v) => serializer.serialize_f32((*v).into()),
            BuiltinValue::F64(v) => serializer.serialize_f64((*v).into()),
            BuiltinValue::String(v) => serializer.serialize_str(v),
            BuiltinValue::Bytes(v) => serializer.serialize_bytes(v),
            BuiltinValue::Array { val } => val.serialize(serializer),
            BuiltinValue::Map { val } => val.serialize(serializer),
        }
    }
}

impl Serialize for ProductValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut tup = serializer.serialize_seq_product(self.elements.len())?;
        for elem in &*self.elements {
            tup.serialize_element(elem)?;
        }
        tup.end()
    }
}

impl Serialize for SumValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_variant(self.tag, None, &*self.value)
    }
}

impl Serialize for ValueWithType<'_, AlgebraicValue> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut ty = self.ty();
        loop {
            break match (self.value(), ty) {
                (AlgebraicValue::Sum(val), AlgebraicType::Sum(ty)) => self.with(ty, val).serialize(serializer),
                (AlgebraicValue::Product(val), AlgebraicType::Product(ty)) => self.with(ty, val).serialize(serializer),
                (AlgebraicValue::Builtin(val), AlgebraicType::Builtin(ty)) => self.with(ty, val).serialize(serializer),
                (_, &AlgebraicType::Ref(r)) => {
                    ty = &self.typespace()[r];
                    continue;
                }
                _ => panic!("mismatched value and schema"),
            };
        }
    }
}

impl Serialize for ValueWithType<'_, BuiltinValue> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match (self.value(), self.ty()) {
            (BuiltinValue::Bool(v), BuiltinType::Bool) => serializer.serialize_bool(*v),
            (BuiltinValue::I8(v), BuiltinType::I8) => serializer.serialize_i8(*v),
            (BuiltinValue::U8(v), BuiltinType::U8) => serializer.serialize_u8(*v),
            (BuiltinValue::I16(v), BuiltinType::I16) => serializer.serialize_i16(*v),
            (BuiltinValue::U16(v), BuiltinType::U16) => serializer.serialize_u16(*v),
            (BuiltinValue::I32(v), BuiltinType::I32) => serializer.serialize_i32(*v),
            (BuiltinValue::U32(v), BuiltinType::U32) => serializer.serialize_u32(*v),
            (BuiltinValue::I64(v), BuiltinType::I64) => serializer.serialize_i64(*v),
            (BuiltinValue::U64(v), BuiltinType::U64) => serializer.serialize_u64(*v),
            (BuiltinValue::I128(v), BuiltinType::I128) => serializer.serialize_i128(*v),
            (BuiltinValue::U128(v), BuiltinType::U128) => serializer.serialize_u128(*v),
            (BuiltinValue::F32(v), BuiltinType::F32) => serializer.serialize_f32((*v).into()),
            (BuiltinValue::F64(v), BuiltinType::F64) => serializer.serialize_f64((*v).into()),
            (BuiltinValue::String(s), BuiltinType::String) => serializer.serialize_str(s),
            (BuiltinValue::Bytes(b), BuiltinType::Array { ty }) if **ty == AlgebraicType::U8 => {
                serializer.serialize_bytes(b)
            }
            (BuiltinValue::Array { val }, BuiltinType::Array { ty }) => {
                let mut vec = serializer.serialize_array(val.len())?;
                for val in val {
                    vec.serialize_element(&self.with(&**ty, val))?;
                }
                vec.end()
            }
            (BuiltinValue::Map { val }, BuiltinType::Map(MapType { key_ty, ty })) => {
                let mut map = serializer.serialize_map(val.len())?;
                for (key, val) in val {
                    map.serialize_entry(&self.with(&**key_ty, key), &self.with(&**ty, val))?;
                }
                map.end()
            }
            (val, ty) => panic!("mismatched value and schema: {val:?} {ty:?}"),
        }
    }
}

impl Serialize for ValueWithType<'_, SumValue> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let &SumValue { tag, ref value } = self.value();
        let var_ty = &self.ty().variants[tag as usize];
        serializer.serialize_variant(
            tag,
            var_ty.name.as_deref(),
            &self.with(&var_ty.algebraic_type, &**value),
        )
    }
}

impl Serialize for ValueWithType<'_, ProductValue> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let val = &self.value().elements;
        assert_eq!(val.len(), self.ty().elements.len());
        let mut prod = serializer.serialize_named_product(val.len())?;
        for (val, el_ty) in val.iter().zip(&self.ty().elements) {
            prod.serialize_element(el_ty.name.as_deref(), &self.with(&el_ty.algebraic_type, val))?
        }
        prod.end()
    }
}
