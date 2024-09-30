use crate::buffer::{BufReader, DecodeError};
use crate::de::{self, Deserializer as _, SeqProductAccess, SumAccess, VariantAccess};
use crate::{i256, u256};

/// Deserializer from the BSATN data format.
pub struct Deserializer<'a, R> {
    // The input to deserialize.
    reader: &'a mut R,
}

impl<'a, 'de, R: BufReader<'de>> Deserializer<'a, R> {
    /// Returns a deserializer using the given `reader`.
    pub fn new(reader: &'a mut R) -> Self {
        Self { reader }
    }

    /// Reborrows the deserializer.
    #[inline]
    pub(crate) fn reborrow(&mut self) -> Deserializer<'_, R> {
        Deserializer { reader: self.reader }
    }

    /// Reads a length as a `u32` then converted to `usize`.
    pub(crate) fn deserialize_len(self) -> Result<usize, DecodeError> {
        Ok(self.deserialize_u32()? as usize)
    }

    /// Reads a slice of `len` elements.
    pub(crate) fn get_slice(&mut self, len: usize) -> Result<&'de [u8], DecodeError> {
        self.reader.get_slice(len)
    }

    /// Reads a byte slice from the `reader`.
    fn deserialize_bytes_inner(mut self) -> Result<&'de [u8], DecodeError> {
        let len = self.reborrow().deserialize_len()?;
        self.get_slice(len)
    }
}

impl de::Error for DecodeError {
    fn custom(msg: impl std::fmt::Display) -> Self {
        DecodeError::Other(msg.to_string())
    }

    fn unknown_variant_tag<'de, T: de::SumVisitor<'de>>(tag: u8, expected: &T) -> Self {
        let sum_name = expected.sum_name().map(|x| x.to_owned());
        DecodeError::InvalidTag { tag, sum_name }
    }
}

impl<'de, 'a, R: BufReader<'de>> de::Deserializer<'de> for Deserializer<'a, R> {
    type Error = DecodeError;

    fn deserialize_product<V: de::ProductVisitor<'de>>(self, visitor: V) -> Result<V::Output, DecodeError> {
        visitor.visit_seq_product(self)
    }

    fn deserialize_sum<V: de::SumVisitor<'de>>(self, visitor: V) -> Result<V::Output, DecodeError> {
        visitor.visit_sum(self)
    }

    fn deserialize_bool(self) -> Result<bool, Self::Error> {
        self.reader.get_u8().map(|x| x != 0)
    }
    fn deserialize_u8(self) -> Result<u8, DecodeError> {
        self.reader.get_u8()
    }
    fn deserialize_u16(self) -> Result<u16, DecodeError> {
        self.reader.get_u16()
    }
    fn deserialize_u32(self) -> Result<u32, DecodeError> {
        self.reader.get_u32()
    }
    fn deserialize_u64(self) -> Result<u64, DecodeError> {
        self.reader.get_u64()
    }
    fn deserialize_u128(self) -> Result<u128, DecodeError> {
        self.reader.get_u128()
    }
    fn deserialize_u256(self) -> Result<u256, DecodeError> {
        self.reader.get_u256()
    }
    fn deserialize_i8(self) -> Result<i8, DecodeError> {
        self.reader.get_i8()
    }
    fn deserialize_i16(self) -> Result<i16, DecodeError> {
        self.reader.get_i16()
    }
    fn deserialize_i32(self) -> Result<i32, DecodeError> {
        self.reader.get_i32()
    }
    fn deserialize_i64(self) -> Result<i64, DecodeError> {
        self.reader.get_i64()
    }
    fn deserialize_i128(self) -> Result<i128, DecodeError> {
        self.reader.get_i128()
    }
    fn deserialize_i256(self) -> Result<i256, DecodeError> {
        self.reader.get_i256()
    }
    fn deserialize_f32(self) -> Result<f32, Self::Error> {
        self.reader.get_u32().map(f32::from_bits)
    }
    fn deserialize_f64(self) -> Result<f64, Self::Error> {
        self.reader.get_u64().map(f64::from_bits)
    }

    fn deserialize_str<V: de::SliceVisitor<'de, str>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let slice = self.deserialize_bytes_inner()?;
        let slice = core::str::from_utf8(slice)?;
        visitor.visit_borrowed(slice)
    }

    fn deserialize_bytes<V: de::SliceVisitor<'de, [u8]>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let slice = self.deserialize_bytes_inner()?;
        visitor.visit_borrowed(slice)
    }

    fn deserialize_array_seed<V: de::ArrayVisitor<'de, T::Output>, T: de::DeserializeSeed<'de> + Clone>(
        mut self,
        visitor: V,
        seed: T,
    ) -> Result<V::Output, Self::Error> {
        let len = self.reborrow().deserialize_len()?;
        let seeds = itertools::repeat_n(seed, len);
        visitor.visit(ArrayAccess { de: self, seeds })
    }
}

impl<'de, 'a, R: BufReader<'de>> SeqProductAccess<'de> for Deserializer<'a, R> {
    type Error = DecodeError;

    fn next_element_seed<T: de::DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Output>, DecodeError> {
        seed.deserialize(self.reborrow()).map(Some)
    }
}

impl<'de, 'a, R: BufReader<'de>> SumAccess<'de> for Deserializer<'a, R> {
    type Error = DecodeError;
    type Variant = Self;

    fn variant<V: de::VariantVisitor>(self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error> {
        let tag = self.reader.get_u8()?;
        visitor.visit_tag(tag).map(|variant| (variant, self))
    }
}

impl<'de, 'a, R: BufReader<'de>> VariantAccess<'de> for Deserializer<'a, R> {
    type Error = DecodeError;
    fn deserialize_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Output, Self::Error> {
        seed.deserialize(self)
    }
}

/// Deserializer for array elements.
pub struct ArrayAccess<'a, R, T> {
    de: Deserializer<'a, R>,
    seeds: itertools::RepeatN<T>,
}

impl<'de, 'a, R: BufReader<'de>, T: de::DeserializeSeed<'de> + Clone> de::ArrayAccess<'de> for ArrayAccess<'a, R, T> {
    type Element = T::Output;
    type Error = DecodeError;

    fn next_element(&mut self) -> Result<Option<T::Output>, Self::Error> {
        self.seeds
            .next()
            .map(|seed| seed.deserialize(self.de.reborrow()))
            .transpose()
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.seeds.len())
    }
}
