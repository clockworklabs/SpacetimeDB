mod impls;
#[cfg(feature = "serde")]
pub mod serde;

use std::fmt;

pub trait Serializer: Sized {
    type Ok;
    type Error: Error;
    type SerializeArray: SerializeArray<Ok = Self::Ok, Error = Self::Error>;
    type SerializeMap: SerializeMap<Ok = Self::Ok, Error = Self::Error>;
    type SerializeSeqProduct: SerializeSeqProduct<Ok = Self::Ok, Error = Self::Error>;
    type SerializeNamedProduct: SerializeNamedProduct<Ok = Self::Ok, Error = Self::Error>;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error>;
    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error>;
    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error>;
    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error>;
    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error>;
    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error>;
    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error>;
    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error>;
    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error>;
    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error>;
    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error>;
    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error>;
    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error>;
    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error>;
    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error>;

    fn serialize_array(self, len: usize) -> Result<Self::SerializeArray, Self::Error>;
    fn serialize_map(self, len: usize) -> Result<Self::SerializeMap, Self::Error>;

    fn serialize_seq_product(self, len: usize) -> Result<Self::SerializeSeqProduct, Self::Error>;
    fn serialize_named_product(self, len: usize) -> Result<Self::SerializeNamedProduct, Self::Error>;
    fn serialize_variant<T: Serialize + ?Sized>(
        self,
        tag: u8,
        name: Option<&str>,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>;
}

pub use spacetimedb_bindings_macro::Serialize;
pub trait Serialize {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error>;

    /// used in the Serialize for Vec<T> impl to allow specializing serializing Vec<T> as bytes
    #[doc(hidden)]
    #[inline(always)]
    fn __serialize_array<S: Serializer>(this: &[Self], serializer: S) -> Result<S::Ok, S::Error>
    where
        Self: Sized,
    {
        let mut vec = serializer.serialize_array(this.len())?;
        for elem in this {
            vec.serialize_element(elem)?;
        }
        vec.end()
    }
}

pub trait Error {
    fn custom<T: fmt::Display>(msg: T) -> Self;
}

impl Error for String {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        msg.to_string()
    }
}
impl Error for std::convert::Infallible {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        panic!("error generated for Infallible serializer: {msg}")
    }
}

pub trait SerializeArray {
    type Ok;
    type Error: Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error>;
    fn end(self) -> Result<Self::Ok, Self::Error>;
}

pub trait SerializeMap {
    type Ok;
    type Error: Error;

    fn serialize_entry<K: Serialize + ?Sized, V: Serialize + ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error>;
    fn end(self) -> Result<Self::Ok, Self::Error>;
}

pub trait SerializeSeqProduct {
    type Ok;
    type Error: Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error>;
    fn end(self) -> Result<Self::Ok, Self::Error>;
}

pub trait SerializeNamedProduct {
    type Ok;
    type Error: Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, name: Option<&str>, elem: &T) -> Result<(), Self::Error>;
    fn end(self) -> Result<Self::Ok, Self::Error>;
}

pub struct ForwardNamedToSeqProduct<S> {
    tup: S,
}
impl<S> ForwardNamedToSeqProduct<S> {
    pub fn new(tup: S) -> Self {
        Self { tup }
    }
    pub fn forward<Ser>(ser: Ser, len: usize) -> Result<Self, Ser::Error>
    where
        Ser: Serializer<SerializeSeqProduct = S>,
    {
        ser.serialize_seq_product(len).map(Self::new)
    }
}
impl<S: SerializeSeqProduct> SerializeNamedProduct for ForwardNamedToSeqProduct<S> {
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, _name: Option<&str>, elem: &T) -> Result<(), Self::Error> {
        self.tup.serialize_element(elem)
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.tup.end()
    }
}
