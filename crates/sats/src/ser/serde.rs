use std::fmt;

use ::serde::ser as serde;

use crate::ser::{self, Serializer};

pub struct SerdeSerializer<S> {
    ser: S,
}

impl<S> SerdeSerializer<S> {
    pub fn new(ser: S) -> Self {
        Self { ser }
    }
}

pub struct SerdeError<E>(pub E);
fn unwrap_error<E>(err: SerdeError<E>) -> E {
    let SerdeError(err) = err;
    err
}

impl<E: serde::Error> ser::Error for SerdeError<E> {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(E::custom(msg))
    }
}

impl<S: serde::Serializer> Serializer for SerdeSerializer<S> {
    type Ok = S::Ok;
    type Error = SerdeError<S::Error>;
    type SerializeArray = SerializeArray<S::SerializeSeq>;
    type SerializeMap = SerializeMap<S::SerializeMap>;
    type SerializeSeqProduct = SerializeSeqProduct<S::SerializeTuple>;
    type SerializeNamedProduct = SerializeNamedProduct<S::SerializeMap>;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_bool(v).map_err(SerdeError)
    }
    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_u8(v).map_err(SerdeError)
    }
    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_u16(v).map_err(SerdeError)
    }
    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_u32(v).map_err(SerdeError)
    }
    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_u64(v).map_err(SerdeError)
    }
    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_u128(v).map_err(SerdeError)
    }
    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_i8(v).map_err(SerdeError)
    }
    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_i16(v).map_err(SerdeError)
    }
    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_i32(v).map_err(SerdeError)
    }
    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_i64(v).map_err(SerdeError)
    }
    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_i128(v).map_err(SerdeError)
    }
    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_f32(v).map_err(SerdeError)
    }
    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_f64(v).map_err(SerdeError)
    }
    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        self.ser.serialize_str(v).map_err(SerdeError)
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        let s = hex::encode(v);
        self.ser.serialize_str(&s).map_err(SerdeError)
    }

    fn serialize_array(self, len: usize) -> Result<Self::SerializeArray, Self::Error> {
        let seq = self.ser.serialize_seq(Some(len)).map_err(SerdeError)?;
        Ok(SerializeArray { seq })
    }

    fn serialize_map(self, len: usize) -> Result<Self::SerializeMap, Self::Error> {
        let map = self.ser.serialize_map(Some(len)).map_err(SerdeError)?;
        Ok(SerializeMap { map })
    }

    fn serialize_seq_product(self, len: usize) -> Result<Self::SerializeSeqProduct, Self::Error> {
        let tup = self.ser.serialize_tuple(len).map_err(SerdeError)?;
        Ok(SerializeSeqProduct { tup })
    }

    fn serialize_named_product(self, len: usize) -> Result<Self::SerializeNamedProduct, Self::Error> {
        let map = self.ser.serialize_map(Some(len)).map_err(SerdeError)?;
        Ok(SerializeNamedProduct { map })
    }

    fn serialize_variant<T: ser::Serialize + ?Sized>(
        self,
        tag: u8,
        name: Option<&str>,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        // can't use serialize_variant cause we're too dynamic :(
        use ::serde::ser::SerializeMap;
        let mut map = self.ser.serialize_map(Some(1)).map_err(SerdeError)?;
        let value = SerializeWrapper::from_ref(value);
        if let Some(name) = name {
            map.serialize_entry(name, value).map_err(SerdeError)?;
        } else {
            // FIXME: this probably wouldn't decode if you ran it back through
            map.serialize_entry(&tag, value).map_err(SerdeError)?;
        }
        map.end().map_err(SerdeError)
    }
}

pub struct SerializeArray<S> {
    seq: S,
}

impl<S: serde::SerializeSeq> ser::SerializeArray for SerializeArray<S> {
    type Ok = S::Ok;
    type Error = SerdeError<S::Error>;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        self.seq
            .serialize_element(SerializeWrapper::from_ref(elem))
            .map_err(SerdeError)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.seq.end().map_err(SerdeError)
    }
}

pub struct SerializeMap<S> {
    map: S,
}

impl<S: serde::SerializeMap> ser::SerializeMap for SerializeMap<S> {
    type Ok = S::Ok;
    type Error = SerdeError<S::Error>;

    fn serialize_entry<K: ser::Serialize + ?Sized, V: ser::Serialize + ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error> {
        self.map
            .serialize_entry(SerializeWrapper::from_ref(key), SerializeWrapper::from_ref(value))
            .map_err(SerdeError)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.map.end().map_err(SerdeError)
    }
}

pub struct SerializeSeqProduct<S> {
    tup: S,
}

impl<S: serde::SerializeTuple> ser::SerializeSeqProduct for SerializeSeqProduct<S> {
    type Ok = S::Ok;
    type Error = SerdeError<S::Error>;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        self.tup
            .serialize_element(SerializeWrapper::from_ref(elem))
            .map_err(SerdeError)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.tup.end().map_err(SerdeError)
    }
}

pub struct SerializeNamedProduct<S> {
    map: S,
}

impl<S: serde::SerializeMap> ser::SerializeNamedProduct for SerializeNamedProduct<S> {
    type Ok = S::Ok;
    type Error = SerdeError<S::Error>;

    fn serialize_element<T: ser::Serialize + ?Sized>(
        &mut self,
        name: Option<&str>,
        elem: &T,
    ) -> Result<(), Self::Error> {
        let name = name.ok_or_else(|| ser::Error::custom("tuple element has no name"))?;
        self.map
            .serialize_entry(name, SerializeWrapper::from_ref(elem))
            .map_err(SerdeError)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.map.end().map_err(SerdeError)
    }
}

pub fn serialize_to<T: super::Serialize + ?Sized, S: serde::Serializer>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value.serialize(SerdeSerializer::new(serializer)).map_err(unwrap_error)
}

#[repr(transparent)]
pub struct SerializeWrapper<T: ?Sized>(T);
impl<T: ?Sized> SerializeWrapper<T> {
    pub fn new(t: T) -> Self
    where
        T: Sized,
    {
        Self(t)
    }
    pub fn from_ref(t: &T) -> &Self {
        unsafe { &*(t as *const T as *const SerializeWrapper<T>) }
    }
}
impl<T: ser::Serialize + ?Sized> serde::Serialize for SerializeWrapper<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_to(&self.0, serializer)
    }
}

macro_rules! delegate_serde {
    ($($t:ty),*) => {
        $(impl serde::Serialize for $t {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serialize_to(self, serializer)
            }
        })*
    };
}

delegate_serde! {
    crate::AlgebraicType, crate::ProductType, crate::ProductTypeElement, crate::SumType, crate::SumTypeVariant,
    crate::AlgebraicValue, crate::ProductValue, crate::SumValue
}
