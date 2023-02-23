use crate::{de, AlgebraicValue, SumValue};

#[repr(transparent)]
pub struct ValueDeserializer {
    val: AlgebraicValue,
}

impl ValueDeserializer {
    pub fn new(val: AlgebraicValue) -> Self {
        Self { val }
    }
    pub fn from_ref(val: &AlgebraicValue) -> &Self {
        unsafe { &*(val as *const AlgebraicValue as *const ValueDeserializer) }
    }
}
impl From<AlgebraicValue> for ValueDeserializer {
    fn from(val: AlgebraicValue) -> Self {
        Self { val }
    }
}

#[derive(Debug)]
pub enum ValueDeserializeError {
    MismatchedType,
    Custom(String),
}
impl de::Error for ValueDeserializeError {
    fn custom(msg: impl std::fmt::Display) -> Self {
        Self::Custom(msg.to_string())
    }
}

impl<'de> de::Deserializer<'de> for ValueDeserializer {
    type Error = ValueDeserializeError;

    fn deserialize_product<V: de::ProductVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let prod = self
            .val
            .into_product()
            .map_err(|_| ValueDeserializeError::MismatchedType)?;
        let vals = prod.elements.into_iter();
        visitor.visit_seq_product(ProductAccess { vals })
    }

    fn deserialize_sum<V: de::SumVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let sum = self.val.into_sum().map_err(|_| ValueDeserializeError::MismatchedType)?;
        visitor.visit_sum(SumAccess { sum })
    }

    fn deserialize_bool(self) -> Result<bool, Self::Error> {
        self.val.into_bool().map_err(|_| ValueDeserializeError::MismatchedType)
    }
    fn deserialize_u8(self) -> Result<u8, Self::Error> {
        self.val.into_u8().map_err(|_| ValueDeserializeError::MismatchedType)
    }
    fn deserialize_u16(self) -> Result<u16, Self::Error> {
        self.val.into_u16().map_err(|_| ValueDeserializeError::MismatchedType)
    }
    fn deserialize_u32(self) -> Result<u32, Self::Error> {
        self.val.into_u32().map_err(|_| ValueDeserializeError::MismatchedType)
    }
    fn deserialize_u64(self) -> Result<u64, Self::Error> {
        self.val.into_u64().map_err(|_| ValueDeserializeError::MismatchedType)
    }
    fn deserialize_u128(self) -> Result<u128, Self::Error> {
        self.val.into_u128().map_err(|_| ValueDeserializeError::MismatchedType)
    }
    fn deserialize_i8(self) -> Result<i8, Self::Error> {
        self.val.into_i8().map_err(|_| ValueDeserializeError::MismatchedType)
    }
    fn deserialize_i16(self) -> Result<i16, Self::Error> {
        self.val.into_i16().map_err(|_| ValueDeserializeError::MismatchedType)
    }
    fn deserialize_i32(self) -> Result<i32, Self::Error> {
        self.val.into_i32().map_err(|_| ValueDeserializeError::MismatchedType)
    }
    fn deserialize_i64(self) -> Result<i64, Self::Error> {
        self.val.into_i64().map_err(|_| ValueDeserializeError::MismatchedType)
    }
    fn deserialize_i128(self) -> Result<i128, Self::Error> {
        self.val.into_i128().map_err(|_| ValueDeserializeError::MismatchedType)
    }
    fn deserialize_f32(self) -> Result<f32, Self::Error> {
        self.val
            .into_f32()
            .map(f32::from)
            .map_err(|_| ValueDeserializeError::MismatchedType)
    }
    fn deserialize_f64(self) -> Result<f64, Self::Error> {
        self.val
            .into_f64()
            .map(f64::from)
            .map_err(|_| ValueDeserializeError::MismatchedType)
    }

    fn deserialize_str<V: de::SliceVisitor<'de, str>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let s = self
            .val
            .into_string()
            .map_err(|_| ValueDeserializeError::MismatchedType)?;
        visitor.visit_owned(s)
    }

    fn deserialize_bytes<V: de::SliceVisitor<'de, [u8]>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let b = self
            .val
            .into_bytes()
            .map_err(|_| ValueDeserializeError::MismatchedType)?;
        visitor.visit_owned(b)
    }

    fn deserialize_array_seed<V: de::ArrayVisitor<'de, T::Output>, T: de::DeserializeSeed<'de> + Clone>(
        self,
        visitor: V,
        seed: T,
    ) -> Result<V::Output, Self::Error> {
        let iter = self
            .val
            .into_array()
            .map_err(|_| ValueDeserializeError::MismatchedType)?
            .into_iter();
        visitor.visit(ArrayAccess { iter, seed })
    }

    fn deserialize_map_seed<
        Vi: de::MapVisitor<'de, K::Output, V::Output>,
        K: de::DeserializeSeed<'de> + Clone,
        V: de::DeserializeSeed<'de> + Clone,
    >(
        self,
        visitor: Vi,
        kseed: K,
        vseed: V,
    ) -> Result<Vi::Output, Self::Error> {
        let iter = self
            .val
            .into_map()
            .map_err(|_| ValueDeserializeError::MismatchedType)?
            .into_iter();
        visitor.visit(MapAccess { iter, kseed, vseed })
    }
}

