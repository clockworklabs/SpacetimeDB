use super::Deserializer;
use crate::serde::{SerdeError, SerdeWrapper};
use crate::{i256, u256};
use core::fmt;
use core::marker::PhantomData;
use serde::de as serde;

/// Converts any [`serde::Deserializer`] to a SATS [`Deserializer`]
/// so that Serde's data formats can be reused.
///
/// In order for successful round-trip deserialization, the `serde::Deserializer`
/// that this type wraps must support `deserialize_any()`.
pub struct SerdeDeserializer<D> {
    /// A deserialization data format in Serde.
    de: D,
}

impl<D> SerdeDeserializer<D> {
    /// Wraps a Serde deserializer.
    pub fn new(de: D) -> Self {
        Self { de }
    }
}

#[inline]
fn unwrap_error<E>(err: SerdeError<E>) -> E {
    let SerdeError(err) = err;
    err
}

impl<E: serde::Error> super::Error for SerdeError<E> {
    fn custom(msg: impl fmt::Display) -> Self {
        SerdeError(E::custom(msg))
    }

    fn invalid_product_length<'de, T: super::ProductVisitor<'de>>(len: usize, expected: &T) -> Self {
        SerdeError(E::invalid_length(len, &super::fmt_invalid_len(expected)))
    }
}

/// Deserialize a `T` provided a serde deserializer `D`.
fn deserialize<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(de: D) -> Result<T, SerdeError<D::Error>> {
    serde::Deserialize::deserialize(de).map_err(SerdeError)
}

impl<'de, D: serde::Deserializer<'de>> Deserializer<'de> for SerdeDeserializer<D> {
    type Error = SerdeError<D::Error>;

    fn deserialize_product<V: super::ProductVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        self.de.deserialize_any(TupleVisitor { visitor }).map_err(SerdeError)
    }

    fn deserialize_sum<V: super::SumVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        self.de.deserialize_any(EnumVisitor { visitor }).map_err(SerdeError)
    }

    fn deserialize_bool(self) -> Result<bool, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_u8(self) -> Result<u8, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_u16(self) -> Result<u16, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_u32(self) -> Result<u32, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_u64(self) -> Result<u64, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_u128(self) -> Result<u128, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_u256(self) -> Result<u256, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_i8(self) -> Result<i8, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_i16(self) -> Result<i16, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_i32(self) -> Result<i32, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_i64(self) -> Result<i64, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_i128(self) -> Result<i128, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_i256(self) -> Result<i256, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_f32(self) -> Result<f32, Self::Error> {
        deserialize(self.de)
    }
    fn deserialize_f64(self) -> Result<f64, Self::Error> {
        deserialize(self.de)
    }

    fn deserialize_str<V: super::SliceVisitor<'de, str>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        self.de.deserialize_str(StrVisitor { visitor }).map_err(SerdeError)
    }

    fn deserialize_bytes<V: super::SliceVisitor<'de, [u8]>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        if self.de.is_human_readable() {
            self.de.deserialize_any(BytesVisitor::<_, true> { visitor })
        } else {
            self.de.deserialize_bytes(BytesVisitor::<_, false> { visitor })
        }
        .map_err(SerdeError)
    }

    fn deserialize_array_seed<V: super::ArrayVisitor<'de, T::Output>, T: super::DeserializeSeed<'de> + Clone>(
        self,
        visitor: V,
        seed: T,
    ) -> Result<V::Output, Self::Error> {
        self.de
            .deserialize_seq(ArrayVisitor { visitor, seed })
            .map_err(SerdeError)
    }
}

pub use crate::serde::SerdeWrapper as SeedWrapper;

impl<'de, T: super::DeserializeSeed<'de>> serde::DeserializeSeed<'de> for SerdeWrapper<T> {
    type Value = T::Output;

    fn deserialize<D>(self, de: D) -> Result<Self::Value, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        self.0.deserialize(SerdeDeserializer { de }).map_err(unwrap_error)
    }
}

/// Converts a `ProductVisitor` to a `serde::Visitor`.
struct TupleVisitor<V> {
    /// The `ProductVisitor` to convert.
    visitor: V,
}

impl<'de, V: super::ProductVisitor<'de>> serde::Visitor<'de> for TupleVisitor<V> {
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(name) = self.visitor.product_name() {
            write!(f, "a {name} tuple")
        } else {
            write!(f, "a {}-element tuple", self.visitor.product_len())
        }
    }

    fn visit_map<A: serde::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        self.visitor
            .visit_named_product(NamedTupleAccess { map })
            .map_err(unwrap_error)
    }

    fn visit_seq<A: serde::SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        self.visitor
            .visit_seq_product(SeqTupleAccess { seq })
            .map_err(unwrap_error)
    }
}

