use crate::array_value::{ArrayValueIntoIter, ArrayValueIterCloned};
use crate::{de, AlgebraicValue, SumValue};

use derive_more::From;

/// An implementation of [`Deserializer`](de::Deserializer)
/// where the input of deserialization is an `AlgebraicValue`.
#[repr(transparent)]
#[derive(From)]
pub struct ValueDeserializer {
    /// The value to deserialize to some `T`.
    val: AlgebraicValue,
}

impl ValueDeserializer {
    /// Returns a `ValueDeserializer` with `val` as the input for deserialization.
    pub fn new(val: AlgebraicValue) -> Self {
        Self { val }
    }

    /// Converts `&AlgebraicValue` to `&ValueDeserialize`.
    pub fn from_ref(val: &AlgebraicValue) -> &Self {
        // SAFETY: The conversion is OK due to `repr(transparent)`.
        unsafe { &*(val as *const AlgebraicValue as *const ValueDeserializer) }
    }
}

/// Errors that can occur when deserializing the `AlgebraicValue`.
#[derive(Debug)]
pub enum ValueDeserializeError {
    /// The input type does not match the target type.
    MismatchedType,
    /// An unstructured error message.
    Custom(String),
}

impl de::Error for ValueDeserializeError {
    fn custom(msg: impl std::fmt::Display) -> Self {
        Self::Custom(msg.to_string())
    }
}

/// Turns any error into `ValueDeserializeError::MismatchedType`.
fn map_err<T, E>(res: Result<T, E>) -> Result<T, ValueDeserializeError> {
    res.map_err(|_| ValueDeserializeError::MismatchedType)
}

/// Turns any option into `ValueDeserializeError::MismatchedType`.
fn ok_or<T>(res: Option<T>) -> Result<T, ValueDeserializeError> {
    res.ok_or(ValueDeserializeError::MismatchedType)
}

impl<'de> de::Deserializer<'de> for ValueDeserializer {
    type Error = ValueDeserializeError;

    fn deserialize_product<V: de::ProductVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let vals = map_err(self.val.into_product())?.elements.into_iter();
        visitor.visit_seq_product(ProductAccess { vals })
    }

    fn deserialize_sum<V: de::SumVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let sum = map_err(self.val.into_sum())?;
        visitor.visit_sum(SumAccess { sum })
    }

    fn deserialize_bool(self) -> Result<bool, Self::Error> {
        map_err(self.val.into_bool())
    }

    fn deserialize_u8(self) -> Result<u8, Self::Error> {
        map_err(self.val.into_u8())
    }

    fn deserialize_u16(self) -> Result<u16, Self::Error> {
        map_err(self.val.into_u16())
    }

    fn deserialize_u32(self) -> Result<u32, Self::Error> {
        map_err(self.val.into_u32())
    }

    fn deserialize_u64(self) -> Result<u64, Self::Error> {
        map_err(self.val.into_u64())
    }

    fn deserialize_u128(self) -> Result<u128, Self::Error> {
        map_err(self.val.into_u128())
    }

    fn deserialize_i8(self) -> Result<i8, Self::Error> {
        map_err(self.val.into_i8())
    }

    fn deserialize_i16(self) -> Result<i16, Self::Error> {
        map_err(self.val.into_i16())
    }

    fn deserialize_i32(self) -> Result<i32, Self::Error> {
        map_err(self.val.into_i32())
    }

    fn deserialize_i64(self) -> Result<i64, Self::Error> {
        map_err(self.val.into_i64())
    }

    fn deserialize_i128(self) -> Result<i128, Self::Error> {
        map_err(self.val.into_i128())
    }

    fn deserialize_f32(self) -> Result<f32, Self::Error> {
        map_err(self.val.into_f32().map(f32::from))
    }

    fn deserialize_f64(self) -> Result<f64, Self::Error> {
        map_err(self.val.into_f64().map(f64::from))
    }

    fn deserialize_str<V: de::SliceVisitor<'de, str>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        visitor.visit_owned(map_err(self.val.into_string())?)
    }

    fn deserialize_bytes<V: de::SliceVisitor<'de, [u8]>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        visitor.visit_owned(map_err(self.val.into_bytes())?)
    }

    fn deserialize_array_seed<V: de::ArrayVisitor<'de, T::Output>, T: de::DeserializeSeed<'de> + Clone>(
        self,
        visitor: V,
        seed: T,
    ) -> Result<V::Output, Self::Error> {
        let iter = map_err(self.val.into_array())?.into_iter();
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
        let iter = map_err(self.val.into_map())?.into_iter();
        visitor.visit(MapAccess { iter, kseed, vseed })
    }
}

