use core::fmt;

use serde::{Deserialize, Serialize};

use crate::buffer::{BufReader, BufWriter, DecodeError};
use crate::fmt_fn;

// NOTICE!! every time you make a breaking change to the wire format, you MUST
//          bump `SCHEMA_FORMAT_VERSION` in lib.rs!

// () -> Tuple or enum?
// (0: 1) -> Tuple or enum?
// (0: 1, x: (1: 2 | 0: 2))
// (0: 1 | 1: 2)

// Types
// () -> 0-tuple or void?
// (0: u32) -> 1-tuple or 1-enum or monuple?
// (0: u32, 1: (0: 1 | 0: 2)) -> 2-tuple with enum for second type
// (0: 1 | 0: 2) -> 2-enum

// Proposed Types?
// () -> 0-tuple (either + or * operator)
// (1: u32) -> 1-tuple (either + or * operator)
// (1: u32, 2: u32) -> 2-tuple (* operator)
// (1: u32 | 2: u32) -> 2-tuple (+ operator)

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ElementDef {
    // In the case of tuples, this is the id of the column
    // In the case of enums, this is the id of the variant
    pub tag: u8,
    pub name: Option<String>,
    pub element_type: TypeDef,
}

impl ElementDef {
    pub fn decode(bytes: &mut impl BufReader) -> Result<Self, DecodeError> {
        let tag = bytes.get_u8()?;

        let name = read_str(bytes)?;
        let name = (!name.is_empty()).then(|| name.to_owned());

        let element_type = TypeDef::decode(bytes)?;

        Ok(ElementDef {
            tag,
            element_type,
            name,
        })
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        bytes.put_u8(self.tag);

        write_str(bytes, self.name.as_deref().unwrap_or(""));

        self.element_type.encode(bytes);
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct TupleDef {
    pub name: Option<Box<str>>,
    pub elements: Vec<ElementDef>,
}

impl TupleDef {
    pub fn decode(bytes: &mut impl BufReader) -> Result<Self, DecodeError> {
        let name = read_str(bytes)?;
        let name = (!name.is_empty()).then(|| name.into());

        let elements = decode_vec_fn(bytes, ElementDef::decode)?;
        Ok(TupleDef { name, elements })
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        write_str(bytes, self.name.as_deref().unwrap_or(""));

        encode_vec_fn(bytes, &self.elements, ElementDef::encode);
    }
}

impl fmt::Display for TupleDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("tuple ")?;
        if let Some(name) = &self.name {
            write!(f, "{name} ")?;
        }
        f.debug_map()
            .entries(self.elements.iter().enumerate().map(|(i, el)| {
                let key = fmt_fn(move |f| {
                    write!(f, "{i}")?;
                    if let Some(name) = &el.name {
                        write!(f, " ({name})")?;
                    }
                    Ok(())
                });
                (key, fmt_fn(|f| el.element_type.fmt(f)))
            }))
            .finish()
    }
}

// TODO: probably implement this with a tuple but store whether the tuple
// is a sum tuple or a product tuple, then we have uniformity over types
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct EnumDef {
    pub variants: Vec<ElementDef>,
}

impl EnumDef {
    pub fn decode(bytes: &mut impl BufReader) -> Result<Self, DecodeError> {
        let variants = decode_vec_fn(bytes, ElementDef::decode)?;
        Ok(EnumDef { variants })
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        encode_vec_fn(bytes, &self.variants, ElementDef::encode)
    }
}

impl fmt::Display for EnumDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("enum ")?;
        f.debug_map()
            .entries(self.variants.iter().enumerate().map(|(i, el)| {
                let key = fmt_fn(move |f| {
                    write!(f, "{i}")?;
                    if let Some(name) = &el.name {
                        write!(f, " ({name})")?;
                    }
                    Ok(())
                });
                (key, fmt_fn(|f| el.element_type.fmt(f)))
            }))
            .finish()
    }
}

/// Type definitions
///
/// WARNING:
///
/// Is important the order in this enum so sorting work correctly, and it must match
/// [TypeWideValue]/[TypeValue]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum TypeDef {
    Primitive(PrimitiveType),

    Enum(EnumDef),
    Tuple(TupleDef),

    Vec { element_type: Box<TypeDef> },
}