struct ProductAccess {
    vals: std::vec::IntoIter<AlgebraicValue>,
}

impl<'de> de::SeqProductAccess<'de> for ProductAccess {
    type Error = ValueDeserializeError;

    fn next_element_seed<T: de::DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Output>, Self::Error> {
        self.vals
            .next()
            .map(|val| seed.deserialize(ValueDeserializer { val }))
            .transpose()
    }
}

#[repr(transparent)]
struct SumAccess {
    sum: SumValue,
}
impl SumAccess {
    fn from_ref(sum: &SumValue) -> &Self {
        unsafe { &*(sum as *const SumValue as *const SumAccess) }
    }
}

impl<'de> de::SumAccess<'de> for SumAccess {
    type Error = ValueDeserializeError;

    type Variant = ValueDeserializer;

    fn variant<V: de::VariantVisitor>(self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error> {
        let tag = visitor.visit_tag(self.sum.tag)?;
        let val = *self.sum.value;
        Ok((tag, ValueDeserializer { val }))
    }
}

impl<'de> de::VariantAccess<'de> for ValueDeserializer {
    type Error = ValueDeserializeError;

    fn deserialize_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Output, Self::Error> {
        seed.deserialize(self)
    }
}

struct ArrayAccess<T> {
    iter: std::vec::IntoIter<AlgebraicValue>,
    seed: T,
}

impl<'de, T: de::DeserializeSeed<'de> + Clone> de::ArrayAccess<'de> for ArrayAccess<T> {
    type Element = T::Output;
    type Error = ValueDeserializeError;

    fn next_element(&mut self) -> Result<Option<Self::Element>, Self::Error> {
        self.iter
            .next()
            .map(|val| self.seed.clone().deserialize(ValueDeserializer { val }))
            .transpose()
    }
}

struct MapAccess<K, V> {
    iter: std::collections::btree_map::IntoIter<AlgebraicValue, AlgebraicValue>,
    kseed: K,
    vseed: V,
}

impl<'de, K: de::DeserializeSeed<'de> + Clone, V: de::DeserializeSeed<'de> + Clone> de::MapAccess<'de>
    for MapAccess<K, V>
{
    type Key = K::Output;
    type Value = V::Output;
    type Error = ValueDeserializeError;

    fn next_entry(&mut self) -> Result<Option<(Self::Key, Self::Value)>, Self::Error> {
        self.iter
            .next()
            .map(|(key, val)| {
                Ok((
                    self.kseed.clone().deserialize(ValueDeserializer { val: key })?,
                    self.vseed.clone().deserialize(ValueDeserializer { val })?,
                ))
            })
            .transpose()
    }
}