/// Defines deserialization for [`ValueDeserializer`] where product elements are in the input.
struct ProductAccess {
    /// The element values of the product as an iterator of owned values.
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

/// Defines deserialization for [`ValueDeserializer`] where a sum value is in the input.
#[repr(transparent)]
struct SumAccess {
    /// The input sum value to deserialize.
    sum: SumValue,
}

impl SumAccess {
    /// Converts `&SumValue` to `&SumAccess`.
    fn from_ref(sum: &SumValue) -> &Self {
        // SAFETY: `repr(transparent)` allows this.
        unsafe { &*(sum as *const SumValue as *const SumAccess) }
    }
}

impl de::SumAccess<'_> for SumAccess {
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

/// Defines deserialization for [`ValueDeserializer`] where an array value is in the input.
struct ArrayAccess<T> {
    /// The elements of the array as an iterator of owned elements.
    iter: ArrayValueIntoIter,
    /// A seed value provided by the caller of
    /// [`deserialize_array_seed`](de::Deserializer::deserialize_array_seed).
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

/// Defines deserialization for [`ValueDeserializer`] where a map value is in the input.
struct MapAccess<K, V> {
    /// The elements of the map as an iterator of owned key/value entries.
    iter: std::collections::btree_map::IntoIter<AlgebraicValue, AlgebraicValue>,
    /// A key seed value provided by the caller of
    /// [`deserialize_map_seed`](de::Deserializer::deserialize_map_seed).
    kseed: K,
    /// A value seed value provided by the caller of
    /// [`deserialize_map_seed`](de::Deserializer::deserialize_map_seed).
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
        let vals = ok_or(self.val.as_product())?.elements.iter();
        visitor.visit_seq_product(RefProductAccess { vals })
    }

    fn deserialize_sum<V: de::SumVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let sum = ok_or(self.val.as_sum())?;
        visitor.visit_sum(SumAccess::from_ref(sum))
    }

    fn deserialize_bool(self) -> Result<bool, Self::Error> {
        ok_or(self.val.as_bool().copied())
    }
    fn deserialize_u8(self) -> Result<u8, Self::Error> {
        ok_or(self.val.as_u8().copied())
    }
    fn deserialize_u16(self) -> Result<u16, Self::Error> {
        ok_or(self.val.as_u16().copied())
    }
    fn deserialize_u32(self) -> Result<u32, Self::Error> {
        ok_or(self.val.as_u32().copied())
    }
    fn deserialize_u64(self) -> Result<u64, Self::Error> {
        ok_or(self.val.as_u64().copied())
    }
    fn deserialize_u128(self) -> Result<u128, Self::Error> {
        ok_or(self.val.as_u128().copied())
    }
    fn deserialize_i8(self) -> Result<i8, Self::Error> {
        ok_or(self.val.as_i8().copied())
    }
    fn deserialize_i16(self) -> Result<i16, Self::Error> {
        ok_or(self.val.as_i16().copied())
    }
    fn deserialize_i32(self) -> Result<i32, Self::Error> {
        ok_or(self.val.as_i32().copied())
    }
    fn deserialize_i64(self) -> Result<i64, Self::Error> {
        ok_or(self.val.as_i64().copied())
    }
    fn deserialize_i128(self) -> Result<i128, Self::Error> {
        ok_or(self.val.as_i128().copied())
    }
    fn deserialize_f32(self) -> Result<f32, Self::Error> {
        ok_or(self.val.as_f32().copied().map(f32::from))
    }
    fn deserialize_f64(self) -> Result<f64, Self::Error> {
        ok_or(self.val.as_f64().copied().map(f64::from))
    }

    fn deserialize_str<V: de::SliceVisitor<'de, str>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        visitor.visit_borrowed(ok_or(self.val.as_string())?)
    }

    fn deserialize_bytes<V: de::SliceVisitor<'de, [u8]>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        visitor.visit_borrowed(ok_or(self.val.as_bytes())?)
    }

    fn deserialize_array_seed<V: de::ArrayVisitor<'de, T::Output>, T: de::DeserializeSeed<'de> + Clone>(
        self,
        visitor: V,
        seed: T,
    ) -> Result<V::Output, Self::Error> {
        let iter = ok_or(self.val.as_array())?.iter_cloned();
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
        let iter = ok_or(self.val.as_map())?.iter();
        visitor.visit(RefMapAccess { iter, kseed, vseed })
    }
}

/// Defines deserialization for [`&'de ValueDeserializer`] where product elements are in the input.
struct RefProductAccess<'a> {
    /// The element values of the product as an iterator of borrowed values.
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

/// Defines deserialization for [`&'de ValueDeserializer`] where an array value is in the input.
struct RefArrayAccess<'a, T> {
    // TODO: idk this kinda sucks
    /// The elements of the array as an iterator of cloned elements.
    iter: ArrayValueIterCloned<'a>,
    /// A seed value provided by the caller of
    /// [`deserialize_array_seed`](de::Deserializer::deserialize_array_seed).
    seed: T,
}

impl<'de, T: de::DeserializeSeed<'de> + Clone> de::ArrayAccess<'de> for RefArrayAccess<'de, T> {
    type Element = T::Output;
    type Error = ValueDeserializeError;

    fn next_element(&mut self) -> Result<Option<Self::Element>, Self::Error> {
        self.iter
            .next()
            .map(|val| self.seed.clone().deserialize(ValueDeserializer { val }))
            .transpose()
    }
}

/// Defines deserialization for [`&'de ValueDeserializer`] where an map value is in the input.
struct RefMapAccess<'a, K, V> {
    /// The elements of the map as an iterator of borrowed key/value entries.
    iter: std::collections::btree_map::Iter<'a, AlgebraicValue, AlgebraicValue>,
    /// A key seed value provided by the caller of
    /// [`deserialize_map_seed`](de::Deserializer::deserialize_map_seed).
    kseed: K,
    /// A value seed value provided by the caller of
    /// [`deserialize_map_seed`](de::Deserializer::deserialize_map_seed).
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