/// Turns Serde's style of deserializing map entries
/// into deserializing field names and their values.
struct NamedTupleAccess<A> {
    /// An implementation of `serde::MapAccess<'de>` to convert.
    map: A,
}

impl<'de, A: serde::MapAccess<'de>> super::NamedProductAccess<'de> for NamedTupleAccess<A> {
    type Error = SerdeError<A::Error>;

    fn get_field_ident<V: super::FieldNameVisitor<'de>>(
        &mut self,
        visitor: V,
    ) -> Result<Option<V::Output>, Self::Error> {
        self.map.next_key_seed(FieldNameVisitor { visitor }).map_err(SerdeError)
    }

    fn get_field_value_seed<T: super::DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Output, Self::Error> {
        self.map.next_value_seed(SeedWrapper(seed)).map_err(SerdeError)
    }
}

/// Converts a SATS field name visitor for use in [`NamedTupleAccess`].
struct FieldNameVisitor<V> {
    /// The underlying field name visitor.
    visitor: V,
}

impl<'de, V: super::FieldNameVisitor<'de>> serde::DeserializeSeed<'de> for FieldNameVisitor<V> {
    type Value = V::Output;

    fn deserialize<D: ::serde::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_str(self)
    }
}

impl<'de, V: super::FieldNameVisitor<'de>> serde::Visitor<'de> for FieldNameVisitor<V> {
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(one_of) = super::one_of_names(|n| self.visitor.field_names(n)) {
            write!(f, "a tuple field ({one_of})")
        } else {
            f.write_str("a tuple field, but there are no fields")
        }
    }

    fn visit_str<E: serde::Error>(self, v: &str) -> Result<Self::Value, E> {
        self.visitor.visit(v).map_err(unwrap_error)
    }
}

/// Turns `serde::SeqAccess` deserializing the elements of a sequence
/// into `SeqProductAccess`.
struct SeqTupleAccess<A> {
    /// The `serde::SeqAccess` to convert.
    seq: A,
}

impl<'de, A: serde::SeqAccess<'de>> super::SeqProductAccess<'de> for SeqTupleAccess<A> {
    type Error = SerdeError<A::Error>;

    fn next_element_seed<T: super::DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Output>, Self::Error> {
        let res = self.seq.next_element_seed(SeedWrapper(seed)).map_err(SerdeError)?;
        Ok(res)
    }
}

/// Deserializes `none` variant of an optional value.
struct NoneAccess<E>(PhantomData<E>);
impl<E: super::Error> super::SumAccess<'_> for NoneAccess<E> {
    type Error = E;
    type Variant = Self;

    fn variant<V: super::VariantVisitor>(self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error> {
        visitor.visit_name("none").map(|var| (var, self))
    }
}
impl<'de, E: super::Error> super::VariantAccess<'de> for NoneAccess<E> {
    type Error = E;
    fn deserialize_seed<T: super::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Output, Self::Error> {
        use crate::algebraic_value::de::*;
        seed.deserialize(ValueDeserializer::new(crate::AlgebraicValue::unit()))
            .map_err(|err| match err {
                ValueDeserializeError::MismatchedType => E::custom("mismatched type"),
                ValueDeserializeError::Custom(err) => E::custom(err),
            })
    }
}

/// Converts a SATS `SumVisitor` to `serde::Visitor`.
struct EnumVisitor<V> {
    /// The `SumVisitor`.
    visitor: V,
}

impl<'de, V: super::SumVisitor<'de>> serde::Visitor<'de> for EnumVisitor<V> {
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.visitor.sum_name() {
            Some(name) => write!(f, "sum type {name}"),
            None => f.write_str("sum type"),
        }
    }

    fn visit_map<A>(self, access: A) -> Result<Self::Value, A::Error>
    where
        A: serde::MapAccess<'de>,
    {
        self.visitor.visit_sum(EnumAccess { access }).map_err(unwrap_error)
    }

    fn visit_seq<A>(self, access: A) -> Result<Self::Value, A::Error>
    where
        A: serde::SeqAccess<'de>,
    {
        self.visitor.visit_sum(SeqEnumAccess { access }).map_err(unwrap_error)
    }

    fn visit_unit<E: serde::Error>(self) -> Result<Self::Value, E> {
        if self.visitor.is_option() {
            self.visitor.visit_sum(NoneAccess(PhantomData)).map_err(unwrap_error)
        } else {
            Err(E::invalid_type(serde::Unexpected::Unit, &self))
        }
    }
}

/// Converts SATS way of identifying a variant to Serde's way.
struct VariantVisitor<V> {
    /// The SATS `VariantVisitor` to convert.
    visitor: V,
}