impl From<PrimitiveType> for TypeDef {
    fn from(prim: PrimitiveType) -> Self {
        TypeDef::Primitive(prim)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PrimitiveType {
    Unit,
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    I128,
    U128,
    F32,
    F64,
    String,
    Bytes,
    Hash,
}
impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            PrimitiveType::Unit => "unit",
            PrimitiveType::Bool => "bool",
            PrimitiveType::I8 => "i8",
            PrimitiveType::U8 => "u8",
            PrimitiveType::I16 => "i16",
            PrimitiveType::U16 => "u16",
            PrimitiveType::I32 => "i32",
            PrimitiveType::U32 => "u32",
            PrimitiveType::I64 => "i64",
            PrimitiveType::U64 => "u64",
            PrimitiveType::I128 => "i128",
            PrimitiveType::U128 => "u128",
            PrimitiveType::F32 => "f32",
            PrimitiveType::F64 => "f64",
            PrimitiveType::String => "string",
            PrimitiveType::Bytes => "bytes",
            PrimitiveType::Hash => "hash",
        })
    }
}
#[allow(non_upper_case_globals)]
impl TypeDef {
    pub const Unit: Self = TypeDef::Primitive(PrimitiveType::Unit);
    pub const Bool: Self = TypeDef::Primitive(PrimitiveType::Bool);
    pub const I8: Self = TypeDef::Primitive(PrimitiveType::I8);
    pub const U8: Self = TypeDef::Primitive(PrimitiveType::U8);
    pub const I16: Self = TypeDef::Primitive(PrimitiveType::I16);
    pub const U16: Self = TypeDef::Primitive(PrimitiveType::U16);
    pub const I32: Self = TypeDef::Primitive(PrimitiveType::I32);
    pub const U32: Self = TypeDef::Primitive(PrimitiveType::U32);
    pub const I64: Self = TypeDef::Primitive(PrimitiveType::I64);
    pub const U64: Self = TypeDef::Primitive(PrimitiveType::U64);
    pub const I128: Self = TypeDef::Primitive(PrimitiveType::I128);
    pub const U128: Self = TypeDef::Primitive(PrimitiveType::U128);
    pub const F32: Self = TypeDef::Primitive(PrimitiveType::F32);
    pub const F64: Self = TypeDef::Primitive(PrimitiveType::F64);
    pub const String: Self = TypeDef::Primitive(PrimitiveType::String);
    pub const Bytes: Self = TypeDef::Primitive(PrimitiveType::Bytes);
    pub const Hash: Self = TypeDef::Primitive(PrimitiveType::Hash);
}

impl TypeDef {
    pub fn decode(bytes: &mut impl BufReader) -> Result<Self, DecodeError> {
        let tag = bytes.get_u8()?;

        let res = match tag {
            0x00 => TypeDef::Tuple(TupleDef::decode(bytes)?),
            0x01 => TypeDef::Enum(EnumDef::decode(bytes)?),
            0x02 => TypeDef::Vec {
                element_type: Box::new(TypeDef::decode(bytes)?),
            },
            0x04 => TypeDef::U16,
            0x03 => TypeDef::U8,
            0x05 => TypeDef::U32,
            0x06 => TypeDef::U64,
            0x07 => TypeDef::U128,
            0x08 => TypeDef::I8,
            0x09 => TypeDef::I16,
            0x0A => TypeDef::I32,
            0x0B => TypeDef::I64,
            0x0C => TypeDef::I128,
            0x0D => TypeDef::Bool,
            0x0E => TypeDef::F32,
            0x0F => TypeDef::F64,
            0x10 => TypeDef::String,
            0x11 => TypeDef::Bytes,
            0x12 => TypeDef::Unit,
            0x13 => TypeDef::Hash,
            _ => return Err(DecodeError::InvalidTag),
        };

        Ok(res)
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        match self {
            TypeDef::Tuple(t) => {
                bytes.put_u8(0x00);
                t.encode(bytes);
            }
            TypeDef::Enum(e) => {
                bytes.put_u8(0x01);
                e.encode(bytes);
            }
            TypeDef::Vec { element_type } => {
                bytes.put_u8(0x02);
                element_type.encode(bytes);
            }
            TypeDef::Primitive(prim) => bytes.put_u8(match prim {
                PrimitiveType::U8 => 0x03,
                PrimitiveType::U16 => 0x04,
                PrimitiveType::U32 => 0x05,
                PrimitiveType::U64 => 0x06,
                PrimitiveType::U128 => 0x07,
                PrimitiveType::I8 => 0x08,
                PrimitiveType::I16 => 0x09,
                PrimitiveType::I32 => 0x0A,
                PrimitiveType::I64 => 0x0B,
                PrimitiveType::I128 => 0x0C,
                PrimitiveType::Bool => 0x0D,
                PrimitiveType::F32 => 0x0E,
                PrimitiveType::F64 => 0x0F,
                PrimitiveType::String => 0x10,
                PrimitiveType::Bytes => 0x11,
                PrimitiveType::Unit => 0x12,
                PrimitiveType::Hash => 0x13,
            }),
        }
    }
}

