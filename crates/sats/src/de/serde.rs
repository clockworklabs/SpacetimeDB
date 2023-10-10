use std::fmt;
use std::marker::PhantomData;

use super::Deserializer;
use ::serde::de as serde;

/// Converts any [`serde::Deserializer`] to a SATS [`Deserializer`]
/// so that Serde's data formats can be reused.
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

/// An error that occured when deserializing SATS to a Serde data format.
#[repr(transparent)]
pub struct SerdeError<E>(pub E);
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
        self.de
            .deserialize_struct("", &[], TupleVisitor { visitor })
            .map_err(SerdeError)
    }

    fn deserialize_sum<V: super::SumVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        if visitor.is_option() && self.de.is_human_readable() {
            self.de.deserialize_any(OptionVisitor { visitor }).map_err(SerdeError)
        } else {
            self.de
                .deserialize_enum("", &[], EnumVisitor { visitor })
                .map_err(SerdeError)
        }
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
            self.de.deserialize_any(BytesVisitor { visitor }).map_err(SerdeError)
        } else {
            self.de.deserialize_bytes(BytesVisitor { visitor }).map_err(SerdeError)
        }
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

    fn deserialize_map_seed<
        Vi: super::MapVisitor<'de, K::Output, V::Output>,
        K: super::DeserializeSeed<'de> + Clone,
        V: super::DeserializeSeed<'de> + Clone,
    >(
        self,
        visitor: Vi,
        kseed: K,
        vseed: V,
    ) -> Result<Vi::Output, Self::Error> {
        self.de
            .deserialize_map(MapVisitor { visitor, kseed, vseed })
            .map_err(SerdeError)
    }
}

/// Converts `DeserializeSeed<'de>` in SATS to the one in Serde.
#[repr(transparent)]
pub struct SeedWrapper<T: ?Sized>(pub T);

impl<T: ?Sized> SeedWrapper<T> {
    /// Convert `&T` to `&SeedWrapper<T>`.
    pub fn from_ref(t: &T) -> &Self {
        // SAFETY: `repr(transparent)` allows this.
        unsafe { &*(t as *const T as *const SeedWrapper<T>) }
    }
}

impl<'de, T: super::DeserializeSeed<'de>> serde::DeserializeSeed<'de> for SeedWrapper<T> {
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

/// Converts a `SumVisitor` into a `serde::Visitor` for deserializing option.
struct OptionVisitor<V> {
    /// The visitor to convert.
    visitor: V,
}

impl<'de, V: super::SumVisitor<'de>> serde::Visitor<'de> for OptionVisitor<V> {
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("option")
    }

    fn visit_map<A: serde::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        self.visitor.visit_sum(SomeAccess(map)).map_err(unwrap_error)
    }

    fn visit_unit<E: serde::Error>(self) -> Result<Self::Value, E> {
        self.visitor.visit_sum(NoneAccess(PhantomData)).map_err(unwrap_error)
    }
}

/// Deserializes `some` variant of an optional value.
/// Converts Serde's map deserialization to SATS.
struct SomeAccess<A>(A);

impl<'de, A: serde::MapAccess<'de>> super::SumAccess<'de> for SomeAccess<A> {
    type Error = SerdeError<A::Error>;
    type Variant = Self;

    fn variant<V: super::VariantVisitor>(mut self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error> {
        self.0
            .next_key_seed(VariantVisitor { visitor })
            .and_then(|x| match x {
                Some(x) => Ok((x, self)),
                None => Err(serde::Error::custom("expected variant name")),
            })
            .map_err(SerdeError)
    }
}
impl<'de, A: serde::MapAccess<'de>> super::VariantAccess<'de> for SomeAccess<A> {
    type Error = SerdeError<A::Error>;

    fn deserialize_seed<T: super::DeserializeSeed<'de>>(mut self, seed: T) -> Result<T::Output, Self::Error> {
        let ret = self.0.next_value_seed(SeedWrapper(seed)).map_err(SerdeError)?;
        self.0.next_key_seed(NothingVisitor).map_err(SerdeError)?;
        Ok(ret)
    }
}

/// Deserializes nothing, producing `!` effectively.
struct NothingVisitor;
impl<'de> serde::DeserializeSeed<'de> for NothingVisitor {
    type Value = std::convert::Infallible;
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_identifier(self)
    }
}
impl serde::Visitor<'_> for NothingVisitor {
    type Value = std::convert::Infallible;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("nothing")
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
        f.write_str("enum")
    }

    fn visit_enum<A: serde::EnumAccess<'de>>(self, access: A) -> Result<Self::Value, A::Error> {
        self.visitor.visit_sum(EnumAccess { access }).map_err(unwrap_error)
    }
}

