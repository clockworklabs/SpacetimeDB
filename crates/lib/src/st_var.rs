use derive_more::From;
use spacetimedb_sats::{AlgebraicValue, SpacetimeType, SumValue};

/// The value of a system variable in `st_var`.
/// Defined here because it is used in both the datastore and query.
#[derive(Debug, Clone, From, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub enum StVarValue {
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
    // No support for u/i256 added here as it seems unlikely to be useful.
    F32(f32),
    F64(f64),
    String(Box<str>),
}

impl StVarValue {
    pub fn try_from_primitive(value: AlgebraicValue) -> Result<Self, AlgebraicValue> {
        match value {
            AlgebraicValue::Bool(v) => Ok(StVarValue::Bool(v)),
            AlgebraicValue::I8(v) => Ok(StVarValue::I8(v)),
            AlgebraicValue::U8(v) => Ok(StVarValue::U8(v)),
            AlgebraicValue::I16(v) => Ok(StVarValue::I16(v)),
            AlgebraicValue::U16(v) => Ok(StVarValue::U16(v)),
            AlgebraicValue::I32(v) => Ok(StVarValue::I32(v)),
            AlgebraicValue::U32(v) => Ok(StVarValue::U32(v)),
            AlgebraicValue::I64(v) => Ok(StVarValue::I64(v)),
            AlgebraicValue::U64(v) => Ok(StVarValue::U64(v)),
            AlgebraicValue::I128(v) => Ok(StVarValue::I128(v.0)),
            AlgebraicValue::U128(v) => Ok(StVarValue::U128(v.0)),
            AlgebraicValue::F32(v) => Ok(StVarValue::F32(v.into_inner())),
            AlgebraicValue::F64(v) => Ok(StVarValue::F64(v.into_inner())),
            AlgebraicValue::String(v) => Ok(StVarValue::String(v)),
            _ => Err(value),
        }
    }

    pub fn try_from_sum(value: AlgebraicValue) -> Result<Self, AlgebraicValue> {
        value.into_sum()?.try_into()
    }
}

impl TryFrom<SumValue> for StVarValue {
    type Error = AlgebraicValue;

    fn try_from(sum: SumValue) -> Result<Self, Self::Error> {
        match sum.tag {
            0 => Ok(StVarValue::Bool(sum.value.into_bool()?)),
            1 => Ok(StVarValue::I8(sum.value.into_i8()?)),
            2 => Ok(StVarValue::U8(sum.value.into_u8()?)),
            3 => Ok(StVarValue::I16(sum.value.into_i16()?)),
            4 => Ok(StVarValue::U16(sum.value.into_u16()?)),
            5 => Ok(StVarValue::I32(sum.value.into_i32()?)),
            6 => Ok(StVarValue::U32(sum.value.into_u32()?)),
            7 => Ok(StVarValue::I64(sum.value.into_i64()?)),
            8 => Ok(StVarValue::U64(sum.value.into_u64()?)),
            9 => Ok(StVarValue::I128(sum.value.into_i128()?.0)),
            10 => Ok(StVarValue::U128(sum.value.into_u128()?.0)),
            11 => Ok(StVarValue::F32(sum.value.into_f32()?.into_inner())),
            12 => Ok(StVarValue::F64(sum.value.into_f64()?.into_inner())),
            13 => Ok(StVarValue::String(sum.value.into_string()?)),
            _ => Err(*sum.value),
        }
    }
}

impl From<StVarValue> for AlgebraicValue {
    fn from(value: StVarValue) -> Self {
        AlgebraicValue::Sum(value.into())
    }
}

impl From<StVarValue> for SumValue {
    fn from(value: StVarValue) -> Self {
        match value {
            StVarValue::Bool(v) => SumValue {
                tag: 0,
                value: Box::new(AlgebraicValue::Bool(v)),
            },
            StVarValue::I8(v) => SumValue {
                tag: 1,
                value: Box::new(AlgebraicValue::I8(v)),
            },
            StVarValue::U8(v) => SumValue {
                tag: 2,
                value: Box::new(AlgebraicValue::U8(v)),
            },
            StVarValue::I16(v) => SumValue {
                tag: 3,
                value: Box::new(AlgebraicValue::I16(v)),
            },
            StVarValue::U16(v) => SumValue {
                tag: 4,
                value: Box::new(AlgebraicValue::U16(v)),
            },
            StVarValue::I32(v) => SumValue {
                tag: 5,
                value: Box::new(AlgebraicValue::I32(v)),
            },
            StVarValue::U32(v) => SumValue {
                tag: 6,
                value: Box::new(AlgebraicValue::U32(v)),
            },
            StVarValue::I64(v) => SumValue {
                tag: 7,
                value: Box::new(AlgebraicValue::I64(v)),
            },
            StVarValue::U64(v) => SumValue {
                tag: 8,
                value: Box::new(AlgebraicValue::U64(v)),
            },
            StVarValue::I128(v) => SumValue {
                tag: 9,
                value: Box::new(AlgebraicValue::I128(v.into())),
            },
            StVarValue::U128(v) => SumValue {
                tag: 10,
                value: Box::new(AlgebraicValue::U128(v.into())),
            },
            StVarValue::F32(v) => SumValue {
                tag: 11,
                value: Box::new(AlgebraicValue::F32(v.into())),
            },
            StVarValue::F64(v) => SumValue {
                tag: 12,
                value: Box::new(AlgebraicValue::F64(v.into())),
            },
            StVarValue::String(v) => SumValue {
                tag: 13,
                value: Box::new(AlgebraicValue::String(v)),
            },
        }
    }
}
