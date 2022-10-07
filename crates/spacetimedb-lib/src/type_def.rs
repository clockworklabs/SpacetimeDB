use serde::{Deserialize, Serialize};

use crate::buffer::{BufReader, BufWriter, DecodeError};

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
    pub elements: Vec<ElementDef>,
}

impl TupleDef {
    pub fn decode(bytes: &mut impl BufReader) -> Result<Self, DecodeError> {
        let len = read_len(bytes)?;

        let mut elements = Vec::with_capacity(len.into());
        for _ in 0..len {
            elements.push(ElementDef::decode(bytes)?);
        }
        Ok(TupleDef { elements })
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        write_len(bytes, self.elements.len());
        for item in &self.elements {
            item.encode(bytes);
        }
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
        let len = read_len(bytes)?;

        let mut items = Vec::with_capacity(len.into());
        for _ in 0..len {
            items.push(ElementDef::decode(bytes)?);
        }
        Ok(EnumDef { variants: items })
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        write_len(bytes, self.variants.len());
        for item in &self.variants {
            item.encode(bytes);
        }
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
            0x04 => TypeDef::Primitive(PrimitiveType::U16),
            0x03 => TypeDef::Primitive(PrimitiveType::U8),
            0x05 => TypeDef::Primitive(PrimitiveType::U32),
            0x06 => TypeDef::Primitive(PrimitiveType::U64),
            0x07 => TypeDef::Primitive(PrimitiveType::U128),
            0x08 => TypeDef::Primitive(PrimitiveType::I8),
            0x09 => TypeDef::Primitive(PrimitiveType::I16),
            0x0A => TypeDef::Primitive(PrimitiveType::I32),
            0x0B => TypeDef::Primitive(PrimitiveType::I64),
            0x0C => TypeDef::Primitive(PrimitiveType::I128),
            0x0D => TypeDef::Primitive(PrimitiveType::Bool),
            0x0E => TypeDef::Primitive(PrimitiveType::F32),
            0x0F => TypeDef::Primitive(PrimitiveType::F64),
            0x10 => TypeDef::Primitive(PrimitiveType::String),
            0x11 => TypeDef::Primitive(PrimitiveType::Bytes),
            0x12 => TypeDef::Primitive(PrimitiveType::Bytes),
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
            }),
        }
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
