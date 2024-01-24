use super::Serialize as _;
use crate::{
    algebraic_value::ser::ValueSerializer,
    ser::{self, Serializer},
};
use ::serde::ser as serde;
use std::fmt;

/// Converts any [`serde::Serializer`] to a SATS [`Serializer`]
/// so that Serde's data formats can be reused.
pub struct SerdeSerializer<S> {
    /// A serialization data format in Serde.
    ser: S,
}

impl<S: serde::Serializer> SerdeSerializer<S> {
    /// Returns a wrapped serializer.
    pub fn new(ser: S) -> Self {
        Self { ser }
    }
}

/// An error that occured when serializing SATS to a Serde data format.
pub struct SerdeError<E>(pub E);

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

    unsafe fn serialize_bsatn(self, ty: &crate::AlgebraicType, bsatn: &[u8]) -> Result<Self::Ok, Self::Error> {
        // TODO(Centril): Consider instead deserializing the `bsatn` through a
        // deserializer that serializes into `self` directly.

        // First convert the BSATN to an `AlgebraicValue`.
        // SAFETY: Forward caller requirements of this method to that we are calling.
        let res = unsafe { ValueSerializer.serialize_bsatn(ty, bsatn) };
        let value = res.unwrap_or_else(|x| match x {});

        // Then serialize that.
        value.serialize(self)
    }

    unsafe fn serialize_bsatn_in_chunks<'a, I: Clone + Iterator<Item = &'a [u8]>>(
        self,
        ty: &crate::AlgebraicType,
        total_bsatn_len: usize,
        bsatn: I,
    ) -> Result<Self::Ok, Self::Error> {
        // TODO(Centril): Unlike above, in this case we must at minimum concatenate `bsatn`
        // before we can do the piping mentioned above, but that's better than
        // serializing to `AlgebraicValue` first, so consider that.

        // First convert the BSATN to an `AlgebraicValue`.
        // SAFETY: Forward caller requirements of this method to that we are calling.
        let res = unsafe { ValueSerializer.serialize_bsatn_in_chunks(ty, total_bsatn_len, bsatn) };
        let value = res.unwrap_or_else(|x| match x {});

        // Then serialize that.
        value.serialize(self)
    }

    unsafe fn serialize_str_in_chunks<'a, I: Clone + Iterator<Item = &'a [u8]>>(
        self,
        total_len: usize,
        string: I,
    ) -> Result<Self::Ok, Self::Error> {
        // First convert the `string` to an `AlgebraicValue`.
        // SAFETY: Forward caller requirements of this method to that we are calling.
        let res = unsafe { ValueSerializer.serialize_str_in_chunks(total_len, string) };
        let value = res.unwrap_or_else(|x| match x {});

        // Then serialize that.
        // This incurs a very minor cost of branching on `AlgebraicValue::String`.
        value.serialize(self)
    }
}

/// Serializes array elements by forwarding to `S: serde::SerializeSeq`.
pub struct SerializeArray<S> {
    /// An implementation of `serde::SerializeSeq`.
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

/// Serializes map entries by forwarding to `S: serde::SerializeMap`.
pub struct SerializeMap<S> {
    /// An implementation of `serde::SerializeMap`.
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

/// Serializes unnamed product elements by forwarding to `S: serde::SerializeTuple`.
pub struct SerializeSeqProduct<S> {
    /// An implementation of `serde::SerializeTuple`.
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

/// Serializes named product elements by forwarding to `S: serde::SerializeMap`.
pub struct SerializeNamedProduct<S> {
    /// An implementation of `serde::SerializeMap`.
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

/// Serializes `T` as a SATS object into `serializer: S`
/// where `S` is a serde data format.
pub fn serialize_to<T: super::Serialize + ?Sized, S: serde::Serializer>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value
        .serialize(SerdeSerializer::new(serializer))
        .map_err(|SerdeError(e)| e)
}

/// Turns a type serializable in SATS into one serializable in serde.
///
/// That is, `T: sats::Serialize => SerializeWrapper<T>: serde::Serialize`.
#[repr(transparent)]
pub struct SerializeWrapper<T: ?Sized>(T);

impl<T: ?Sized> SerializeWrapper<T> {
    /// Wraps an object serializable in SATS so that it's serializable in Serde.
    pub fn new(t: T) -> Self
    where
        T: Sized,
    {
        Self(t)
    }

    /// Converts `&T` to `&SerializeWrapper<T>`.
    pub fn from_ref(t: &T) -> &Self {
        // SAFETY: OK because of `repr(transparent)`.
        unsafe { &*(t as *const T as *const SerializeWrapper<T>) }
    }
}

impl<T: ser::Serialize + ?Sized> serde::Serialize for SerializeWrapper<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
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