impl<'de> de::Deserializer<'de> for &'de ValueDeserializer {
    type Error = ValueDeserializeError;

    fn deserialize_product<V: de::ProductVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let prod = self.val.as_product().ok_or(ValueDeserializeError::MismatchedType)?;
        let vals = prod.elements.iter();
        visitor.visit_seq_product(RefProductAccess { vals })
    }

    fn deserialize_sum<V: de::SumVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let sum = self.val.as_sum().ok_or(ValueDeserializeError::MismatchedType)?;
        visitor.visit_sum(SumAccess::from_ref(sum))
    }

    fn deserialize_bool(self) -> Result<bool, Self::Error> {
        self.val.as_bool().copied().ok_or(ValueDeserializeError::MismatchedType)
    }
    fn deserialize_u8(self) -> Result<u8, Self::Error> {
        self.val.as_u8().copied().ok_or(ValueDeserializeError::MismatchedType)
    }
    fn deserialize_u16(self) -> Result<u16, Self::Error> {
        self.val.as_u16().copied().ok_or(ValueDeserializeError::MismatchedType)
    }
    fn deserialize_u32(self) -> Result<u32, Self::Error> {
        self.val.as_u32().copied().ok_or(ValueDeserializeError::MismatchedType)
    }
    fn deserialize_u64(self) -> Result<u64, Self::Error> {
        self.val.as_u64().copied().ok_or(ValueDeserializeError::MismatchedType)
    }
    fn deserialize_u128(self) -> Result<u128, Self::Error> {
        self.val.as_u128().copied().ok_or(ValueDeserializeError::MismatchedType)
    }
    fn deserialize_i8(self) -> Result<i8, Self::Error> {
        self.val.as_i8().copied().ok_or(ValueDeserializeError::MismatchedType)
    }
    fn deserialize_i16(self) -> Result<i16, Self::Error> {
        self.val.as_i16().copied().ok_or(ValueDeserializeError::MismatchedType)
    }
    fn deserialize_i32(self) -> Result<i32, Self::Error> {
        self.val.as_i32().copied().ok_or(ValueDeserializeError::MismatchedType)
    }
    fn deserialize_i64(self) -> Result<i64, Self::Error> {
        self.val.as_i64().copied().ok_or(ValueDeserializeError::MismatchedType)
    }
    fn deserialize_i128(self) -> Result<i128, Self::Error> {
        self.val.as_i128().copied().ok_or(ValueDeserializeError::MismatchedType)
    }
    fn deserialize_f32(self) -> Result<f32, Self::Error> {
        self.val
            .as_f32()
            .copied()
            .map(f32::from)
            .ok_or(ValueDeserializeError::MismatchedType)
    }
    fn deserialize_f64(self) -> Result<f64, Self::Error> {
        self.val
            .as_f64()
            .copied()
            .map(f64::from)
            .ok_or(ValueDeserializeError::MismatchedType)
    }

    fn deserialize_str<V: de::SliceVisitor<'de, str>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let s = self.val.as_string().ok_or(ValueDeserializeError::MismatchedType)?;
        visitor.visit_borrowed(s)
    }

    fn deserialize_bytes<V: de::SliceVisitor<'de, [u8]>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let b = self.val.as_bytes().ok_or(ValueDeserializeError::MismatchedType)?;
        visitor.visit_borrowed(b)
    }

    fn deserialize_array_seed<V: de::ArrayVisitor<'de, T::Output>, T: de::DeserializeSeed<'de> + Clone>(
        self,
        visitor: V,
        seed: T,
    ) -> Result<V::Output, Self::Error> {
        let iter = self.val.as_array().ok_or(ValueDeserializeError::MismatchedType)?.iter();
        visitor.visit(RefArrayAccess { iter, seed })
    }

    fn deserialize_map_seed<
        Vi: de::MapVisitor<'de, K::Output, V::Output>,
        K: de::DeserializeSeed<'de> + Clone,
        V: de::DeserializeSeed<'de> + Clone,
    >(
        self,
        visitor: Vi,
        kseed: K,
        vseed: V,
    ) -> Result<Vi::Output, Self::Error> {
        let iter = self.val.as_map().ok_or(ValueDeserializeError::MismatchedType)?.iter();
        visitor.visit(RefMapAccess { iter, kseed, vseed })
    }
}

struct RefProductAccess<'a> {
    vals: std::slice::Iter<'a, AlgebraicValue>,
}

impl<'de> de::SeqProductAccess<'de> for RefProductAccess<'de> {
    type Error = ValueDeserializeError;

    fn next_element_seed<T: de::DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Output>, Self::Error> {
        self.vals
            .next()
            .map(|val| seed.deserialize(ValueDeserializer::from_ref(val)))
            .transpose()
    }
}

impl<'de> de::SumAccess<'de> for &'de SumAccess {
    type Error = ValueDeserializeError;

    type Variant = &'de ValueDeserializer;

    fn variant<V: de::VariantVisitor>(self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error> {
        let tag = visitor.visit_tag(self.sum.tag)?;
        Ok((tag, ValueDeserializer::from_ref(&self.sum.value)))
    }
}

impl<'de> de::VariantAccess<'de> for &'de ValueDeserializer {
    type Error = ValueDeserializeError;

    fn deserialize_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Output, Self::Error> {
        seed.deserialize(self)
    }
}

struct RefArrayAccess<'a, T> {
    iter: std::slice::Iter<'a, AlgebraicValue>,
    seed: T,
}

impl<'de, T: de::DeserializeSeed<'de> + Clone> de::ArrayAccess<'de> for RefArrayAccess<'de, T> {
    type Element = T::Output;
    type Error = ValueDeserializeError;

    fn next_element(&mut self) -> Result<Option<Self::Element>, Self::Error> {
        self.iter
            .next()
            .map(|val| self.seed.clone().deserialize(ValueDeserializer::from_ref(val)))
            .transpose()
    }
}

struct RefMapAccess<'a, K, V> {
    iter: std::collections::btree_map::Iter<'a, AlgebraicValue, AlgebraicValue>,
    kseed: K,
    vseed: V,
}

impl<'de, K: de::DeserializeSeed<'de> + Clone, V: de::DeserializeSeed<'de> + Clone> de::MapAccess<'de>
    for RefMapAccess<'de, K, V>
{
    type Key = K::Output;
    type Value = V::Output;
    type Error = ValueDeserializeError;

    fn next_entry(&mut self) -> Result<Option<(Self::Key, Self::Value)>, Self::Error> {
        self.iter
            .next()
            .map(|(key, val)| {
                Ok((
                    self.kseed.clone().deserialize(ValueDeserializer::from_ref(key))?,
                    self.vseed.clone().deserialize(ValueDeserializer::from_ref(val))?,
                ))
            })
            .transpose()
    }
}
