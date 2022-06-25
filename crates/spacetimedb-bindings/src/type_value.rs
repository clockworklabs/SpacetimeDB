use crate::type_def::{EnumDef, TupleDef, TypeDef};
use enum_as_inner::EnumAsInner;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementValue {
    pub tag: u8,
    pub type_value: Box<TypeValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TupleValue {
    pub elements: Vec<TypeValue>,
}

impl TupleValue {
    pub fn decode(tuple_def: &TupleDef, bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let mut num_read = 0;
        let bytes = bytes.as_ref();

        let len = tuple_def.elements.len();

        let mut elements = Vec::new();
        for i in 0..len {
            // TODO: sort by tags or use the tags in some way or remove the tags from the def
            let type_def = &tuple_def.elements[i].element_type;
            let (type_value, nr) = TypeValue::decode(&type_def, &bytes[num_read..]);
            num_read += nr;
            elements.push(type_value);
        }

        let tuple_value = TupleValue { elements };
        (tuple_value, num_read)
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        for element in &self.elements {
            element.encode(bytes);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumValue {
    pub item_value: ElementValue,
}

impl EnumValue {
    pub fn decode(enum_def: &EnumDef, bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        let tag = bytes[num_read];
        num_read += 1;

        let mut i = 0;
        let type_def = loop {
            let item = &enum_def.items[i];
            if item.tag == tag {
                break &item.element_type;
            }
            i += 1;
        };
        let (type_value, nr) = TypeValue::decode(&type_def, &bytes[num_read..]);
        num_read += nr;

        let item_value = ElementValue {
            tag,
            type_value: Box::new(type_value),
        };
        (EnumValue { item_value }, num_read)
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.item_value.tag);
        self.item_value.type_value.encode(bytes);
    }
}

// TODO: Clone copies :(
#[derive(EnumAsInner, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EqTypeValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Bool(bool),
    String(String),
    Unit,
}

impl TryFrom<TypeValue> for EqTypeValue {
    type Error = &'static str;

    fn try_from(value: TypeValue) -> Result<Self, Self::Error> {
        match value {
            TypeValue::Tuple(_) => Err("Tuples are not equatable"),
            TypeValue::Enum(_) => Err("Enums are not equatable"),
            TypeValue::Vec(_) => Err("Vecs are not equatable"),
            TypeValue::U8(v) => Ok(Self::U8(v)),
            TypeValue::U16(v) => Ok(Self::U16(v)),
            TypeValue::U32(v) => Ok(Self::U32(v)),
            TypeValue::U64(v) => Ok(Self::U64(v)),
            TypeValue::U128(v) => Ok(Self::U128(v)),
            TypeValue::I8(v) => Ok(Self::I8(v)),
            TypeValue::I16(v) => Ok(Self::I16(v)),
            TypeValue::I32(v) => Ok(Self::I32(v)),
            TypeValue::I64(v) => Ok(Self::I64(v)),
            TypeValue::I128(v) => Ok(Self::I128(v)),
            TypeValue::Bool(v) => Ok(Self::Bool(v)),
            TypeValue::F32(_) => Err("Floats are not equatable"),
            TypeValue::F64(_) => Err("Floats are not equatable"),
            TypeValue::String(v) => Ok(Self::String(v)),
            TypeValue::Bytes(_) => Err("Bytes are not equatable"),
            TypeValue::Unit => Ok(Self::Unit),
        }
    }
}

impl TryFrom<&TypeValue> for EqTypeValue {
    type Error = &'static str;

    fn try_from(value: &TypeValue) -> Result<Self, Self::Error> {
        match value {
            TypeValue::Tuple(_) => Err("Tuples are not equatable"),
            TypeValue::Enum(_) => Err("Enums are not equatable"),
            TypeValue::Vec(_) => Err("Vecs are not equatable"),
            TypeValue::U8(v) => Ok(Self::U8(v.clone())),
            TypeValue::U16(v) => Ok(Self::U16(v.clone())),
            TypeValue::U32(v) => Ok(Self::U32(v.clone())),
            TypeValue::U64(v) => Ok(Self::U64(v.clone())),
            TypeValue::U128(v) => Ok(Self::U128(v.clone())),
            TypeValue::I8(v) => Ok(Self::I8(v.clone())),
            TypeValue::I16(v) => Ok(Self::I16(v.clone())),
            TypeValue::I32(v) => Ok(Self::I32(v.clone())),
            TypeValue::I64(v) => Ok(Self::I64(v.clone())),
            TypeValue::I128(v) => Ok(Self::I128(v.clone())),
            TypeValue::Bool(v) => Ok(Self::Bool(v.clone())),
            TypeValue::F32(_) => Err("Floats are not equatable"),
            TypeValue::F64(_) => Err("Floats are not equatable"),
            TypeValue::String(v) => Ok(Self::String(v.clone())),
            TypeValue::Bytes(_) => Err("Bytes are not equatable"),
            TypeValue::Unit => Ok(Self::Unit),
        }
    }
}

// TODO: Clone copies :(
#[derive(EnumAsInner, Debug, Clone, PartialEq, PartialOrd)]
pub enum RangeTypeValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    F32(f32),
    F64(f64),
    Bool(bool),
    String(String),
    Unit,
}

impl TryFrom<TypeValue> for RangeTypeValue {
    type Error = &'static str;

    fn try_from(value: TypeValue) -> Result<Self, Self::Error> {
        match value {
            TypeValue::Tuple(_) => Err("Tuples are not rangeable"),
            TypeValue::Enum(_) => Err("Enums are not rangeable"),
            TypeValue::Vec(_) => Err("Vecs are not rangeable"),
            TypeValue::U8(v) => Ok(Self::U8(v)),
            TypeValue::U16(v) => Ok(Self::U16(v)),
            TypeValue::U32(v) => Ok(Self::U32(v)),
            TypeValue::U64(v) => Ok(Self::U64(v)),
            TypeValue::U128(v) => Ok(Self::U128(v)),
            TypeValue::I8(v) => Ok(Self::I8(v)),
            TypeValue::I16(v) => Ok(Self::I16(v)),
            TypeValue::I32(v) => Ok(Self::I32(v)),
            TypeValue::I64(v) => Ok(Self::I64(v)),
            TypeValue::I128(v) => Ok(Self::I128(v)),
            TypeValue::Bool(v) => Ok(Self::Bool(v)),
            TypeValue::F32(v) => Ok(Self::F32(v)),
            TypeValue::F64(v) => Ok(Self::F64(v)),
            TypeValue::String(v) => Ok(Self::String(v)),
            TypeValue::Bytes(_) => Err("Bytes are not rangeable"),
            TypeValue::Unit => Ok(Self::Unit),
        }
    }
}

impl TryFrom<&TypeValue> for RangeTypeValue {
    type Error = &'static str;

    fn try_from(value: &TypeValue) -> Result<Self, Self::Error> {
        match value {
            TypeValue::Tuple(_) => Err("Tuples are not rangeable"),
            TypeValue::Enum(_) => Err("Enums are not rangeable"),
            TypeValue::Vec(_) => Err("Vecs are not rangeable"),
            TypeValue::U8(v) => Ok(Self::U8(v.clone())),
            TypeValue::U16(v) => Ok(Self::U16(v.clone())),
            TypeValue::U32(v) => Ok(Self::U32(v.clone())),
            TypeValue::U64(v) => Ok(Self::U64(v.clone())),
            TypeValue::U128(v) => Ok(Self::U128(v.clone())),
            TypeValue::I8(v) => Ok(Self::I8(v.clone())),
            TypeValue::I16(v) => Ok(Self::I16(v.clone())),
            TypeValue::I32(v) => Ok(Self::I32(v.clone())),
            TypeValue::I64(v) => Ok(Self::I64(v.clone())),
            TypeValue::I128(v) => Ok(Self::I128(v.clone())),
            TypeValue::Bool(v) => Ok(Self::Bool(v.clone())),
            TypeValue::F32(v) => Ok(Self::F32(v.clone())),
            TypeValue::F64(v) => Ok(Self::F64(v.clone())),
            TypeValue::String(v) => Ok(Self::String(v.clone())),
            TypeValue::Bytes(_) => Err("Bytes are not rangeable"),
            TypeValue::Unit => Ok(Self::Unit),
        }
    }
}

#[derive(EnumAsInner, Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TypeValue {
    Tuple(TupleValue),
    Enum(EnumValue),

    // base types
    Vec(Vec<TypeValue>),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Bool(bool),
    F32(f32),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
    Unit,
}

