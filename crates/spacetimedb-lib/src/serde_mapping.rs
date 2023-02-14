//! DeserializeSeed implementations for converting from serde to TypeValue.
//!
//! Note that the diagnostics for these assume specifically json, so if we
//! start using this for other formats they should be tweaked

use std::fmt;

use itertools::Itertools;
use serde::de::{self, DeserializeSeed, VariantAccess};
use serde::ser::{Serialize, SerializeMap, SerializeSeq};

use crate::type_value::{ElementValue, EnumValue};
use crate::{fmt_fn, ElementDef, EnumDef, Hash, PrimitiveType, ReducerDef, TupleDef, TupleValue, TypeDef, TypeValue};

impl<'de> DeserializeSeed<'de> for &TypeDef {
    type Value = TypeValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match self {
            TypeDef::Primitive(p) => p.deserialize(deserializer),
            // THIS (deserialize_enum with empty strings) ONLY WORKS BECAUSE IT'S JSON
            TypeDef::Enum(enu) => enu.deserialize(deserializer).map(TypeValue::Enum),
            TypeDef::Tuple(tup) => tup.deserialize(deserializer).map(TypeValue::Tuple),
            TypeDef::Vec { element_type } => deserializer
                .deserialize_seq(VecVisitor { element_type })
                .map(TypeValue::Vec),
        }
    }
}

impl<'de> DeserializeSeed<'de> for PrimitiveType {
    type Value = TypeValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use de::Deserialize;
        macro_rules! de_prim {
            ($var:ident) => {
                Ok(TypeValue::$var(de::Deserialize::deserialize(deserializer)?))
            };
        }
        match self {
            PrimitiveType::Unit => {
                de::Deserialize::deserialize(deserializer)?;
                Ok(TypeValue::Unit)
            }
            PrimitiveType::Bool => de_prim!(Bool),
            PrimitiveType::I8 => de_prim!(I8),
            PrimitiveType::U8 => de_prim!(U8),
            PrimitiveType::I16 => de_prim!(I16),
            PrimitiveType::U16 => de_prim!(U16),
            PrimitiveType::I32 => de_prim!(I32),
            PrimitiveType::U32 => de_prim!(U32),
            PrimitiveType::I64 => de_prim!(I64),
            PrimitiveType::U64 => de_prim!(U64),
            PrimitiveType::I128 => de_prim!(I128),
            PrimitiveType::U128 => de_prim!(U128),
            PrimitiveType::F32 => Ok(TypeValue::F32(f32::deserialize(deserializer)?.into())),
            PrimitiveType::F64 => Ok(TypeValue::F64(f64::deserialize(deserializer)?.into())),
            PrimitiveType::String => de_prim!(String),
            PrimitiveType::Bytes => deserializer.deserialize_str(BytesVisitor).map(TypeValue::Bytes),
            PrimitiveType::Hash => deserializer.deserialize_str(HashVisitor).map(TypeValue::Hash),
        }
    }
}

struct BytesVisitor;

impl<'de> de::Visitor<'de> for BytesVisitor {
    type Value = Vec<u8>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(BYTES_DESC)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        hex_string(v, &self)
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_byte_buf(v.to_owned())
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(v)
    }
}

struct HashVisitor;

impl<'de> de::Visitor<'de> for HashVisitor {
    type Value = Box<Hash>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(HASH_DESC)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let data = hex_string(v, &self)?;
        Ok(Box::new(Hash { data }))
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let data = v
            .try_into()
            .map_err(|_| de::Error::invalid_value(de::Unexpected::Bytes(v), &"a 32-byte hash"))?;
        Ok(Box::new(Hash { data }))
    }
}

fn hex_string<T: hex::FromHex<Error = hex::FromHexError>, E: de::Error>(
    v: &str,
    exp: &dyn de::Expected,
) -> Result<T, E> {
    T::from_hex(v).map_err(|_| de::Error::invalid_value(de::Unexpected::Str(v), exp))
}

// TypeDef::Enum(_enu) => write!(f, "an enum"),
// TypeDef::Tuple(tup) => write!(f, "a {} tuple", tup.name.as_ref().unwrap()),
// TypeDef::Vec { element_type } =>

const BYTES_DESC: &str = "a hex string representing binary data";
const BYTES_DESC_PLURAL: &str = "hex strings representing binary data";
const HASH_DESC: &str = "a hex string representing a 32-byte hash";
const HASH_DESC_PLURAL: &str = "hex strings representing a 32-byte hash";

