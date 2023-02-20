use super::BuiltinValue;
use crate::{
    algebraic_value::AlgebraicValue,
    builtin_type::BuiltinType,
    builtin_value::{F32, F64},
    MapType,
};
use std::mem::size_of;

impl BuiltinValue {
    pub fn decode(ty: &BuiltinType, bytes: impl AsRef<[u8]>) -> Result<(Self, usize), &'static str> {
        let bytes = bytes.as_ref();
        match ty {
            BuiltinType::Bool => {
                const SIZE: usize = size_of::<bool>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode Bool.");
                }
                Ok((Self::Bool(if bytes[0] == 0 { false } else { true }), SIZE))
            }
            BuiltinType::I8 => {
                const SIZE: usize = size_of::<i8>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode I8.");
                }
                let mut dst = [0u8; SIZE];
                dst.copy_from_slice(&bytes[0..SIZE]);
                Ok((Self::I8(i8::from_le_bytes(dst)), SIZE))
            }
            BuiltinType::U8 => {
                const SIZE: usize = size_of::<u8>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode U8.");
                }
                let mut dst = [0u8; SIZE];
                dst.copy_from_slice(&bytes[0..SIZE]);
                Ok((Self::U8(u8::from_le_bytes(dst)), SIZE))
            }
            BuiltinType::I16 => {
                const SIZE: usize = size_of::<i16>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode I16.");
                }
                let mut dst = [0u8; SIZE];
                dst.copy_from_slice(&bytes[0..SIZE]);
                Ok((Self::I16(i16::from_le_bytes(dst)), SIZE))
            }
            BuiltinType::U16 => {
                const SIZE: usize = size_of::<u16>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode U16.");
                }
                let mut dst = [0u8; SIZE];
                dst.copy_from_slice(&bytes[0..SIZE]);
                Ok((Self::U16(u16::from_le_bytes(dst)), SIZE))
            }
            BuiltinType::I32 => {
                const SIZE: usize = size_of::<i32>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode I32.");
                }
                let mut dst = [0u8; SIZE];
                dst.copy_from_slice(&bytes[0..SIZE]);
                Ok((Self::I32(i32::from_le_bytes(dst)), SIZE))
            }
            BuiltinType::U32 => {
                const SIZE: usize = size_of::<u32>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode U32.");
                }
                let mut dst = [0u8; SIZE];
                dst.copy_from_slice(&bytes[0..SIZE]);
                Ok((Self::U32(u32::from_le_bytes(dst)), SIZE))
            }
            BuiltinType::I64 => {
                const SIZE: usize = size_of::<i64>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode I64.");
                }
                let mut dst = [0u8; SIZE];
                dst.copy_from_slice(&bytes[0..SIZE]);
                Ok((Self::I64(i64::from_le_bytes(dst)), SIZE))
            }
            BuiltinType::U64 => {
                const SIZE: usize = size_of::<u64>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode U64.");
                }
                let mut dst = [0u8; SIZE];
                dst.copy_from_slice(&bytes[0..SIZE]);
                Ok((Self::U64(u64::from_le_bytes(dst)), SIZE))
            }
            BuiltinType::I128 => {
                const SIZE: usize = size_of::<i128>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode I128.");
                }
                let mut dst = [0u8; SIZE];
                dst.copy_from_slice(&bytes[0..SIZE]);
                Ok((Self::I128(i128::from_le_bytes(dst)), SIZE))
            }
            BuiltinType::U128 => {
                const SIZE: usize = size_of::<u128>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode U128.");
                }
                let mut dst = [0u8; SIZE];
                dst.copy_from_slice(&bytes[0..SIZE]);
                Ok((Self::U128(u128::from_le_bytes(dst)), SIZE))
            }
            BuiltinType::F32 => {
                const SIZE: usize = size_of::<f32>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode F32.");
                }
                let mut dst = [0u8; SIZE];
                dst.copy_from_slice(&bytes[0..SIZE]);
                Ok((Self::F32(F32::from(f32::from_le_bytes(dst))), SIZE))
            }
            BuiltinType::F64 => {
                const SIZE: usize = size_of::<f64>();
                if bytes.len() < SIZE {
                    return Err("Byte array length not long enough to decode F64.");
                }
                let mut dst = [0u8; SIZE];
                dst.copy_from_slice(&bytes[0..SIZE]);
                Ok((Self::F64(F64::from(f64::from_le_bytes(dst))), SIZE))
            }
            BuiltinType::String => {
                if bytes.len() < 2 {
                    return Err("Byte array length not long enough to get length of string.");
                }
                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                let mut num_read = 2;
                let len = u16::from_le_bytes(dst);
                if bytes.len() - 2 < len as usize {
                    return Err("Cannot decode string, buffer not long enough.");
                }
                let string = std::str::from_utf8(&bytes[num_read..num_read + (len as usize)]).unwrap();
                num_read += len as usize;
                Ok((Self::String(string.to_owned()), num_read))
            }
            BuiltinType::Array { ty } => {
                if bytes.len() < 2 {
                    return Err("Byte array length not long enough to decode Array.");
                }

                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                let mut num_read = 2;
                let len = u16::from_le_bytes(dst);
                let mut vec = Vec::new();
                for _ in 0..len {
                    if bytes.len() <= num_read {
                        return Err("Buffer has no room to decode any more elements from this Array.");
                    }

                    let (value, nr) = AlgebraicValue::decode(ty, &bytes[num_read..])?;
                    num_read += nr;
                    vec.push(value);
                }
                Ok((Self::Array { val: vec }, num_read))
            }
            BuiltinType::Map(MapType { key_ty, ty }) => {
                if bytes.len() < 2 {
                    return Err("Byte array length not long enough to decode Array.");
                }

                let mut dst = [0u8; 2];
                dst.copy_from_slice(&bytes[0..2]);
                let mut num_read = 2;
                let len = u16::from_le_bytes(dst);
                let mut tuples = Vec::new();
                for _ in 0..len {
                    if bytes.len() <= num_read {
                        return Err("Buffer has no room to decode any more elements from this Array.");
                    }

                    let (key, nr) = AlgebraicValue::decode(key_ty, &bytes[num_read..])?;
                    num_read += nr;
                    let (value, nr) = AlgebraicValue::decode(ty, &bytes[num_read..])?;
                    num_read += nr;
                    tuples.push((key, value));
                }
                Ok((
                    Self::Map {
                        val: tuples.into_iter().collect(),
                    },
                    num_read,
                ))
            }
        }
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            BuiltinValue::Bool(v) => {
                bytes.push(if *v { 1 } else { 0 });
            }
            BuiltinValue::I8(v) => {
                bytes.extend(v.to_le_bytes());
            }
            BuiltinValue::U8(v) => {
                bytes.extend(v.to_le_bytes());
            }
            BuiltinValue::I16(v) => {
                bytes.extend(v.to_le_bytes());
            }
            BuiltinValue::U16(v) => {
                bytes.extend(v.to_le_bytes());
            }
            BuiltinValue::I32(v) => {
                bytes.extend(v.to_le_bytes());
            }
            BuiltinValue::U32(v) => {
                bytes.extend(v.to_le_bytes());
            }
            BuiltinValue::I64(v) => {
                bytes.extend(v.to_le_bytes());
            }
            BuiltinValue::U64(v) => {
                bytes.extend(v.to_le_bytes());
            }
            BuiltinValue::I128(v) => {
                bytes.extend(v.to_le_bytes());
            }
            BuiltinValue::U128(v) => {
                bytes.extend(v.to_le_bytes());
            }
            BuiltinValue::F32(v) => {
                bytes.extend(v.into_inner().to_le_bytes());
            }
            BuiltinValue::F64(v) => {
                bytes.extend(v.into_inner().to_le_bytes());
            }
            BuiltinValue::String(v) => {
                let len = v.len() as u16;
                bytes.extend(len.to_le_bytes());
                bytes.extend(v.as_bytes());
            }
            BuiltinValue::Bytes(v) => {
                let len = v.len() as u16;
                bytes.extend(len.to_le_bytes());
                bytes.extend(v);
            }
            BuiltinValue::Array { val } => {
                let len = val.len() as u16;
                bytes.extend(len.to_le_bytes());
                for v in val {
                    v.encode(bytes);
                }
            }
            BuiltinValue::Map { val } => {
                let len = val.len() as u16;
                bytes.extend(len.to_le_bytes());
                for (k, v) in val {
                    k.encode(bytes);
                    v.encode(bytes);
                }
            }
        }
    }
}