impl fmt::Display for TypeDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeDef::Primitive(p) => p.fmt(f),
            TypeDef::Enum(enu) => enu.fmt(f),
            TypeDef::Tuple(tup) => tup.fmt(f),
            TypeDef::Vec { element_type } => write!(f, "vec<{element_type}>"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TableDef {
    pub tuple: TupleDef,
    /// must be sorted!
    pub unique_columns: Vec<u8>,
}

impl TableDef {
    pub fn decode(bytes: &mut impl BufReader) -> Result<Self, DecodeError> {
        let tuple = TupleDef::decode(bytes)?;
        let unique_columns_len = read_len(bytes)?;
        let mut unique_columns = bytes.get_slice(unique_columns_len)?.to_owned();
        unique_columns.sort();
        Ok(Self { tuple, unique_columns })
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        self.tuple.encode(bytes);
        write_len(bytes, self.unique_columns.len());
        bytes.put_slice(&self.unique_columns);
    }
}

#[derive(Debug, Clone)]
pub struct ReducerDef {
    pub name: Option<Box<str>>,
    pub args: Vec<ElementDef>,
}

impl ReducerDef {
    pub fn decode(bytes: &mut impl BufReader) -> Result<Self, DecodeError> {
        let name = read_str(bytes)?;
        let name = (!name.is_empty()).then(|| name.into());
        let args = decode_vec_fn(bytes, ElementDef::decode)?;
        Ok(Self { name, args })
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        write_str(bytes, self.name.as_deref().unwrap_or(""));
        encode_vec_fn(bytes, &self.args, ElementDef::encode);
    }
}

#[derive(Debug, Clone)]
pub enum EntityDef {
    Table(TableDef),
    Reducer(ReducerDef),
}

impl EntityDef {
    pub fn decode<R: BufReader>(bytes: &mut R) -> Result<Self, DecodeError> {
        let tag = bytes.get_u8()?;
        Self::decode_with_tag(bytes, tag)
    }
    fn decode_with_tag(bytes: &mut impl BufReader, tag: u8) -> Result<Self, DecodeError> {
        match tag {
            0x00 => TableDef::decode(bytes).map(Self::Table),
            0x01 => ReducerDef::decode(bytes).map(Self::Reducer),
            _ => Err(DecodeError::InvalidTag),
        }
    }
    pub fn encode<W: BufWriter>(&self, bytes: &mut W) {
        match self {
            EntityDef::Table(t) => {
                bytes.put_u8(0x00);
                t.encode(bytes);
            }
            EntityDef::Reducer(r) => {
                bytes.put_u8(0x01);
                r.encode(bytes);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModuleItemDef {
    Entity(EntityDef),
    Tuple(TupleDef),
}

impl ModuleItemDef {
    pub fn decode<R: BufReader>(bytes: &mut R) -> Result<Self, DecodeError> {
        let tag = bytes.get_u8()?;
        match tag {
            0x10 => TupleDef::decode(bytes).map(Self::Tuple),
            _ => EntityDef::decode_with_tag(bytes, tag).map(Self::Entity),
        }
    }
    pub fn encode<W: BufWriter>(&self, bytes: &mut W) {
        match self {
            ModuleItemDef::Entity(e) => e.encode(bytes),
            ModuleItemDef::Tuple(t) => t.encode(bytes),
        }
    }
}

pub struct ModuleDef {
    pub items: Vec<(String, ModuleItemDef)>,
}

impl ModuleDef {
    pub fn decode<R: BufReader>(bytes: &mut R) -> Result<Self, DecodeError> {
        let items = decode_vec_fn(bytes, |bytes| {
            Ok((read_str(bytes)?.to_owned(), ModuleItemDef::decode(bytes)?))
        })?;
        Ok(ModuleDef { items })
    }
}

fn read_len(bytes: &mut impl BufReader) -> Result<usize, DecodeError> {
    // eventually should be leb128
    bytes.get_u8().map(Into::into)
}
fn write_len(bytes: &mut impl BufWriter, len: usize) {
    bytes.put_u8(len.try_into().expect("too big"))
}
fn read_str(bytes: &mut impl BufReader) -> Result<&str, DecodeError> {
    let len = read_len(bytes)?;
    let slice = bytes.get_slice(len)?;
    Ok(std::str::from_utf8(slice)?)
}
fn write_str(bytes: &mut impl BufWriter, s: &str) {
    write_len(bytes, s.len());
    bytes.put_slice(s.as_bytes());
}
fn decode_vec_fn<T, R: BufReader>(
    bytes: &mut R,
    f: impl Fn(&mut R) -> Result<T, DecodeError>,
) -> Result<Vec<T>, DecodeError> {
    let len = read_len(bytes)?;
    let mut v = Vec::with_capacity(len);
    for _ in 0..len {
        v.push(f(bytes)?)
    }
    Ok(v)
}
// fn decode_vec<T: Decode>(bytes: &mut impl BufReader) -> Result<Vec<T>, DecodeError> {
//     decode_vec_fn(bytes, T::decode)
// }
fn encode_vec_fn<T, W: BufWriter>(bytes: &mut W, v: &[T], f: impl Fn(&T, &mut W)) {
    write_len(bytes, v.len());
    for t in v {
        f(t, bytes)
    }
}
// fn encode_vec<T: Encode>(bytes: &mut impl BufWriter, v: &[T]) {
//     encode_vec_fn(bytes, v, T::encode)
// }