fn type_fmt_plural(typedef: &TypeDef) -> impl fmt::Display + '_ {
    fmt_fn(move |f| match typedef {
        TypeDef::Primitive(p) => match p {
            PrimitiveType::Unit => f.write_str("unit"),
            PrimitiveType::Bool => f.write_str("bool"),
            PrimitiveType::I8 => f.write_str("i8"),
            PrimitiveType::U8 => f.write_str("u8"),
            PrimitiveType::I16 => f.write_str("i16"),
            PrimitiveType::U16 => f.write_str("u16"),
            PrimitiveType::I32 => f.write_str("i32"),
            PrimitiveType::U32 => f.write_str("u32"),
            PrimitiveType::I64 => f.write_str("i64"),
            PrimitiveType::U64 => f.write_str("u64"),
            PrimitiveType::I128 => f.write_str("i128"),
            PrimitiveType::U128 => f.write_str("u128"),
            PrimitiveType::F32 => f.write_str("f32"),
            PrimitiveType::F64 => f.write_str("f64"),
            PrimitiveType::String => f.write_str("strings"),
            PrimitiveType::Bytes => f.write_str(BYTES_DESC_PLURAL),
            PrimitiveType::Hash => f.write_str(HASH_DESC_PLURAL),
        },
        TypeDef::Enum(_) => f.write_str("enums"),
        TypeDef::Tuple(tup) => write!(f, "{} tuples", tup.name.as_ref().unwrap()),
        TypeDef::Vec { element_type } => write!(f, "seqs of {}", type_fmt_plural(element_type)),
    })
}

struct VecVisitor<'a> {
    element_type: &'a TypeDef,
}

impl<'de> de::Visitor<'de> for VecVisitor<'_> {
    type Value = Vec<TypeValue>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a seq of {}", type_fmt_plural(self.element_type))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut v = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(el) = seq.next_element_seed(self.element_type)? {
            v.push(el);
        }
        Ok(v)
    }
}

impl<'de> DeserializeSeed<'de> for &TupleDef {
    type Value = TupleValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let visitor = ProductTypeVisitor {
            kind: ProductTypeKind::Tuple,
            name: self.name.as_ref().unwrap(),
            elements: &self.elements,
        };
        // THIS ONLY WORKS WITH JSON
        deserializer.deserialize_struct("", &[], visitor)
    }
}

impl<'de> DeserializeSeed<'de> for &ReducerDef {
    type Value = TupleValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let visitor = ProductTypeVisitor {
            kind: ProductTypeKind::Reducer,
            name: self.name.as_ref().unwrap(),
            elements: &self.args,
        };
        // THIS ONLY WORKS WITH JSON
        deserializer.deserialize_struct("", &[], visitor)
    }
}

enum ProductTypeKind {
    Tuple,
    Reducer,
}
impl ProductTypeKind {
    fn field_desc(&self) -> &'static str {
        match self {
            ProductTypeKind::Tuple => "field",
            ProductTypeKind::Reducer => "arg",
        }
    }
}
struct ProductTypeVisitor<'a> {
    kind: ProductTypeKind,
    name: &'a str,
    elements: &'a [ElementDef],
}

