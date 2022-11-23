use crate::error::LibError;
use crate::{
    buffer::{BufReader, BufWriter, DecodeError},
    type_def::{EnumDef, TupleDef, TypeDef},
    DataKey, Hash,
};
use enum_as_inner::EnumAsInner;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::{fmt, hash};

// NOTICE!! every time you make a breaking change to the wire format, you MUST
//          bump `SCHEMA_FORMAT_VERSION` in lib.rs!

/// Totally ordered [f32]
pub type F32 = decorum::Total<f32>;
/// Totally ordered [f64]
pub type F64 = decorum::Total<f64>;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ElementValue {
    pub tag: u8,
    pub type_value: Box<TypeValue>,
}

#[derive(Debug, Clone, Ord, PartialOrd, Serialize, Deserialize)]
pub struct TupleValue {
    pub elements: Box<[TypeValue]>,
}

impl fmt::Display for TupleValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        for (i, e) in self.elements.iter().enumerate() {
            if i < self.elements.len() - 1 {
                write!(f, "{}: {}, ", i, e)?;
            } else {
                write!(f, "{}: {}", i, e)?;
            }
        }
        write!(f, "}}")?;
        Ok(())
    }
}

impl hash::Hash for TupleValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // TODO(cloutiertyler): Oh my heavens, copies galore.
        self.to_data_key().hash(state);
    }
}

impl PartialEq for TupleValue {
    fn eq(&self, other: &Self) -> bool {
        self.to_data_key() == other.to_data_key()
    }
}

impl Eq for TupleValue {}

impl TupleValue {
    pub fn to_data_key(&self) -> DataKey {
        let mut bytes = Vec::new();
        self.encode(&mut bytes);
        DataKey::from_data(&bytes.iter())
    }

    pub fn decode(tuple_def: &TupleDef, bytes: &mut impl BufReader) -> Result<Self, DecodeError> {
        let mut elements = Vec::with_capacity(tuple_def.elements.len());
        for elem in &tuple_def.elements {
            // TODO: sort by tags or use the tags in some way or remove the tags from the def
            elements.push(TypeValue::decode(&elem.element_type, bytes)?);
        }

        Ok(TupleValue {
            elements: elements.into(),
        })
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        for element in &*self.elements {
            element.encode(bytes);
        }
    }

    pub fn get_field(&self, index: usize, named: Option<&'static str>) -> Result<&TypeValue, LibError> {
        self.elements
            .get(index)
            .ok_or(LibError::TupleFieldNotFound(index, named))
    }

    pub fn field_as_bool(&self, index: usize, named: Option<&'static str>) -> Result<bool, LibError> {
        let f = self.get_field(index, named)?;
        let r = f.as_bool().ok_or(LibError::TupleFieldTypeInvalid(index, named))?;
        Ok(*r)
    }

    pub fn field_as_u32(&self, index: usize, named: Option<&'static str>) -> Result<u32, LibError> {
        let f = self.get_field(index, named)?;
        let r = f.as_u32().ok_or(LibError::TupleFieldTypeInvalid(index, named))?;
        Ok(*r)
    }

    pub fn field_as_i64(&self, index: usize, named: Option<&'static str>) -> Result<i64, LibError> {
        let f = self.get_field(index, named)?;
        let r = f.as_i64().ok_or(LibError::TupleFieldTypeInvalid(index, named))?;
        Ok(*r)
    }

    pub fn field_as_str(&self, index: usize, named: Option<&'static str>) -> Result<&str, LibError> {
        let f = self.get_field(index, named)?;
        let r = f.as_string().ok_or(LibError::TupleFieldTypeInvalid(index, named))?;
        Ok(r)
    }