impl<'de, V: super::VariantVisitor> serde::DeserializeSeed<'de> for VariantVisitor<V> {
    type Value = V::Output;

    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<V: super::VariantVisitor> serde::Visitor<'_> for VariantVisitor<V> {
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("variant identifier (string or int)")
    }

    fn visit_u8<E: serde::Error>(self, v: u8) -> Result<Self::Value, E> {
        self.visitor.visit_tag(v).map_err(unwrap_error)
    }
    fn visit_u64<E: serde::Error>(self, v: u64) -> Result<Self::Value, E> {
        let v: u8 = v
            .try_into()
            .map_err(|_| E::invalid_value(serde::Unexpected::Unsigned(v), &"a u8 tag"))?;
        self.visit_u8(v)
    }

    fn visit_str<E: serde::Error>(self, v: &str) -> Result<Self::Value, E> {
        if let Ok(tag) = v.parse::<u8>() {
            self.visit_u8(tag)
        } else {
            self.visitor.visit_name(v).map_err(unwrap_error)
        }
    }
}

/// Converts Serde's `EnumAccess` to SATS `SumAccess`.
struct EnumAccess<A> {
    /// The Serde `EnumAccess`.
    access: A,
}

impl<'de, A: serde::MapAccess<'de>> super::SumAccess<'de> for EnumAccess<A> {
    type Error = SerdeError<A::Error>;
    type Variant = Self;

    fn variant<V: super::VariantVisitor>(mut self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error> {
        let errmsg = "expected map representing sum type to have exactly one field";
        let key = self
            .access
            .next_key_seed(VariantVisitor { visitor })
            .map_err(SerdeError)?
            .ok_or_else(|| SerdeError(serde::Error::custom(errmsg)))?;
        Ok((key, self))
    }
}

impl<'de, A: serde::MapAccess<'de>> super::VariantAccess<'de> for EnumAccess<A> {
    type Error = SerdeError<A::Error>;

    fn deserialize_seed<T: super::DeserializeSeed<'de>>(mut self, seed: T) -> Result<T::Output, Self::Error> {
        self.access.next_value_seed(SeedWrapper(seed)).map_err(SerdeError)
    }
}

struct SeqEnumAccess<A> {
    access: A,
}

const SEQ_ENUM_ERR: &str = "expected seq representing sum type to have exactly two fields";
impl<'de, A: serde::SeqAccess<'de>> super::SumAccess<'de> for SeqEnumAccess<A> {
    type Error = SerdeError<A::Error>;
    type Variant = Self;

    fn variant<V: super::VariantVisitor>(mut self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error> {
        let key = self
            .access
            .next_element_seed(VariantVisitor { visitor })
            .map_err(SerdeError)?
            .ok_or_else(|| SerdeError(serde::Error::custom(SEQ_ENUM_ERR)))?;
        Ok((key, self))
    }
}

impl<'de, A: serde::SeqAccess<'de>> super::VariantAccess<'de> for SeqEnumAccess<A> {
    type Error = SerdeError<A::Error>;

    fn deserialize_seed<T: super::DeserializeSeed<'de>>(mut self, seed: T) -> Result<T::Output, Self::Error> {
        self.access
            .next_element_seed(SeedWrapper(seed))
            .map_err(SerdeError)?
            .ok_or_else(|| SerdeError(serde::Error::custom(SEQ_ENUM_ERR)))
    }
}

/// Translates a `SliceVisitor<'de, str>` to `serde::Visitor<'de>`
/// for implementing `deserialize_str`.
struct StrVisitor<V> {
    /// The `SliceVisitor<'de, str>`.
    visitor: V,
}

impl<'de, V: super::SliceVisitor<'de, str>> serde::Visitor<'de> for StrVisitor<V> {
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a string")
    }

    fn visit_str<E: serde::Error>(self, v: &str) -> Result<Self::Value, E> {
        self.visitor.visit(v).map_err(unwrap_error)
    }

    fn visit_borrowed_str<E: serde::Error>(self, v: &'de str) -> Result<Self::Value, E> {
        self.visitor.visit_borrowed(v).map_err(unwrap_error)
    }

    fn visit_string<E: serde::Error>(self, v: String) -> Result<Self::Value, E> {
        self.visitor.visit_owned(v).map_err(unwrap_error)
    }
}

/// Translates a `SliceVisitor<'de, str>` to `serde::Visitor<'de>`
/// for implementing `deserialize_bytes`.
struct BytesVisitor<V, const HUMAN_READABLE: bool> {
    /// The `SliceVisitor<'de, [u8]>`.
    visitor: V,
}

