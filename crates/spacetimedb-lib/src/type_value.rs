use crate::{
    type_def::{EnumDef, PrimitiveType, TupleDef, TypeDef},
    DataKey,
};
use enum_as_inner::EnumAsInner;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::mem::size_of;
use std::{fmt::Display, hash::Hash};

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

impl Display for TupleValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

impl Hash for TupleValue {
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

    pub fn decode(tuple_def: &TupleDef, bytes: impl AsRef<[u8]>) -> (Result<Self, &'static str>, usize) {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        let len = tuple_def.elements.len();

        let mut elements = Vec::with_capacity(len);
        for i in 0..len {
            // TODO: sort by tags or use the tags in some way or remove the tags from the def
            let type_def = &tuple_def.elements[i].element_type;
            let (type_value, nr) = TypeValue::decode(&type_def, &bytes[num_read..]);
            if let Err(e) = type_value {
                return (Err(e), 0);
            }
            num_read += nr;
            elements.push(type_value.unwrap());
        }

        let tuple_value = TupleValue {
            elements: elements.into(),
        };
        (Ok(tuple_value), num_read)
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        for element in &self.elements[..] {
            element.encode(bytes);
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct EnumValue {
    pub element_value: ElementValue,
}

impl EnumValue {
    pub fn decode(enum_def: &EnumDef, bytes: impl AsRef<[u8]>) -> (Result<Self, &'static str>, usize) {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        if bytes.len() == 0 {
            return (Err("EnumValue::decode: Byte array length is invalid."), 0);
        }
        let tag = bytes[num_read];
        num_read += 1;

        let mut i = 0;
        let type_def = loop {
            let item = &enum_def.elements[i];
            if item.tag == tag {
                break &item.element_type;
            }
            i += 1;
        };
        let (type_value, nr) = TypeValue::decode(&type_def, &bytes[num_read..]);
        if let Err(e) = type_value {
            return (Err(e), 0);
        }
        num_read += nr;

        let item_value = ElementValue {
            tag,
            type_value: Box::new(type_value.unwrap()),
        };
        (
            Ok(EnumValue {
                element_value: item_value,
            }),
            num_read,
        )
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.element_value.tag);
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
            TypeValue::Enum(x) => TypeWideValue::Enum(x),
            TypeValue::Tuple(x) => TypeWideValue::Vec(&x.elements),
            TypeValue::Vec(x) => TypeWideValue::Vec(&x),
        }
    }

