use crate::{Value, hash::hash_bytes};

use super::col_type::ColType;
use std::fmt::Display;
use enum_as_inner::EnumAsInner;
use super::hash::Hash;

pub trait ObjectResolver {
    fn get(&self, hash: Hash) -> Vec<u8>;
    fn add(&mut self, bytes: Vec<u8>) -> Hash;
}

#[derive(EnumAsInner, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntValue {
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
}

#[derive(EnumAsInner, Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum FloatValue {
    F32(f32),
    F64(f64)
}

/*
type ItemStack = (
    item_id: u32,
    quantity: u32 
)
type Pocket = (
    item_stack: ItemStack,
    volumn: u32 
)
type Inventory = (pockets: Vec<Pocket>)
type Position = (x: u32, y: u32, z: u32)
*/

// TODO!!!
// A ColValue should not contain any type information and really
// should just be a bag of bytes. Therefore, it would only be Eq, PartialEq,
// etc when interpreted in the context of a "type". "type"s should be
// general types which can be either sum types or product types and tables
// should just be relations of those generic types.
//
// Type = Enum | Tuple
// Tuple = (name: Type, ...)
// Enum = (Type | ...)
//
// Basically I'm saying I want to generalize tables to be sets of arbitrary algebraic types
//
// For now I am going to just assume basic types so that I can get this working
// but this should eventually be replaced with a more intelligent and 
// generic type system
#[derive(EnumAsInner, Debug, Clone, PartialEq, PartialOrd)]
pub enum ColValue {
    Integer(IntValue),
    Boolean(bool),
    Float(FloatValue),
    String(String),
    Bytes(Vec<u8>),
}

impl ColValue {
    pub fn decode(col_type: &ColType, bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let bytes = bytes.as_ref();
        match col_type {
            ColType::U8 => (ColValue::Integer(IntValue::U8(bytes[0])), 1),
            ColType::U16 => {
                let mut dst = [0u8; 2];
                dst.copy_from_slice(bytes);
                (ColValue::Integer(IntValue::U16(u16::from_le_bytes(dst))), 2)
            }
            ColType::U32 => {
                let mut dst = [0u8; 4];
                dst.copy_from_slice(bytes);
                (ColValue::Integer(IntValue::U32(u32::from_le_bytes(dst))), 4)
            }
            ColType::U64 => {
                let mut dst = [0u8; 8];
                dst.copy_from_slice(bytes);
                (ColValue::Integer(IntValue::U64(u64::from_le_bytes(dst))), 8)
            }
            ColType::U128 => {
                let mut dst = [0u8; 16];
                dst.copy_from_slice(bytes);
                (ColValue::Integer(IntValue::U128(u128::from_le_bytes(dst))), 16)
            }
            ColType::I8 => (ColValue::Integer(IntValue::I8(bytes[0] as i8)), 1),
            ColType::I16 => {
                let mut dst = [0u8; 2];
                dst.copy_from_slice(bytes);
                (ColValue::Integer(IntValue::I16(i16::from_le_bytes(dst))), 2)
            }
            ColType::I32 => {
                let mut dst = [0u8; 4];
                dst.copy_from_slice(bytes);
                (ColValue::Integer(IntValue::I32(i32::from_le_bytes(dst))), 4)
            }
            ColType::I64 => {
                let mut dst = [0u8; 8];
                dst.copy_from_slice(bytes);
                (ColValue::Integer(IntValue::I64(i64::from_le_bytes(dst))), 8)
            }
            ColType::I128 => {
                let mut dst = [0u8; 16];
                dst.copy_from_slice(bytes);
                (ColValue::Integer(IntValue::I128(i128::from_le_bytes(dst))), 16)
            }
            ColType::Bool => (ColValue::Boolean(if bytes[0] == 0 { false } else { true }), 1),
            ColType::F32 => {
                let mut dst = [0u8; 4];
                dst.copy_from_slice(bytes);
                (ColValue::Float(FloatValue::F32(f32::from_le_bytes(dst))), 4)
            },
            ColType::F64 => {
                let mut dst = [0u8; 8];
                dst.copy_from_slice(bytes);
                (ColValue::Float(FloatValue::F64(f64::from_le_bytes(dst))), 8)
            },
            ColType::String => {
                let (v, num_bytes) = Value::decode(bytes);
                match v {
                    Value::Data { len, buf } => {
                        let slice = &buf[0..len as usize];
                        let str = String::from_utf8(slice.to_vec()).unwrap();
                        (ColValue::String(str), num_bytes)
                    },
                    Value::Hash(h) => {
                        let data = object_db.get(h);
                        let str = String::from_utf8(data).unwrap();
                        (ColValue::String(str), num_bytes)
                    },
                }
            },
            ColType::Bytes => {
                let (v, num_bytes) = Value::decode(bytes);
                match v {
                    Value::Data { len, buf } => {
                        let slice = &buf[0..len as usize];
                        let data = slice.to_vec();
                        (ColValue::Bytes(data), num_bytes)
                    },
                    Value::Hash(h) => {
                        let data = object_db.get(h);
                        (ColValue::Bytes(data), num_bytes)
                    },
                }
            },
        }
    }