    pub fn field_as_bytes(&self, index: usize, named: Option<&'static str>) -> Result<&[u8], LibError> {
        let f = self.get_field(index, named)?;
        let r = f.as_bytes().ok_or(LibError::TupleFieldTypeInvalid(index, named))?;
        Ok(r)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct EnumValue {
    pub element_value: ElementValue,
}

impl EnumValue {
    pub fn decode(enum_def: &EnumDef, bytes: &mut impl BufReader) -> Result<Self, DecodeError> {
        let tag = bytes.get_u8()?;

        let elem = enum_def
            .variants
            .iter()
            .find(|var| var.tag == tag)
            .ok_or(DecodeError::InvalidTag)?;
        let type_value = TypeValue::decode(&elem.element_type, bytes)?;

        let element_value = ElementValue {
            tag,
            type_value: Box::new(type_value),
        };
        Ok(EnumValue { element_value })
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        bytes.put_u8(self.element_value.tag);
        self.element_value.type_value.encode(bytes);
    }
}

/// Helper for implement `Ord`/`Eq` for numerical values
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
enum TypeWideValue<'a> {
    Unit,
    Bool(bool),
    I64(i64),
    U64(u64),
    I128(i128),
    U128(u128),
    F64(F64),
    String(&'a str),
    Bytes(&'a [u8]),
    Hash(&'a Hash),
    Enum(&'a EnumValue),
    Vec(&'a [TypeValue]),
}

impl<'a> TypeWideValue<'a> {
    fn from_i64(x: i64) -> Self {
        if x < 0 {
            Self::I64(x)
        } else {
            Self::U64(x as u64)
        }
    }

    fn from_i128(x: i128) -> Self {
        if x < 0 {
            Self::I128(x)
        } else {
            Self::U128(x as u128)
        }
    }
}

/// The `scalars` values.
///
/// WARNING:
///
/// Is important the order in this enum so sorting work correctly, and it must match
/// [TypeWideValue]/[TypeDef]
#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TypeValue {
    /// The **BOTTOM** value
    Unit,
    /// Base types
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    I128(i128),
    U128(u128),
    F32(F32),
    F64(F64),
    String(String),
    Bytes(Vec<u8>),
    Hash(Box<Hash>),
    Enum(EnumValue),
    Tuple(TupleValue),

    // TODO(cloutiertyler): This is very inefficient it turns out
    // we should probably have a packed encoding like protobuf
    // so if someone tries to make a Vec<f32>, we don't spend all
    // day encoding and decoding.
    // We could have:
    // Vec(TypeDef, Vec<u8>)
    // or VecF32(Vec<f32>), ... etc
    Vec(Vec<TypeValue>),
}

impl TypeValue {
    /// Promote the values to their wider representation to make easier to compare.
    ///
    /// It turns the negative values to [i64]/[i128] and positive to [u64]/[u128], ie:
    ///
    ///
    ///  -1i64 -> -1i64
    ///   1i64 ->  1u64
    fn to_wide_value(&self) -> TypeWideValue<'_> {
        match self {
            TypeValue::Unit => TypeWideValue::Unit,
            TypeValue::Bool(x) => TypeWideValue::Bool(*x),
            TypeValue::I8(x) => TypeWideValue::from_i64(*x as i64),
            TypeValue::U8(x) => TypeWideValue::U64(*x as u64),
            TypeValue::I16(x) => TypeWideValue::from_i64(*x as i64),
            TypeValue::U16(x) => TypeWideValue::U64(*x as u64),
            TypeValue::I32(x) => TypeWideValue::from_i64(*x as i64),
            TypeValue::U32(x) => TypeWideValue::U64(*x as u64),
            TypeValue::I64(x) => TypeWideValue::from_i64(*x),
            TypeValue::U64(x) => TypeWideValue::U64(*x as u64),
            TypeValue::I128(x) => TypeWideValue::from_i128(*x),
            TypeValue::U128(x) => TypeWideValue::U128(*x),
            TypeValue::F32(x) => TypeWideValue::F64(F64::from(x.into_inner() as f64)),
            TypeValue::F64(x) => TypeWideValue::F64(*x),

            TypeValue::String(x) => TypeWideValue::String(x),
            TypeValue::Bytes(x) => TypeWideValue::Bytes(x),
            TypeValue::Hash(x) => TypeWideValue::Hash(x),
            TypeValue::Enum(x) => TypeWideValue::Enum(x),
            TypeValue::Tuple(x) => TypeWideValue::Vec(&x.elements),
            TypeValue::Vec(x) => TypeWideValue::Vec(x),
        }
    }

    pub fn decode(type_def: &TypeDef, bytes: &mut impl BufReader) -> Result<Self, DecodeError> {
        macro_rules! prim {
            ($v:ident, $get:ident) => {
                TypeValue::$v(bytes.$get()?)
            };
        }
        let result = match *type_def {
            TypeDef::Tuple(ref tuple_def) => TypeValue::Tuple(TupleValue::decode(tuple_def, bytes)?),
            TypeDef::Enum(ref enum_def) => TypeValue::Enum(EnumValue::decode(enum_def, bytes)?),
            TypeDef::Vec { ref element_type } => {
                let len = bytes.get_u16()?;
                let mut vec = Vec::with_capacity(len.into());
                for _ in 0..len {
                    vec.push(TypeValue::decode(element_type, bytes)?);
                }
                TypeValue::Vec(vec)
            }
            TypeDef::U8 => {
                prim!(U8, get_u8)
            }
            TypeDef::U16 => {
                prim!(U16, get_u16)
            }
            TypeDef::U32 => {
                prim!(U32, get_u32)
            }
            TypeDef::U64 => {
                prim!(U64, get_u64)
            }
            TypeDef::U128 => {
                prim!(U128, get_u128)
            }
            TypeDef::I8 => {
                prim!(I8, get_i8)
            }
            TypeDef::I16 => {
                prim!(I16, get_i16)
            }
            TypeDef::I32 => {
                prim!(I32, get_i32)
            }
            TypeDef::I64 => {
                prim!(I64, get_i64)
            }
            TypeDef::I128 => {
                prim!(I128, get_i128)
            }
            TypeDef::Bool => TypeValue::Bool(match bytes.get_u8()? {
                0x00 => false,
                0x01 => true,
                _ => {
                    // TODO: how strict should we be?
                    // return Err(DecodeError::InvalidTag)
                    true
                }
            }),
            TypeDef::F32 => TypeValue::F32(f32::from_bits(bytes.get_u32()?).into()),
            TypeDef::F64 => TypeValue::F64(f64::from_bits(bytes.get_u64()?).into()),
            TypeDef::String => {
                let len = bytes.get_u16()?;
                let slice = bytes.get_slice(len.into())?;
                let string = std::str::from_utf8(slice)?;
                TypeValue::String(string.to_owned())
            }
            TypeDef::Bytes => {
                let len = bytes.get_u16()?;
                let slice = bytes.get_slice(len.into())?;
                TypeValue::Bytes(slice.to_owned())
            }
            TypeDef::Hash => TypeValue::Hash(Box::new(Hash {
                data: bytes.get_array()?,
            })),
            TypeDef::Unit => TypeValue::Unit,
        };
        Ok(result)
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        macro_rules! prim {
            ($put:ident, $v:expr) => {
                bytes.$put(*$v)
            };
        }
        match self {
            TypeValue::Tuple(v) => {
                v.encode(bytes);
            }
            TypeValue::Enum(v) => {
                v.encode(bytes);
            }
            TypeValue::Vec(v) => {
                let len = v.len().try_into().expect("too big");
                bytes.put_u16(len);
                for val in v {
                    val.encode(bytes);
                }
            }
            TypeValue::U8(v) => {
                prim!(put_u8, v)
            }
            TypeValue::U16(v) => {
                prim!(put_u16, v)
            }
            TypeValue::U32(v) => {
                prim!(put_u32, v)
            }
            TypeValue::U64(v) => {
                prim!(put_u64, v)
            }
            TypeValue::U128(v) => {
                prim!(put_u128, v)
            }
            TypeValue::I8(v) => {
                prim!(put_i8, v)
            }
            TypeValue::I16(v) => {
                prim!(put_i16, v)
            }
            TypeValue::I32(v) => {
                prim!(put_i32, v)
            }
            TypeValue::I64(v) => {
                prim!(put_i64, v)
            }
            TypeValue::I128(v) => {
                prim!(put_i128, v)
            }
            TypeValue::Bool(v) => {
                bytes.put_u8(u8::from(*v));
            }
            TypeValue::F32(v) => {
                bytes.put_u32(v.into_inner().to_bits());
            }
            TypeValue::F64(v) => {
                bytes.put_u64(v.into_inner().to_bits());
            }
            TypeValue::String(v) => {
                let len = v.len().try_into().expect("too big");
                bytes.put_u16(len);
                bytes.put_slice(v.as_bytes())
            }
            TypeValue::Bytes(v) => {
                let len = v.len().try_into().expect("too big");
                bytes.put_u16(len);
                bytes.put_slice(v)
            }
            TypeValue::Hash(v) => bytes.put_slice(&v.data),
            TypeValue::Unit => {
                // Do nothing.
            }
        }
    }
}

impl PartialOrd for TypeValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.to_wide_value().partial_cmp(&other.to_wide_value())
    }
}

impl Ord for TypeValue {
    fn cmp(&self, other: &Self) -> Ordering {
        self.to_wide_value().cmp(&other.to_wide_value())
    }
}

impl fmt::Display for TypeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeValue::Tuple(v) => write!(f, "{}", v),
            TypeValue::Enum(_) => write!(f, "<enum>"),
            TypeValue::Vec(v) => {
                write!(f, "[")?;
                for (i, t) in v.iter().enumerate() {
                    if i < v.len() - 1 {
                        write!(f, "{}, ", t)?;
                    } else {
                        write!(f, "{}", t)?;
                    }
                }
                write!(f, "]")?;
                Ok(())
            }
            TypeValue::U8(n) => write!(f, "{}", n),
            TypeValue::U16(n) => write!(f, "{}", n),
            TypeValue::U32(n) => write!(f, "{}", n),
            TypeValue::U64(n) => write!(f, "{}", n),
            TypeValue::U128(n) => write!(f, "{}", n),
            TypeValue::I8(n) => write!(f, "{}", n),
            TypeValue::I16(n) => write!(f, "{}", n),
            TypeValue::I32(n) => write!(f, "{}", n),
            TypeValue::I64(n) => write!(f, "{}", n),
            TypeValue::I128(n) => write!(f, "{}", n),
            TypeValue::Bool(n) => write!(f, "{}", n),
            TypeValue::F32(n) => write!(f, "{}", n),
            TypeValue::F64(n) => write!(f, "{}", n),
            TypeValue::String(n) => write!(f, "{}", n),
            TypeValue::Bytes(bytes) => write!(f, "{}", hex::encode(bytes)),
            TypeValue::Hash(h) => write!(f, "{}", hex::encode(h.data)),
            TypeValue::Unit => write!(f, "<unit>"),
        }
    }
}

