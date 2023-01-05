use crate::algebraic_type::{
    self, AlgebraicType, TAG_ARRAY, TAG_F32, TAG_F64, TAG_I128, TAG_I16, TAG_I32, TAG_I64, TAG_I8, TAG_STRING,
    TAG_U128, TAG_U16, TAG_U32, TAG_U64, TAG_U8,
};
use algebraic_type::TAG_BOOL;
use enum_as_inner::EnumAsInner;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum BuiltinType {
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
    String, // Keep this because it is easy to just use Rust's String (utf-8)
    Array { ty: Box<AlgebraicType> },
}

pub struct SATNFormatter<'a> {
    ty: &'a BuiltinType,
}

impl<'a> SATNFormatter<'a> {
    pub fn new(ty: &'a BuiltinType) -> Self {
        Self { ty }
    }
}

impl<'a> Display for SATNFormatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ty {
            BuiltinType::Bool => write!(f, "Bool"),
            BuiltinType::I8 => write!(f, "I8"),
            BuiltinType::U8 => write!(f, "U8"),
            BuiltinType::I16 => write!(f, "I16"),
            BuiltinType::U16 => write!(f, "U16"),
            BuiltinType::I32 => write!(f, "I32"),
            BuiltinType::U32 => write!(f, "U32"),
            BuiltinType::I64 => write!(f, "I64"),
            BuiltinType::U64 => write!(f, "U64"),
            BuiltinType::I128 => write!(f, "I128"),
            BuiltinType::U128 => write!(f, "U128"),
            BuiltinType::F32 => write!(f, "F32"),
            BuiltinType::F64 => write!(f, "F64"),
            BuiltinType::String => write!(f, "String"),
            BuiltinType::Array { ty } => write!(f, "Array<{}>", algebraic_type::SATNFormatter::new(ty)),
        }
    }
}

impl BuiltinType {
    pub fn decode(bytes: impl AsRef<[u8]>) -> Result<(Self, usize), String> {
        let bytes = bytes.as_ref();
        if bytes.len() == 0 {
            return Err("Byte array length is invalid.".to_string());
        }
        match bytes[0] {
            TAG_BOOL => Ok((Self::Bool, 1)),
            TAG_I8 => Ok((Self::I8, 1)),
            TAG_U8 => Ok((Self::U8, 1)),
            TAG_I16 => Ok((Self::I16, 1)),
            TAG_U16 => Ok((Self::U16, 1)),
            TAG_I32 => Ok((Self::I32, 1)),
            TAG_U32 => Ok((Self::U32, 1)),
            TAG_I64 => Ok((Self::I64, 1)),
            TAG_U64 => Ok((Self::U64, 1)),
            TAG_I128 => Ok((Self::I128, 1)),
            TAG_U128 => Ok((Self::U128, 1)),
            TAG_F32 => Ok((Self::F32, 1)),
            TAG_F64 => Ok((Self::F64, 1)),
            TAG_STRING => Ok((Self::String, 1)),
            TAG_ARRAY => {
                let (ty, num_read) = AlgebraicType::decode(bytes)?;
                Ok((Self::Array { ty: Box::new(ty) }, num_read))
            }
            b => panic!("Unknown {}", b),
        }
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            BuiltinType::Bool => bytes.push(TAG_BOOL),
            BuiltinType::I8 => bytes.push(TAG_I8),
            BuiltinType::U8 => bytes.push(TAG_U8),
            BuiltinType::I16 => bytes.push(TAG_I16),
            BuiltinType::U16 => bytes.push(TAG_U16),
            BuiltinType::I32 => bytes.push(TAG_I32),
            BuiltinType::U32 => bytes.push(TAG_U32),
            BuiltinType::I64 => bytes.push(TAG_I64),
            BuiltinType::U64 => bytes.push(TAG_U64),
            BuiltinType::I128 => bytes.push(TAG_I128),
            BuiltinType::U128 => bytes.push(TAG_U128),
            BuiltinType::F32 => bytes.push(TAG_F32),
            BuiltinType::F64 => bytes.push(TAG_F64),
            BuiltinType::String => bytes.push(TAG_STRING),
            BuiltinType::Array { ty } => {
                bytes.push(TAG_ARRAY);
                ty.encode(bytes);
            }
        }
    }
}