    pub fn encode(&self, object_db: &dyn ObjectResolver, bytes: &mut Vec<u8>) {
        match self {
            ColValue::Integer(IntValue::U8(x)) => bytes.copy_from_slice(&x.to_le_bytes()),
            ColValue::Integer(IntValue::U16(x)) => bytes.copy_from_slice(&x.to_le_bytes()),
            ColValue::Integer(IntValue::U32(x)) => bytes.copy_from_slice(&x.to_le_bytes()),
            ColValue::Integer(IntValue::U64(x)) => bytes.copy_from_slice(&x.to_le_bytes()),
            ColValue::Integer(IntValue::U128(x)) => bytes.copy_from_slice(&x.to_le_bytes()),
            ColValue::Integer(IntValue::I8(x)) => bytes.copy_from_slice(&x.to_le_bytes()),
            ColValue::Integer(IntValue::I16(x)) => bytes.copy_from_slice(&x.to_le_bytes()),
            ColValue::Integer(IntValue::I32(x)) => bytes.copy_from_slice(&x.to_le_bytes()),
            ColValue::Integer(IntValue::I64(x)) => bytes.copy_from_slice(&x.to_le_bytes()),
            ColValue::Integer(IntValue::I128(x)) => bytes.copy_from_slice(&x.to_le_bytes()),
            ColValue::Boolean(x) => bytes.copy_from_slice(&(if *x { 1 as u8 } else { 0 as u8 }).to_le_bytes()),
            ColValue::Float(FloatValue::F32(x)) => bytes.copy_from_slice(&x.to_le_bytes()),
            ColValue::Float(FloatValue::F64(x)) => bytes.copy_from_slice(&x.to_le_bytes()),
            ColValue::String(s) => {
                let sbytes = s.as_bytes();
                let v = if sbytes.len() > 32 {
                    object_db.add(sbytes.to_vec());
                    Value::Hash(hash_bytes(sbytes))
                } else {
                    let buf = [0; 32];
                    buf.copy_from_slice(sbytes);
                    Value::Data { len: sbytes.len() as u8, buf, }
                };
            },
            ColValue::Bytes(sbytes) => {
                let v = if sbytes.len() > 32 {
                    Value::Hash(hash_bytes(sbytes))
                } else {
                    let buf = [0; 32];
                    buf.copy_from_slice(sbytes);
                    Value::Data { len: sbytes.len() as u8, buf, }
                };
            },
        };
    }

}

impl ColValue {
    pub fn col_type(&self) -> ColType {
        match self {
            ColValue::Integer(i) => {
                match i {
                    IntValue::U8(_) => ColType::U8,
                    IntValue::U16(_) => ColType::U16,
                    IntValue::U32(_) => todo!(),
                    IntValue::U64(_) => todo!(),
                    IntValue::U128(_) => todo!(),
                    IntValue::I8(_) => todo!(),
                    IntValue::I16(_) => todo!(),
                    IntValue::I32(_) => todo!(),
                    IntValue::I64(_) => todo!(),
                    IntValue::I128(_) => todo!(),
                }
            },
            ColValue::Boolean(b) => ColType::Bool,
            ColValue::Float(f) => {
                match f {
                    FloatValue::F32(_) => ColType::F32,
                    FloatValue::F64(_) => ColType::F64,
                }
            },
            ColValue::String(s) => ColType::String,
            ColValue::Bytes(b) => ColType::Bytes,
        }
    }

}

impl Display for ColValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColValue::Integer(IntValue::U8(v)) => {
                write!(f, "{}", *v)
            }
            ColValue::Integer(IntValue::U16(v)) => {
                write!(f, "{}", *v)
            }
            ColValue::Integer(IntValue::U32(v)) => {
                write!(f, "{}", *v)
            }
            ColValue::Integer(IntValue::U64(v)) => {
                write!(f, "{}", *v)
            }
            ColValue::Integer(IntValue::U128(v)) => {
                write!(f, "{}", *v)
            }
            ColValue::Integer(IntValue::I8(v)) => {
                write!(f, "{}", *v)
            }
            ColValue::Integer(IntValue::I16(v)) => {
                write!(f, "{}", *v)
            }
            ColValue::Integer(IntValue::I32(v)) => {
                write!(f, "{}", *v)
            }
            ColValue::Integer(IntValue::I64(v)) => {
                write!(f, "{}", *v)
            }
            ColValue::Integer(IntValue::I128(v)) => {
                write!(f, "{}", *v)
            }
            ColValue::Boolean(v) => {
                write!(f, "{}", *v)
            }
            ColValue::Float(FloatValue::F32(v)) => {
                write!(f, "{}", *v)
            }
            ColValue::Float(FloatValue::F64(v)) => {
                write!(f, "{}", *v)
            }
            ColValue::String(v) => {
                write!(f, "{}", v)
            },
            ColValue::Bytes(v) => {
                write!(f, "{:?}", v)
            },
        }
    }
}
