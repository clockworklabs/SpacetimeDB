use std::convert::Infallible;

use super::AlgebraicValue;
use crate::ser::{self, ForwardNamedToSeqProduct};
use crate::{ArrayValue, BuiltinValue, ProductValue, SumValue};

pub struct ValueSerializer;

impl ser::Serializer for ValueSerializer {
    type Ok = AlgebraicValue;
    type Error = Infallible;

    type SerializeArray = SerializeArrayValue;
    type SerializeMap = SerializeMapValue;
    type SerializeSeqProduct = SerializeProductValue;
    type SerializeNamedProduct = ForwardNamedToSeqProduct<SerializeProductValue>;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::Bool(v))
    }
    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::U8(v))
    }
    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::U16(v))
    }
    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::U32(v))
    }
    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::U64(v))
    }
    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::U128(v))
    }
    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::I8(v))
    }
    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::I16(v))
    }
    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::I32(v))
    }
    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::I64(v))
    }
    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::I128(v))
    }
    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::F32(v.into()))
    }
    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::F64(v.into()))
    }
    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::String(v.to_owned()))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::Bytes(v.to_owned()))
    }

    fn serialize_array(self, _len: usize) -> Result<Self::SerializeArray, Self::Error> {
        Ok(SerializeArrayValue { v: Default::default() })
    }

    fn serialize_map(self, len: usize) -> Result<Self::SerializeMap, Self::Error> {
        Ok(SerializeMapValue {
            v: Vec::with_capacity(len),
        })
    }

    fn serialize_seq_product(self, len: usize) -> Result<Self::SerializeSeqProduct, Self::Error> {
        Ok(SerializeProductValue {
            v: Vec::with_capacity(len),
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
        let value = Box::new(value.serialize(self)?);
        Ok(AlgebraicValue::Sum(SumValue { tag, value }))
    }
}

pub struct SerializeArrayValue {
    v: ArrayValue,
}

impl ser::SerializeArray for SerializeArrayValue {
    type Ok = AlgebraicValue;
    type Error = Infallible;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        // TODO: this can be more efficient
        self.v
            .push(elem.serialize(ValueSerializer)?)
            .expect("heterogeneous array");
        Ok(())
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::Builtin(BuiltinValue::Array { val: self.v }))
    }
}

pub struct SerializeMapValue {
    v: Vec<(AlgebraicValue, AlgebraicValue)>,
}

impl ser::SerializeMap for SerializeMapValue {
    type Ok = AlgebraicValue;
    type Error = Infallible;

    fn serialize_entry<K: ser::Serialize + ?Sized, V: ser::Serialize + ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error> {
        self.v
            .push((key.serialize(ValueSerializer)?, value.serialize(ValueSerializer)?));
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::Builtin(BuiltinValue::Map {
            val: self.v.into_iter().collect(),
        }))
    }
}

pub struct SerializeProductValue {
    v: Vec<AlgebraicValue>,
}

impl ser::SerializeSeqProduct for SerializeProductValue {
    type Ok = AlgebraicValue;
    type Error = Infallible;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        self.v.push(elem.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::Product(ProductValue { elements: self.v }))
    }
}
