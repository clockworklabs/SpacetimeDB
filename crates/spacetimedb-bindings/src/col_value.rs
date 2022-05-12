use super::col_type::ColType;
use std::fmt::Display;

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug, PartialOrd, Ord)]
pub enum ColValue {
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
    //F32(f32),
    //F64(f64),
}

impl ColValue {
    pub fn col_type(&self) -> ColType {
        match self {
            ColValue::U8(_) => ColType::U8,
            ColValue::U16(_) => ColType::U16,
            ColValue::U32(_) => ColType::U32,
            ColValue::U64(_) => ColType::U64,
            ColValue::U128(_) => ColType::U128,
            ColValue::I8(_) => ColType::I8,
            ColValue::I16(_) => ColType::I16,
            ColValue::I32(_) => ColType::I32,
            ColValue::I64(_) => ColType::I64,
            ColValue::I128(_) => ColType::I128,
            ColValue::Bool(_) => ColType::Bool,
            //ColValue::F32(_) => ColType::F32,
            //ColValue::F64(_) => ColType::F64,
        }
    }

    pub fn from_data(col_type: &ColType, data: &[u8]) -> Self {
        match col_type {
            ColType::U8 => ColValue::U8(data[0]),
            ColType::U16 => {
                let mut dst = [0u8; 2];
                dst.copy_from_slice(data);
                ColValue::U16(u16::from_le_bytes(dst))
            }
            ColType::U32 => {
                let mut dst = [0u8; 4];
                dst.copy_from_slice(&data[0..4]);
                ColValue::U32(u32::from_le_bytes(dst))
            }
            ColType::U64 => {
                let mut dst = [0u8; 8];
                dst.copy_from_slice(data);
                ColValue::U64(u64::from_le_bytes(dst))
            }
            ColType::U128 => {
                let mut dst = [0u8; 16];
                dst.copy_from_slice(data);
                ColValue::U128(u128::from_le_bytes(dst))
            }
            ColType::I8 => ColValue::I8(data[0] as i8),
            ColType::I16 => {
                let mut dst = [0u8; 2];
                dst.copy_from_slice(data);
                ColValue::I16(i16::from_le_bytes(dst))
            }
            ColType::I32 => {
                let mut dst = [0u8; 4];
                dst.copy_from_slice(data);
                ColValue::I32(i32::from_le_bytes(dst))
            }
            ColType::I64 => {
                let mut dst = [0u8; 8];
                dst.copy_from_slice(data);
                ColValue::I64(i64::from_le_bytes(dst))
            }
            ColType::I128 => {
                let mut dst = [0u8; 16];
                dst.copy_from_slice(data);
                ColValue::I128(i128::from_le_bytes(dst))
            }
            ColType::Bool => ColValue::Bool(if data[0] == 0 { false } else { true }),
            // ColType::F32 => {
            //     let mut dst = [0u8; 4];
            //     dst.copy_from_slice(data);
            //     ColValue::F32(f32::from_le_bytes(dst))
            // },
            // ColType::F64 => {
            //     let mut dst = [0u8; 8];
            //     dst.copy_from_slice(data);
            //     ColValue::F64(f64::from_le_bytes(dst))
            // },
        }
    }

    pub fn to_data(&self) -> Vec<u8> {
        match self {
            ColValue::U8(x) => x.to_le_bytes().to_vec(),
            ColValue::U16(x) => x.to_le_bytes().to_vec(),
            ColValue::U32(x) => x.to_le_bytes().to_vec(),
            ColValue::U64(x) => x.to_le_bytes().to_vec(),
            ColValue::U128(x) => x.to_le_bytes().to_vec(),
            ColValue::I8(x) => x.to_le_bytes().to_vec(),
            ColValue::I16(x) => x.to_le_bytes().to_vec(),
            ColValue::I32(x) => x.to_le_bytes().to_vec(),
            ColValue::I64(x) => x.to_le_bytes().to_vec(),
            ColValue::I128(x) => x.to_le_bytes().to_vec(),
            ColValue::Bool(x) => (if *x { 1 as u8 } else { 0 as u8 }).to_le_bytes().to_vec(),
            // ColValue::F32(x) => x.to_le_bytes().to_vec(),
            // ColValue::F64(x) => x.to_le_bytes().to_vec(),
        }
    }
}

impl Display for ColValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColValue::U8(v) => {
                write!(f, "{}", *v)
            }
            ColValue::U16(v) => {
                write!(f, "{}", *v)
            }
            ColValue::U32(v) => {
                write!(f, "{}", *v)
            }
            ColValue::U64(v) => {
                write!(f, "{}", *v)
            }
            ColValue::U128(v) => {
                write!(f, "{}", *v)
            }
            ColValue::I8(v) => {
                write!(f, "{}", *v)
            }
            ColValue::I16(v) => {
                write!(f, "{}", *v)
            }
            ColValue::I32(v) => {
                write!(f, "{}", *v)
            }
            ColValue::I64(v) => {
                write!(f, "{}", *v)
            }
            ColValue::I128(v) => {
                write!(f, "{}", *v)
            }
            ColValue::Bool(v) => {
                write!(f, "{}", *v)
            }
        }
    }
}
