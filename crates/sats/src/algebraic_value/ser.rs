use std::convert::Infallible;

use super::AlgebraicValue;
use crate::ser::{self, ForwardNamedToSeqProduct};
use crate::ArrayValue;

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
        Ok(AlgebraicValue::String(v.into()))
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
}

/// Continuation for serializing an array.
pub struct SerializeArrayValue {
    /// For efficiency, the first time `serialize_element` is done,
    /// this is used to allocate with capacity.
    len: Option<usize>,
    /// The array being built.
    array: ArrayValue,
}

impl ser::SerializeArray for SerializeArrayValue {
    type Ok = AlgebraicValue;
    type Error = Infallible;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        self.array
            .push(elem.serialize(ValueSerializer)?, self.len.take())
            .expect("heterogeneous array");
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::ArrayOf(self.array))
    }
}

/// Continuation for serializing a map value.
pub struct SerializeMapValue {
    /// The entry pairs to collect and convert into a map.
    entries: Vec<(AlgebraicValue, AlgebraicValue)>,
}

impl ser::SerializeMap for SerializeMapValue {
    type Ok = AlgebraicValue;
    type Error = Infallible;

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
    type Error = Infallible;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        self.elements.push(elem.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(AlgebraicValue::product(self.elements))
    }
}
