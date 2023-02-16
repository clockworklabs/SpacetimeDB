use std::fmt;

use crate::buffer::BufWriter;

use crate::ser::{self, Error, ForwardNamedToSeqProduct, Serialize, SerializeArray, SerializeMap, SerializeSeqProduct};

pub struct Serializer<'a, W> {
    writer: &'a mut W,
}

impl<'a, W> Serializer<'a, W> {
    pub fn new(writer: &'a mut W) -> Self {
        Self { writer }
    }

    #[inline]
    fn reborrow(&mut self) -> Serializer<'_, W> {
        Serializer {
            writer: &mut self.writer,
        }
    }
}

#[derive(Debug)]
pub struct BsatnError {
    custom: String,
}
impl fmt::Display for BsatnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.custom)
    }
}
impl std::error::Error for BsatnError {}
impl Error for BsatnError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        let custom = msg.to_string();
        Self { custom }
    }
}

impl<W: BufWriter> ser::Serializer for Serializer<'_, W> {
    type Ok = ();
    type Error = BsatnError;
    type SerializeArray = Self;
    type SerializeMap = Self;
    type SerializeSeqProduct = Self;
    type SerializeNamedProduct = ForwardNamedToSeqProduct<Self>;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        self.writer.put_u8(v as u8);
        Ok(())
    }
    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.writer.put_u8(v);
        Ok(())
    }
    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.writer.put_u16(v);
        Ok(())
    }
    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.writer.put_u32(v);
        Ok(())
    }
    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        self.writer.put_u64(v);
        Ok(())
    }
    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> {
        self.writer.put_u128(v);
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.writer.put_i8(v);
        Ok(())
    }
    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.writer.put_i16(v);
        Ok(())
    }
    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.writer.put_i32(v);
        Ok(())
    }
    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        self.writer.put_i64(v);
        Ok(())
    }
    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> {
        self.writer.put_i128(v);
        Ok(())
    }
    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        self.writer.put_u32(v.to_bits());
        Ok(())
    }
    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        self.writer.put_u64(v.to_bits());
        Ok(())
    }
    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        self.serialize_bytes(v.as_bytes())
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        let len = v.len().try_into().map_err(|_| Error::custom("too big"))?;
        self.writer.put_u16(len);
        self.writer.put_slice(v);
        Ok(())
    }
    fn serialize_array(self, len: usize) -> Result<Self::SerializeArray, Self::Error> {
        let len = len.try_into().map_err(|_| Error::custom("too big"))?;
        self.writer.put_u16(len);
        Ok(self)
    }
    fn serialize_map(self, len: usize) -> Result<Self::SerializeMap, Self::Error> {
        let len = len.try_into().map_err(|_| Error::custom("too big"))?;
        self.writer.put_u16(len);
        Ok(self)
    }
    fn serialize_seq_product(self, _len: usize) -> Result<Self::SerializeSeqProduct, Self::Error> {
        Ok(self)
    }
    fn serialize_named_product(self, len: usize) -> Result<Self::SerializeNamedProduct, Self::Error> {
        self.serialize_seq_product(len).map(ForwardNamedToSeqProduct::new)
    }
    fn serialize_variant<T: super::Serialize + ?Sized>(
        self,
        tag: u8,
        _name: Option<&str>,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.writer.put_u8(tag);
        value.serialize(self)
    }
}

impl<W: BufWriter> SerializeArray for Serializer<'_, W> {
    type Ok = ();
    type Error = BsatnError;

    fn serialize_element<T: super::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        elem.serialize(self.reborrow())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<W: BufWriter> SerializeMap for Serializer<'_, W> {
    type Ok = ();
    type Error = BsatnError;

    fn serialize_entry<K: Serialize + ?Sized, V: Serialize + ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error> {
        key.serialize(self.reborrow())?;
        value.serialize(self.reborrow())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<W: BufWriter> SerializeSeqProduct for Serializer<'_, W> {
    type Ok = ();
    type Error = BsatnError;

    fn serialize_element<T: super::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        elem.serialize(self.reborrow())
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}
