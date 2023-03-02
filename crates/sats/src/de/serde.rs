use std::fmt;
use std::marker::PhantomData;

use super::Deserializer;
use ::serde::de as serde;

pub struct SerdeDeserializer<D> {
    de: D,
}

impl<D> SerdeDeserializer<D> {
    pub fn new(de: D) -> Self {
        Self { de }
    }
}

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

impl<'de, D: serde::Deserializer<'de>> Deserializer<'de> for SerdeDeserializer<D> {
    type Error = SerdeError<D::Error>;

    fn deserialize_product<V: super::ProductVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        self.de
            .deserialize_struct("", &[], TupleVisitor { visitor })
            .map_err(SerdeError)
    }

    fn deserialize_sum<V: super::SumVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        self.de
            .deserialize_enum("", &[], EnumVisitor { visitor })
            .map_err(SerdeError)
    }

    fn deserialize_bool(self) -> Result<bool, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
    }
    fn deserialize_u8(self) -> Result<u8, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
    }
    fn deserialize_u16(self) -> Result<u16, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
    }
    fn deserialize_u32(self) -> Result<u32, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
    }
    fn deserialize_u64(self) -> Result<u64, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
    }
    fn deserialize_u128(self) -> Result<u128, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
    }
    fn deserialize_i8(self) -> Result<i8, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
    }
    fn deserialize_i16(self) -> Result<i16, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
    }
    fn deserialize_i32(self) -> Result<i32, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
    }
    fn deserialize_i64(self) -> Result<i64, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
    }
    fn deserialize_i128(self) -> Result<i128, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
    }
    fn deserialize_f32(self) -> Result<f32, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
    }
    fn deserialize_f64(self) -> Result<f64, Self::Error> {
        serde::Deserialize::deserialize(self.de).map_err(SerdeError)
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

#[repr(transparent)]
pub struct SeedWrapper<T: ?Sized>(pub T);
impl<T: ?Sized> SeedWrapper<T> {
    pub fn from_ref(t: &T) -> &Self {
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

struct TupleVisitor<V> {
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

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::MapAccess<'de>,
    {
        self.visitor
            .visit_named_product(NamedTupleAccess { map })
            .map_err(unwrap_error)
    }

    fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::SeqAccess<'de>,
    {
        self.visitor
            .visit_seq_product(SeqTupleAccess { seq })
            .map_err(unwrap_error)
    }
}

struct NullProduct<E>(PhantomData<E>);
impl<'de, E: super::Error> super::SeqProductAccess<'de> for NullProduct<E> {
    type Error = E;
    fn next_element_seed<T: super::DeserializeSeed<'de>>(&mut self, _: T) -> Result<Option<T::Output>, Self::Error> {
        Ok(None)
    }
}

struct NamedTupleAccess<A> {
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

struct FieldNameVisitor<V> {
    visitor: V,
}

impl<'de, V: super::FieldNameVisitor<'de>> serde::DeserializeSeed<'de> for FieldNameVisitor<V> {
    type Value = V::Output;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
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

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::Error,
    {
        self.visitor.visit(v).map_err(unwrap_error)
    }
}

struct SeqTupleAccess<A> {
    seq: A,
}

impl<'de, A: serde::SeqAccess<'de>> super::SeqProductAccess<'de> for SeqTupleAccess<A> {
    type Error = SerdeError<A::Error>;

    fn next_element_seed<T: super::DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Output>, Self::Error> {
        let res = self.seq.next_element_seed(SeedWrapper(seed)).map_err(SerdeError)?;
        Ok(res)
    }
}

struct EnumVisitor<V> {
    visitor: V,
}

impl<'de, V: super::SumVisitor<'de>> serde::Visitor<'de> for EnumVisitor<V> {
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("enum")
    }

    fn visit_enum<A>(self, access: A) -> Result<Self::Value, A::Error>
    where
        A: serde::EnumAccess<'de>,
    {
        self.visitor.visit_sum(EnumAccess { access }).map_err(unwrap_error)
    }
}