    pub fn decode(type_def: &TypeDef, bytes: impl AsRef<[u8]>) -> (Result<Self, &'static str>, usize) {
        let bytes = bytes.as_ref();
        let result = match type_def {
            TypeDef::Tuple(tuple_def) => {
                let (tuple, nr) = TupleValue::decode(tuple_def, &bytes[0..]);
                if let Err(e) = tuple {
                    return (Err(e), 0);
                }
                (TypeValue::Tuple(tuple.unwrap()), nr)
            }
            TypeDef::Enum(enum_def) => {
                let (enum_value, nr) = EnumValue::decode(enum_def, &bytes[0..]);
                if let Err(e) = enum_value {
                    return (Err(e), 0);
                }
                (TypeValue::Enum(enum_value.unwrap()), nr)
            }
            TypeDef::Vec { element_type } => {
                if bytes.len() < 2 {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode vec."),
                        0,
                    );
                }

                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                let mut num_read = 2;
                let len = u16::from_le_bytes(dst);
                let mut vec = Vec::new();
                for _ in 0..len {
                    if bytes.len() <= num_read {
                        return (
                            Err("TypeValue::decode: buffer has no room to decode any more elements from this vec."),
                            0,
                        );
                    }

                    let (value, nr) = TypeValue::decode(element_type, &bytes[num_read..]);
                    num_read += nr;
                    if let Err(e) = value {
                        return (Err(e), 0);
                    }
                    vec.push(value.unwrap());
                }
                (TypeValue::Vec(vec), num_read)
            }
            TypeDef::Primitive(PrimitiveType::U8) => {
                if bytes.len() < size_of::<u8>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode U8."),
                        0,
                    );
                }
                (TypeValue::U8(bytes[0]), 1)
            }
            TypeDef::Primitive(PrimitiveType::U16) => {
                if bytes.len() < size_of::<u16>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode U16."),
                        0,
                    );
                }
                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                (TypeValue::U16(u16::from_le_bytes(dst)), 2)
            }
            TypeDef::Primitive(PrimitiveType::U32) => {
                if bytes.len() < size_of::<u32>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode U32."),
                        0,
                    );
                }
                let mut dst = [0u8; 4];
                dst.copy_from_slice(&bytes[0..4]);
                (TypeValue::U32(u32::from_le_bytes(dst)), 4)
            }
            TypeDef::Primitive(PrimitiveType::U64) => {
                if bytes.len() < size_of::<u64>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode U64."),
                        0,
                    );
                }
                let mut dst = [0u8; 8];
                dst.copy_from_slice(&bytes[0..8]);
                (TypeValue::U64(u64::from_le_bytes(dst)), 8)
            }
            TypeDef::Primitive(PrimitiveType::U128) => {
                if bytes.len() < size_of::<u128>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode U128."),
                        0,
                    );
                }
                let mut dst = [0u8; 16];
                dst.copy_from_slice(&bytes[0..16]);
                (TypeValue::U128(u128::from_le_bytes(dst)), 16)
            }
            TypeDef::Primitive(PrimitiveType::I8) => {
                if bytes.len() < size_of::<i8>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode I8."),
                        0,
                    );
                }
                (TypeValue::I8(bytes[0] as i8), 1)
            }
            TypeDef::Primitive(PrimitiveType::I16) => {
                if bytes.len() < size_of::<i16>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode I16."),
                        0,
                    );
                }
                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                (TypeValue::I16(i16::from_le_bytes(dst)), 2)
            }
            TypeDef::Primitive(PrimitiveType::I32) => {
                if bytes.len() < size_of::<i32>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode I32."),
                        0,
                    );
                }
                let mut dst = [0u8; 4];
                dst.copy_from_slice(&bytes[0..4]);
                (TypeValue::I32(i32::from_le_bytes(dst)), 4)
            }
            TypeDef::Primitive(PrimitiveType::I64) => {
                if bytes.len() < size_of::<i64>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode I64."),
                        0,
                    );
                }
                let mut dst = [0u8; 8];
                dst.copy_from_slice(&bytes[0..8]);
                (TypeValue::I64(i64::from_le_bytes(dst)), 8)
            }
            TypeDef::Primitive(PrimitiveType::I128) => {
                if bytes.len() < size_of::<i128>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode I128."),
                        0,
                    );
                }
                let mut dst = [0u8; 16];
                dst.copy_from_slice(&bytes[0..16]);
                (TypeValue::I128(i128::from_le_bytes(dst)), 16)
            }
            TypeDef::Primitive(PrimitiveType::Bool) => {
                if bytes.len() < size_of::<bool>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode Bool."),
                        0,
                    );
                }
                (TypeValue::Bool(if bytes[0] == 0 { false } else { true }), 1)
            }
            TypeDef::Primitive(PrimitiveType::F32) => {
                if bytes.len() < size_of::<f32>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode F32."),
                        0,
                    );
                }
                let mut dst = [0u8; 4];
                dst.copy_from_slice(&bytes[0..4]);
                (TypeValue::F32(F32::from(f32::from_le_bytes(dst))), 4)
            }
            TypeDef::Primitive(PrimitiveType::F64) => {
                if bytes.len() < size_of::<f64>() {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to decode F64."),
                        0,
                    );
                }
                let mut dst = [0u8; 8];
                dst.copy_from_slice(&bytes[0..8]);
                (TypeValue::F64(F64::from(f64::from_le_bytes(dst))), 8)
            }
            TypeDef::Primitive(PrimitiveType::String) => {
                if bytes.len() < 2 {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to get length of string."),
                        0,
                    );
                }
                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                let mut num_read = 2;
                let len = u16::from_le_bytes(dst);
                if bytes.len() - 2 < len as usize {
                    return (
                        Err("TypeValue::decode: Cannot decode string, buffer not long enough."),
                        0,
                    );
                }

                let string = std::str::from_utf8(&bytes[num_read..num_read + (len as usize)]).unwrap();
                num_read += len as usize;
                (TypeValue::String(string.to_owned()), num_read)
            }
            TypeDef::Primitive(PrimitiveType::Bytes) => {
                if bytes.len() < 2 {
                    return (
                        Err("TypeValue::decode: byte array length not long enough to get length of byte array."),
                        0,
                    );
                }
                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                let mut num_read = 2;
                let len = u16::from_le_bytes(dst);
                if bytes.len() - 2 < len as usize {
                    return (
                        Err("TypeValue::decode: Cannot decode byte array, buffer not long enough."),
                        0,
                    );
                }
                let output = &bytes[num_read..(num_read + (len as usize))];
                num_read += len as usize;
                (TypeValue::Bytes(output.to_owned()), num_read)
            }
            TypeDef::Primitive(PrimitiveType::Unit) => (TypeValue::Unit, 0),
            TypeDef::Ref(x) => match *x {},
        };

        (Ok(result.0), result.1)
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            TypeValue::Tuple(v) => {
                v.encode(bytes);
            }
            TypeValue::Enum(v) => {
                v.encode(bytes);
            }
            TypeValue::Vec(v) => {
                let len = v.len() as u16;
                bytes.extend(len.to_le_bytes());
                for val in v {
                    val.encode(bytes);
                }
            }
            TypeValue::U8(v) => {
                bytes.push(*v);
            }
            TypeValue::U16(v) => {
                bytes.extend(v.to_le_bytes());
            }
            TypeValue::U32(v) => {
                bytes.extend(v.to_le_bytes());
            }
            TypeValue::U64(v) => {
                bytes.extend(v.to_le_bytes());
            }
            TypeValue::U128(v) => {
                bytes.extend(v.to_le_bytes());
            }
            TypeValue::I8(v) => {
                bytes.extend(v.to_le_bytes());
            }
            TypeValue::I16(v) => {
                bytes.extend(v.to_le_bytes());
            }
            TypeValue::I32(v) => {
                bytes.extend(v.to_le_bytes());
            }
            TypeValue::I64(v) => {
                bytes.extend(v.to_le_bytes());
            }
            TypeValue::I128(v) => {
                bytes.extend(v.to_le_bytes());
            }
            TypeValue::Bool(v) => {
                bytes.push(if *v { 1 } else { 0 });
            }
            TypeValue::F32(v) => {
                bytes.extend(v.into_inner().to_le_bytes());
            }
            TypeValue::F64(v) => {
                bytes.extend(v.into_inner().to_le_bytes());
            }
            TypeValue::String(v) => {
                let len = v.len() as u16;
                bytes.extend(len.to_le_bytes());
                bytes.extend(v.as_bytes());
            }
            TypeValue::Bytes(v) => {
                let len = v.len() as u16;
                bytes.extend(len.to_le_bytes());
                bytes.extend(v);
            }
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

impl Display for TypeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
