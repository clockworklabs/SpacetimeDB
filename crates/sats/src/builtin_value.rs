// pub mod encoding;
pub mod satn;

use crate::algebraic_value::AlgebraicValue;
use crate::builtin_type::BuiltinType;
use enum_as_inner::EnumAsInner;
use std::collections::BTreeMap;

/// Totally ordered [f32]
pub type F32 = decorum::Total<f32>;

/// Totally ordered [f64]
pub type F64 = decorum::Total<f64>;

#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum BuiltinValue {
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
    Array {
        val: Vec<AlgebraicValue>,
    },
    Map {
        val: BTreeMap<AlgebraicValue, AlgebraicValue>,
    },
}

impl crate::Value for BuiltinValue {
    type Type = BuiltinType;
}