impl<'de> de::Visitor<'de> for ProductTypeVisitor<'_> {
    type Value = TupleValue;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ProductTypeKind::Tuple => write!(f, "a {} tuple", self.name),
            ProductTypeKind::Reducer => write!(f, "reducer args for {}", self.name),
        }
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        let key_vis = IdentifierVisitor {
            kind: match self.kind {
                ProductTypeKind::Tuple => IdentiferKind::Field,
                ProductTypeKind::Reducer => IdentiferKind::Arg,
            },
            elements: self.elements,
        };
        let mut elements = vec![None; self.elements.len()];
        let mut n = 0;
        // under a certain threshold, just do linear searches
        while let Some(key) = map.next_key_seed(key_vis)? {
            let slot = &mut elements[key.tag as usize];
            if slot.is_some() {
                return Err(de::Error::custom(format_args!(
                    "duplicate {} `{}`",
                    self.kind.field_desc(),
                    key.name.as_ref().unwrap()
                )));
            }
            *slot = Some(map.next_value_seed(&key.element_type)?);
            n += 1;
        }
        if n < self.elements.len() {
            // if this is None, weird, but ok
            if let Some(missing) = elements.iter().position(|field| field.is_none()) {
                let field_ty = self.kind.field_desc();
                let field_name = self.elements[missing].name.as_ref().unwrap();
                return Err(de::Error::custom(format_args!("missing {field_ty} `{field_name}`",)));
            }
        }
        let elements = elements.into_iter().map(Option::unwrap).collect();
        Ok(TupleValue { elements })
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let elements = self.elements.iter().enumerate().map(|(i, el)| {
            seq.next_element_seed(&el.element_type)?.ok_or_else(|| {
                let ty = match self.kind {
                    ProductTypeKind::Tuple => "reducer args for",
                    ProductTypeKind::Reducer => "tuple",
                };
                let (name, len) = (self.name, self.elements.len());
                let exp = fmt_fn(|f| write!(f, "{ty} {name} with {len} elements"));
                de::Error::invalid_length(i, &exp)
            })
        });
        let elements = elements.collect::<Result<_, _>>()?;
        Ok(TupleValue { elements })
    }
}

impl<'de> DeserializeSeed<'de> for &EnumDef {
    type Value = EnumValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // THIS ONLY WORKS BECAUSE OF JSON
        deserializer.deserialize_enum("", &[], self)
    }
}

impl<'de> de::Visitor<'de> for &EnumDef {
    type Value = EnumValue;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an enum")
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: de::EnumAccess<'de>,
    {
        let (variant, data) = data.variant_seed(IdentifierVisitor {
            kind: IdentiferKind::Variant,
            elements: &self.variants,
        })?;
        let tag = variant.tag;
        let type_value = Box::new(data.newtype_variant_seed(&variant.element_type)?);
        let element_value = ElementValue { tag, type_value };
        Ok(EnumValue { element_value })
    }
}

#[derive(Clone, Copy)]
enum IdentiferKind {
    Field,
    Variant,
    Arg,
}
impl IdentiferKind {
    fn container_desc(self) -> &'static str {
        match self {
            IdentiferKind::Field => "a tuple",
            IdentiferKind::Variant => "an enum",
            IdentiferKind::Arg => "a",
        }
    }
    fn desc(self) -> &'static str {
        match self {
            IdentiferKind::Field => "field",
            IdentiferKind::Variant => "variant",
            IdentiferKind::Arg => "reducer argument",
        }
    }
}
#[derive(Clone, Copy)]
struct IdentifierVisitor<'a> {
    kind: IdentiferKind,
    elements: &'a [ElementDef],
}

impl<'a, 'de> de::DeserializeSeed<'de> for IdentifierVisitor<'a> {
    type Value = &'a ElementDef;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_identifier(self)
    }
}

impl<'a, 'de> de::Visitor<'de> for IdentifierVisitor<'a> {
    type Value = &'a ElementDef;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ty = self.kind.container_desc();
        let el_ty = self.kind.desc();
        if self.elements.is_empty() {
            write!(f, "{ty} {el_ty} name, but there are no {el_ty}s")
        } else {
            write!(f, "{ty} {el_ty} name ({})", one_of(self.elements))
        }
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.elements
            .iter()
            .find(|var| var.name.as_deref().unwrap() == v)
            .ok_or_else(|| {
                let el_ty = self.kind.desc();
                if self.elements.is_empty() {
                    de::Error::custom(format_args!("unknown {el_ty} `{v}`, there are no {el_ty}s"))
                } else {
                    de::Error::custom(format_args!(
                        "unknown {el_ty} `{v}`, expected {}",
                        one_of(self.elements)
                    ))
                }
            })
    }
    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.elements.get(v as usize).ok_or_else(|| {
            let exp = fmt_fn(|f| {
                let el_ty = self.kind.desc();
                if self.elements.is_empty() {
                    write!(f, "{el_ty} tag but there are no {el_ty}s")
                } else {
                    write!(f, "{el_ty} tag 0 <= i < {}", self.elements.len())
                }
            });
            de::Error::invalid_value(de::Unexpected::Unsigned(v), &exp)
        })
    }
}

fn one_of(elements: &[ElementDef]) -> impl fmt::Display + '_ {
    fmt_fn(|f| {
        let it = elements
            .iter()
            .map(|v| v.name.as_deref().unwrap())
            .map(|name| fmt_fn(move |f| write!(f, "`{name}`")));
        if it.len() == 2 {
            write!(f, "{}", it.format(" or "))
        } else {
            write!(f, "one of {}", it.format(", "))
        }
    })
}