impl TypeValue {
    pub fn decode(type_def: &TypeDef, bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let bytes = bytes.as_ref();
        match type_def {
            TypeDef::Tuple(tuple_def) => {
                let (tuple, nr) = TupleValue::decode(tuple_def, &bytes[0..]);
                (TypeValue::Tuple(tuple), nr)
            }
            TypeDef::Enum(enum_def) => {
                let (enum_value, nr) = EnumValue::decode(enum_def, &bytes[0..]);
                (TypeValue::Enum(enum_value), nr)
            }
            TypeDef::Vec { element_type } => {
                //element_type.
                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                let mut num_read = 2;
                let len = u16::from_le_bytes(dst);
                let mut vec = Vec::new();
                for _ in 0..len {
                    let (value, nr) = TypeValue::decode(element_type, &bytes[num_read..]);
                    num_read += nr;
                    vec.push(value);
                }
                (TypeValue::Vec(vec), num_read)
            }
            TypeDef::U8 => (TypeValue::U8(bytes[0]), 1),
            TypeDef::U16 => {
                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                (TypeValue::U16(u16::from_le_bytes(dst)), 2)
            }
            TypeDef::U32 => {
                let mut dst = [0u8; 4];
                dst.copy_from_slice(&bytes[0..4]);
                (TypeValue::U32(u32::from_le_bytes(dst)), 4)
            }
            TypeDef::U64 => {
                let mut dst = [0u8; 8];
                dst.copy_from_slice(&bytes[0..8]);
                (TypeValue::U64(u64::from_le_bytes(dst)), 8)
            }
            TypeDef::U128 => {
                let mut dst = [0u8; 16];
                dst.copy_from_slice(&bytes[0..16]);
                (TypeValue::U128(u128::from_le_bytes(dst)), 16)
            }
            TypeDef::I8 => (TypeValue::I8(bytes[0] as i8), 1),
            TypeDef::I16 => {
                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                (TypeValue::I16(i16::from_le_bytes(dst)), 2)
            }
            TypeDef::I32 => {
                let mut dst = [0u8; 4];
                dst.copy_from_slice(&bytes[0..4]);
                (TypeValue::I32(i32::from_le_bytes(dst)), 4)
            }
            TypeDef::I64 => {
                let mut dst = [0u8; 8];
                dst.copy_from_slice(&bytes[0..8]);
                (TypeValue::I64(i64::from_le_bytes(dst)), 8)
            }
            TypeDef::I128 => {
                let mut dst = [0u8; 16];
                dst.copy_from_slice(&bytes[0..16]);
                (TypeValue::I128(i128::from_le_bytes(dst)), 16)
            }
            TypeDef::Bool => (TypeValue::Bool(if bytes[0] == 0 { false } else { true }), 1),
            TypeDef::F32 => {
                let mut dst = [0u8; 4];
                dst.copy_from_slice(&bytes[0..4]);
                (TypeValue::F32(f32::from_le_bytes(dst)), 4)
            }
            TypeDef::F64 => {
                let mut dst = [0u8; 8];
                dst.copy_from_slice(&bytes[0..8]);
                (TypeValue::F64(f64::from_le_bytes(dst)), 8)
            }
            TypeDef::String => {
                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                let mut num_read = 2;
                let len = u16::from_le_bytes(dst);
                let string = std::str::from_utf8(&bytes[num_read..num_read + (len as usize)]).unwrap();
                num_read += len as usize;
                (TypeValue::String(string.to_owned()), num_read)
            }
            TypeDef::Bytes => {
                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                let mut num_read = 2;
                let len = u16::from_le_bytes(dst);
                let output = &bytes[num_read..(num_read + (len as usize))];
                num_read += len as usize;
                (TypeValue::Bytes(output.to_owned()), num_read)
            }
            TypeDef::Unit => (TypeValue::Unit, 0),
        }
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
                bytes.extend(v.to_le_bytes());
            }
            TypeValue::F64(v) => {
                bytes.extend(v.to_le_bytes());
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

impl From<EqTypeValue> for TypeValue {
    fn from(value: EqTypeValue) -> Self {
        match value {
            EqTypeValue::U8(v) => Self::U8(v),
            EqTypeValue::U16(v) => Self::U16(v),
            EqTypeValue::U32(v) => Self::U32(v),
            EqTypeValue::U64(v) => Self::U64(v),
            EqTypeValue::U128(v) => Self::U128(v),
            EqTypeValue::I8(v) => Self::I8(v),
            EqTypeValue::I16(v) => Self::I16(v),
            EqTypeValue::I32(v) => Self::I32(v),
            EqTypeValue::I64(v) => Self::I64(v),
            EqTypeValue::I128(v) => Self::I128(v),
            EqTypeValue::Bool(v) => Self::Bool(v),
            EqTypeValue::String(v) => Self::String(v),
            EqTypeValue::Unit => Self::Unit,
        }
    }
}

impl From<&EqTypeValue> for TypeValue {
    fn from(value: &EqTypeValue) -> Self {
        match value {
            EqTypeValue::U8(v) => Self::U8(v.clone()),
            EqTypeValue::U16(v) => Self::U16(v.clone()),
            EqTypeValue::U32(v) => Self::U32(v.clone()),
            EqTypeValue::U64(v) => Self::U64(v.clone()),
            EqTypeValue::U128(v) => Self::U128(v.clone()),
            EqTypeValue::I8(v) => Self::I8(v.clone()),
            EqTypeValue::I16(v) => Self::I16(v.clone()),
            EqTypeValue::I32(v) => Self::I32(v.clone()),
            EqTypeValue::I64(v) => Self::I64(v.clone()),
            EqTypeValue::I128(v) => Self::I128(v.clone()),
            EqTypeValue::Bool(v) => Self::Bool(v.clone()),
            EqTypeValue::String(v) => Self::String(v.clone()),
            EqTypeValue::Unit => Self::Unit,
        }
    }
}

impl From<RangeTypeValue> for TypeValue {
    fn from(value: RangeTypeValue) -> Self {
        match value {
            RangeTypeValue::U8(v) => Self::U8(v),
            RangeTypeValue::U16(v) => Self::U16(v),
            RangeTypeValue::U32(v) => Self::U32(v),
            RangeTypeValue::U64(v) => Self::U64(v),
            RangeTypeValue::U128(v) => Self::U128(v),
            RangeTypeValue::I8(v) => Self::I8(v),
            RangeTypeValue::I16(v) => Self::I16(v),
            RangeTypeValue::I32(v) => Self::I32(v),
            RangeTypeValue::I64(v) => Self::I64(v),
            RangeTypeValue::I128(v) => Self::I128(v),
            RangeTypeValue::F32(v) => Self::F32(v),
            RangeTypeValue::F64(v) => Self::F64(v),
            RangeTypeValue::Bool(v) => Self::Bool(v),
            RangeTypeValue::String(v) => Self::String(v),
            RangeTypeValue::Unit => Self::Unit,
        }
    }
}

impl From<&RangeTypeValue> for TypeValue {
    fn from(value: &RangeTypeValue) -> Self {
        match value {
            RangeTypeValue::U8(v) => Self::U8(v.clone()),
            RangeTypeValue::U16(v) => Self::U16(v.clone()),
            RangeTypeValue::U32(v) => Self::U32(v.clone()),
            RangeTypeValue::U64(v) => Self::U64(v.clone()),
            RangeTypeValue::U128(v) => Self::U128(v.clone()),
            RangeTypeValue::I8(v) => Self::I8(v.clone()),
            RangeTypeValue::I16(v) => Self::I16(v.clone()),
            RangeTypeValue::I32(v) => Self::I32(v.clone()),
            RangeTypeValue::I64(v) => Self::I64(v.clone()),
            RangeTypeValue::I128(v) => Self::I128(v.clone()),
            RangeTypeValue::F32(v) => Self::F32(v.clone()),
            RangeTypeValue::F64(v) => Self::F64(v.clone()),
            RangeTypeValue::Bool(v) => Self::Bool(v.clone()),
            RangeTypeValue::String(v) => Self::String(v.clone()),
            RangeTypeValue::Unit => Self::Unit,
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