struct EnumAccess<A> {
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

struct VariantVisitor<V> {
    visitor: V,
}
impl<'de, V: super::VariantVisitor> serde::DeserializeSeed<'de> for VariantVisitor<V> {
    type Value = V::Output;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_identifier(self)
    }
}
impl<'de, V: super::VariantVisitor> serde::Visitor<'de> for VariantVisitor<V> {
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("variant identifier (string or int)")
    }

    fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
    where
        E: serde::Error,
    {
        self.visitor.visit_tag(v).map_err(unwrap_error)
    }
    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: serde::Error,
    {
        let v: u8 = v
            .try_into()
            .map_err(|_| E::invalid_value(serde::Unexpected::Unsigned(v), &"a u8 tag"))?;
        self.visit_u8(v)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::Error,
    {
        self.visitor.visit_name(v).map_err(unwrap_error)
    }
}

struct VariantAccess<A> {
    access: A,
}

impl<'de, A: serde::VariantAccess<'de>> super::VariantAccess<'de> for VariantAccess<A> {
    type Error = SerdeError<A::Error>;

    fn deserialize_seed<T: super::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Output, Self::Error> {
        self.access.newtype_variant_seed(SeedWrapper(seed)).map_err(SerdeError)
    }
}

struct StrVisitor<V> {
    visitor: V,
}

impl<'de, V: super::SliceVisitor<'de, str>> serde::Visitor<'de> for StrVisitor<V> {
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::Error,
    {
        self.visitor.visit(v).map_err(unwrap_error)
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: serde::Error,
    {
        self.visitor.visit_borrowed(v).map_err(unwrap_error)
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::Error,
    {
        self.visitor.visit_owned(v).map_err(unwrap_error)
    }
}

struct BytesVisitor<V> {
    visitor: V,
}

impl<'de, V: super::SliceVisitor<'de, [u8]>> serde::Visitor<'de> for BytesVisitor<V> {
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a byte array")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::Error,
    {
        self.visitor.visit(v).map_err(unwrap_error)
    }

    fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
    where
        E: serde::Error,
    {
        self.visitor.visit_borrowed(v).map_err(unwrap_error)
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::Error,
    {
        self.visitor.visit_owned(v).map_err(unwrap_error)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::Error,
    {
        let data = hex_string(v, &self)?;
        self.visitor.visit_owned(data).map_err(unwrap_error)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::SeqAccess<'de>,
    {
        let mut v = Vec::with_capacity(std::cmp::min(seq.size_hint().unwrap_or(0), 4096));
        while let Some(val) = seq.next_element()? {
            v.push(val);
        }
        self.visitor.visit_owned(v).map_err(unwrap_error)
    }
}

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

struct ArrayVisitor<V, T> {
    visitor: V,
    seed: T,
}

impl<'de, T: super::DeserializeSeed<'de> + Clone, V: super::ArrayVisitor<'de, T::Output>> serde::Visitor<'de>
    for ArrayVisitor<V, T>
{
    type Value = V::Output;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a vec")
    }

    fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::SeqAccess<'de>,
    {
        self.visitor
            .visit(ArrayAccess { seq, seed: self.seed })
            .map_err(unwrap_error)
    }
}

struct ArrayAccess<A, T> {
    seq: A,
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

struct MapVisitor<Vi, K, V> {
    visitor: Vi,
    kseed: K,
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

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::MapAccess<'de>,
    {
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
    map: A,
    kseed: K,
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

pub fn deserialize_from<'de, T: super::Deserialize<'de>, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    T::deserialize(SerdeDeserializer::new(deserializer)).map_err(unwrap_error)
}

pub struct DeserializeWrapper<T>(pub T);
impl<'de, T> serde::Deserialize<'de> for DeserializeWrapper<T>
where
    T: super::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_from(deserializer).map(Self)
    }
}

macro_rules! delegate_serde {
    ($($t:ty),*) => {
        $(impl<'de> serde::Deserialize<'de> for $t {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserialize_from(deserializer)
            }
        })*
    };
}

delegate_serde!(crate::AlgebraicType, crate::ProductType, crate::SumType);