pub struct ValueWithSchema<'a> {
    pub value: &'a TypeValue,
    pub schema: &'a TypeDef,
}

impl Serialize for ValueWithSchema<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match (self.value, self.schema) {
            (TypeValue::Unit, &TypeDef::Unit) => serializer.serialize_unit(),
            (TypeValue::Bool(v), &TypeDef::Bool) => serializer.serialize_bool(*v),
            (TypeValue::I8(v), &TypeDef::I8) => serializer.serialize_i8(*v),
            (TypeValue::U8(v), &TypeDef::U8) => serializer.serialize_u8(*v),
            (TypeValue::I16(v), &TypeDef::I16) => serializer.serialize_i16(*v),
            (TypeValue::U16(v), &TypeDef::U16) => serializer.serialize_u16(*v),
            (TypeValue::I32(v), &TypeDef::I32) => serializer.serialize_i32(*v),
            (TypeValue::U32(v), &TypeDef::U32) => serializer.serialize_u32(*v),
            (TypeValue::I64(v), &TypeDef::I64) => serializer.serialize_i64(*v),
            (TypeValue::U64(v), &TypeDef::U64) => serializer.serialize_u64(*v),
            (TypeValue::I128(v), &TypeDef::I128) => serializer.serialize_i128(*v),
            (TypeValue::U128(v), &TypeDef::U128) => serializer.serialize_u128(*v),
            (TypeValue::F32(v), &TypeDef::F32) => serializer.serialize_f32((*v).into()),
            (TypeValue::F64(v), &TypeDef::F64) => serializer.serialize_f64((*v).into()),
            (TypeValue::String(s), &TypeDef::String) => serializer.serialize_str(s),
            (TypeValue::Bytes(b), &TypeDef::Bytes) => {
                let s = hex::encode(b);
                serializer.serialize_str(&s)
            }
            (TypeValue::Hash(h), &TypeDef::Hash) => {
                let s = hex::encode(h.data);
                serializer.serialize_str(&s)
            }
            (TypeValue::Enum(value), TypeDef::Enum(schema)) => EnumWithSchema { value, schema }.serialize(serializer),
            (TypeValue::Tuple(value), TypeDef::Tuple(schema)) => {
                TupleWithSchema { value, schema }.serialize(serializer)
            }
            (TypeValue::Vec(value), TypeDef::Vec { element_type: schema }) => {
                let mut seq = serializer.serialize_seq(Some(value.len()))?;
                for value in value {
                    seq.serialize_element(&ValueWithSchema { value, schema })?;
                }
                seq.end()
            }
            _ => panic!("mismatched value and schema"),
        }
    }
}

pub struct EnumWithSchema<'a> {
    pub value: &'a EnumValue,
    pub schema: &'a EnumDef,
}

impl Serialize for EnumWithSchema<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // can't use serialize_variant cause we're too dynamic :(
        let mut map = serializer.serialize_map(Some(1))?;
        let value = &self.value.element_value.type_value;
        let variant = &self.schema.variants[usize::from(self.value.element_value.tag)];
        let schema = &variant.element_type;
        map.serialize_entry(variant.name.as_deref().unwrap(), &ValueWithSchema { value, schema })?;
        map.end()
    }
}

pub struct TupleWithSchema<'a> {
    pub value: &'a TupleValue,
    pub schema: &'a TupleDef,
}

impl Serialize for TupleWithSchema<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.value.elements.len()))?;
        for (value, elem) in self.value.elements.iter().zip_eq(&self.schema.elements) {
            let schema = &elem.element_type;
            map.serialize_entry(elem.name.as_deref().unwrap(), &ValueWithSchema { value, schema })?;
        }
        map.end()
    }
}

pub struct ReducerArgsWithSchema<'a> {
    pub value: &'a TupleValue,
    pub schema: &'a ReducerDef,
}

impl Serialize for ReducerArgsWithSchema<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.value.elements.len()))?;
        for (value, elem) in self.value.elements.iter().zip_eq(&self.schema.args) {
            let schema = &elem.element_type;
            seq.serialize_element(&ValueWithSchema { value, schema })?;
        }
        seq.end()
    }
}