/// Converts Serde's `EnumAccess` to SATS `SumAccess`.
struct EnumAccess<A> {
    /// The Serde `EnumAccess`.
    access: A,
}

impl<'de, A: serde::EnumAccess<'de>> super::SumAccess<'de> for EnumAccess<A> {
    type Error = SerdeError<A::Error>;
    type Variant = VariantAccess<A::Variant>;

    fn variant<V: super::VariantVisitor>(self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error> {
        self.access
            .variant_seed(VariantVisitor { visitor })
            .map(|(variant, access)| (variant, VariantAccess { access }))
            .map_err(SerdeError)
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
        deserializer.deserialize_identifier(self)
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

/// Deserializes the data of a variant using Serde's `serde::VariantAccess` translating this to SATS.
struct VariantAccess<A> {
    // Implements `serde::VariantAccess`.
    access: A,
}

impl<'de, A: serde::VariantAccess<'de>> super::VariantAccess<'de> for VariantAccess<A> {
    type Error = SerdeError<A::Error>;

    fn deserialize_seed<T: super::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Output, Self::Error> {
        self.access.newtype_variant_seed(SeedWrapper(seed)).map_err(SerdeError)
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
struct BytesVisitor<V> {
    /// The `SliceVisitor<'de, [u8]>`.
    visitor: V,
}

impl<'de, V: super::SliceVisitor<'de, [u8]>> serde::Visitor<'de> for BytesVisitor<V> {
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a byte array")
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

/// Translates SATS's `MapVisior<'de>` (the trait) to `serde::Visitor<'de>`
/// for implementing deserialization of maps.
struct MapVisitor<Vi, K, V> {
    /// The SATS visitor to translate to a Serde visitor.
    visitor: Vi,
    /// The seed value to provide to `DeserializeSeed` for deserializing keys.
    /// As this is reused for every entry element, it will be `.cloned()`.
    kseed: K,
    /// The seed value to provide to `DeserializeSeed` for deserializing values.
    /// As this is reused for every entry element, it will be `.cloned()`.
    vseed: V,
}

impl<
        'de,
        K: super::DeserializeSeed<'de> + Clone,
        V: super::DeserializeSeed<'de> + Clone,
        Vi: super::MapVisitor<'de, K::Output, V::Output>,
    > serde::Visitor<'de> for MapVisitor<Vi, K, V>
{
    type Value = Vi::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a vec")
    }

    fn visit_map<A: serde::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        self.visitor
            .visit(MapAccess {
                map,
                kseed: self.kseed,
                vseed: self.vseed,
            })
            .map_err(unwrap_error)
    }
}

struct MapAccess<A, K, V> {
    /// An implementation of `serde::MapAccess<'de>`.
    map: A,
    /// The seed value to provide to `DeserializeSeed` for deserializing keys.
    /// As this is reused for every entry element, it will be `.cloned()`.
    kseed: K,
    /// The seed value to provide to `DeserializeSeed` for deserializing values.
    /// As this is reused for every entry element, it will be `.cloned()`.
    vseed: V,
}

impl<'de, A: serde::MapAccess<'de>, K: super::DeserializeSeed<'de> + Clone, V: super::DeserializeSeed<'de> + Clone>
    super::MapAccess<'de> for MapAccess<A, K, V>
{
    type Key = K::Output;
    type Value = V::Output;
    type Error = SerdeError<A::Error>;

    fn next_entry(&mut self) -> Result<Option<(Self::Key, Self::Value)>, Self::Error> {
        self.map
            .next_entry_seed(SeedWrapper(self.kseed.clone()), SeedWrapper(self.vseed.clone()))
            .map_err(SerdeError)
    }

    fn size_hint(&self) -> Option<usize> {
        self.map.size_hint()
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

/// Turns a type deserializable in SATS into one deserializiable in Serde.
///
/// That is, `T: sats::Deserialize<'de> => DeserializeWrapper<T>: serde::Deserialize`.
pub struct DeserializeWrapper<T>(pub T);
impl<'de, T: super::Deserialize<'de>> serde::Deserialize<'de> for DeserializeWrapper<T> {
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