// impl Display for ColValue {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         match self {
//             ColValue::Integer(IntValue::U8(v)) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::Integer(IntValue::U16(v)) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::Integer(IntValue::U32(v)) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::Integer(IntValue::U64(v)) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::Integer(IntValue::U128(v)) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::Integer(IntValue::I8(v)) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::Integer(IntValue::I16(v)) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::Integer(IntValue::I32(v)) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::Integer(IntValue::I64(v)) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::Integer(IntValue::I128(v)) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::Boolean(v) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::Float(FloatValue::F32(v)) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::Float(FloatValue::F64(v)) => {
//                 write!(f, "{}", *v)
//             }
//             ColValue::String(v) => {
//                 write!(f, "{}", v)
//             },
//             ColValue::Bytes(v) => {
//                 write!(f, "{:?}", v)
//             },
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;

    // Verify the sorting match expectations
    #[test]
    fn test_sorting_values() {
        let values = vec![
            TypeValue::Unit,
            TypeValue::Bool(false),
            TypeValue::Bool(true),
            TypeValue::I32(-3),
            TypeValue::I64(-2),
            TypeValue::I8(-1),
            TypeValue::I16(0),
            TypeValue::I8(1),
            TypeValue::I64(2),
            TypeValue::I32(3),
            TypeValue::I8(i8::MAX),
            TypeValue::U8((i8::MAX as u8) + 1),
            TypeValue::I16(i16::MAX),
            TypeValue::U16((i16::MAX as u16) + 1),
            TypeValue::I32(i32::MAX),
            TypeValue::U32((i32::MAX as u32) + 1),
            TypeValue::I64(i64::MAX),
            TypeValue::U64((i64::MAX as u64) + 1),
            TypeValue::I128(i128::MAX),
            TypeValue::U128((i128::MAX as u128) + 1),
            TypeValue::F32(F32::from(f32::MAX)),
            TypeValue::F64(F64::from(f32::MAX as f64) + 1.0),
            TypeValue::String("A".into()),
            TypeValue::String("a".into()),
        ];

        let mut scramble = values.clone();
        scramble.sort();

        assert_eq!(values, scramble)
    }
}