impl<'de, V: super::SliceVisitor<'de, [u8]>, const HUMAN_READABLE: bool> serde::Visitor<'de>
    for BytesVisitor<V, HUMAN_READABLE>
{
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(if HUMAN_READABLE {
            "a byte array or hex string"
        } else {
            "a byte array"
        })
    }

    fn visit_bytes<E: serde::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        self.visitor.visit(v).map_err(unwrap_error)
    }

    fn visit_borrowed_bytes<E: serde::Error>(self, v: &'de [u8]) -> Result<Self::Value, E> {
        self.visitor.visit_borrowed(v).map_err(unwrap_error)
    }

    fn visit_byte_buf<E: serde::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
        self.visitor.visit_owned(v).map_err(unwrap_error)
    }

    fn visit_str<E: serde::Error>(self, v: &str) -> Result<Self::Value, E> {
        let data = hex_string(v, &self)?;
        self.visitor.visit_owned(data).map_err(unwrap_error)
    }

    fn visit_seq<A: serde::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut v = Vec::with_capacity(std::cmp::min(seq.size_hint().unwrap_or(0), 4096));
        while let Some(val) = seq.next_element()? {
            v.push(val);
        }
        self.visitor.visit_owned(v).map_err(unwrap_error)
    }
}

/// Hex decodes the string `v`.
fn hex_string<T: hex::FromHex<Error = hex::FromHexError>, E: serde::Error>(
    v: &str,
    exp: &dyn serde::Expected,
) -> Result<T, E> {
    T::from_hex(v).map_err(|_| serde::Error::invalid_value(serde::Unexpected::Str(v), exp))
}

// struct HashVisitor;

// impl<'de> serde::Visitor<'de> for HashVisitor {
//     type Value = crate::Hash;

//     fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         f.write_str("a hex string representing a 32-byte hash")
//     }

//     fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
//     where
//         E: serde::Error,
//     {
//         let data = hex_string(v, &self)?;
//         Ok(crate::Hash { data })
//     }

//     fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
//     where
//         E: serde::Error,
//     {
//         let data = v
//             .try_into()
//             .map_err(|_| serde::Error::invalid_value(serde::Unexpected::Bytes(v), &"a 32-byte hash"))?;
//         Ok(crate::Hash { data })
//     }
// }

/// Translates `ArrayVisitor<'de, T::Output>` (the trait) to `serde::Visitor<'de>`
/// for implementing `deserialize_array`.
struct ArrayVisitor<V, T> {
    /// The SATS visitor to translate to a Serde visitor.
    visitor: V,
    /// The seed value to provide to `DeserializeSeed`.
    seed: T,
}

impl<'de, T: super::DeserializeSeed<'de> + Clone, V: super::ArrayVisitor<'de, T::Output>> serde::Visitor<'de>
    for ArrayVisitor<V, T>
{
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a vec")
    }

    fn visit_seq<A: serde::SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        self.visitor
            .visit(ArrayAccess { seq, seed: self.seed })
            .map_err(unwrap_error)
    }
}

/// Translates `serde::SeqAcess<'de>` (the trait) to `ArrayAccess<'de>`
/// for implementing deserialization of array elements.
struct ArrayAccess<A, T> {
    /// The `serde::SeqAcess<'de>` implementation.
    seq: A,
    /// The seed to pass onto `DeserializeSeed`.
    seed: T,
}

impl<'de, A: serde::SeqAccess<'de>, T: super::DeserializeSeed<'de> + Clone> super::ArrayAccess<'de>
    for ArrayAccess<A, T>
{
    type Element = T::Output;
    type Error = SerdeError<A::Error>;

    fn next_element(&mut self) -> Result<Option<T::Output>, Self::Error> {
        self.seq
            .next_element_seed(SeedWrapper(self.seed.clone()))
            .map_err(SerdeError)
    }

    fn size_hint(&self) -> Option<usize> {
        self.seq.size_hint()
    }
}

impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> serde::Expected for super::FDisplay<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.0)(f)
    }
}

/// Deserializes `T` as a SATS object from `deserializer: D`
/// where `D` is a serde data format.
pub fn deserialize_from<'de, T: super::Deserialize<'de>, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    T::deserialize(SerdeDeserializer::new(deserializer)).map_err(unwrap_error)
}

pub use crate::serde::SerdeWrapper as DeserializeWrapper;

impl<'de, T: super::Deserialize<'de>> serde::Deserialize<'de> for SerdeWrapper<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_from(deserializer).map(Self)
    }
}

macro_rules! delegate_serde {
    ($($t:ty),*) => {
        $(impl<'de> serde::Deserialize<'de> for $t {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                deserialize_from(deserializer)
            }
        })*
    };
}

delegate_serde!(crate::AlgebraicType, crate::ProductType, crate::SumType);
