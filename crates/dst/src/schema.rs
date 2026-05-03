//! Shared schema and row model used by DST targets.

use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductValue};

use crate::seed::DstRng;

/// Generated schema for one simulator case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaPlan {
    /// User-visible tables installed before the workload starts.
    pub tables: Vec<TablePlan>,
}

/// Table definition used by simulators.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TablePlan {
    /// Stable logical table name used in generated interactions and assertions.
    pub name: String,
    /// Ordered column definitions. Column 0 is treated as the primary id column.
    pub columns: Vec<ColumnPlan>,
    /// Additional indexed column sets beyond the implicit primary id index.
    ///
    /// A value like `[1]` means a single-column secondary index on column 1.
    /// A value like `[0, 1]` means a composite btree index over columns 0 and 1.
    pub extra_indexes: Vec<Vec<u16>>,
}

/// Column definition used by simulators.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColumnPlan {
    /// Column name installed into the target schema.
    pub name: String,
    /// Algebraic type for generated values in this column.
    pub ty: AlgebraicType,
}

/// Serializable row representation used by generated interactions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimRow {
    /// Column values in schema order.
    pub values: Vec<AlgebraicValue>,
}

pub fn generate_supported_type(rng: &mut DstRng) -> AlgebraicType {
    match rng.index(12) {
        0 => AlgebraicType::Bool,
        1 => AlgebraicType::I8,
        2 => AlgebraicType::U8,
        3 => AlgebraicType::I16,
        4 => AlgebraicType::U16,
        5 => AlgebraicType::I32,
        6 => AlgebraicType::U32,
        7 => AlgebraicType::I64,
        8 => AlgebraicType::U64,
        9 => AlgebraicType::I128,
        10 => AlgebraicType::U128,
        _ => AlgebraicType::String,
    }
}

pub fn generate_value_for_type(rng: &mut DstRng, ty: &AlgebraicType, idx: usize) -> AlgebraicValue {
    if rng.index(5) == 0 {
        return edge_value_for_type(rng, ty, idx);
    }

    match ty {
        AlgebraicType::Bool => AlgebraicValue::Bool(rng.index(2) == 0),
        AlgebraicType::I8 => AlgebraicValue::I8(((rng.next_u64() % 64) as i8) - 32),
        AlgebraicType::U8 => AlgebraicValue::U8((rng.next_u64() % u8::MAX as u64) as u8),
        AlgebraicType::I16 => AlgebraicValue::I16(((rng.next_u64() % 2048) as i16) - 1024),
        AlgebraicType::U16 => AlgebraicValue::U16((rng.next_u64() % u16::MAX as u64) as u16),
        AlgebraicType::I32 => AlgebraicValue::I32(((rng.next_u64() % 200_000) as i32) - 100_000),
        AlgebraicType::U32 => AlgebraicValue::U32((rng.next_u64() % 1_000_000) as u32),
        AlgebraicType::I64 => AlgebraicValue::I64(((rng.next_u64() % 2_000_000) as i64) - 1_000_000),
        AlgebraicType::U64 => AlgebraicValue::U64((rng.next_u64() % 1000) + idx as u64),
        AlgebraicType::I128 => {
            let v = ((rng.next_u64() % 2_000_000) as i128) - 1_000_000;
            AlgebraicValue::I128(v.into())
        }
        AlgebraicType::U128 => {
            let v = (rng.next_u64() % 2_000_000) as u128;
            AlgebraicValue::U128(v.into())
        }
        AlgebraicType::String => AlgebraicValue::String(format!("v{}_{}", idx, rng.next_u64() % 10_000).into()),
        other => panic!("unsupported generated column type: {other:?}"),
    }
}

pub fn default_value_for_type(ty: &AlgebraicType) -> AlgebraicValue {
    match ty {
        AlgebraicType::Bool => AlgebraicValue::Bool(false),
        AlgebraicType::I8 => AlgebraicValue::I8(0),
        AlgebraicType::U8 => AlgebraicValue::U8(0),
        AlgebraicType::I16 => AlgebraicValue::I16(0),
        AlgebraicType::U16 => AlgebraicValue::U16(0),
        AlgebraicType::I32 => AlgebraicValue::I32(0),
        AlgebraicType::U32 => AlgebraicValue::U32(0),
        AlgebraicType::I64 => AlgebraicValue::I64(0),
        AlgebraicType::U64 => AlgebraicValue::U64(0),
        AlgebraicType::I128 => AlgebraicValue::I128(0.into()),
        AlgebraicType::U128 => AlgebraicValue::U128(0.into()),
        AlgebraicType::String => AlgebraicValue::String("".into()),
        other => panic!("unsupported generated column type: {other:?}"),
    }
}

pub fn distinct_value_for_type(ty: &AlgebraicType, current: &AlgebraicValue) -> AlgebraicValue {
    let default = default_value_for_type(ty);
    if &default != current {
        return default;
    }

    match ty {
        AlgebraicType::Bool => AlgebraicValue::Bool(true),
        AlgebraicType::I8 => AlgebraicValue::I8(1),
        AlgebraicType::U8 => AlgebraicValue::U8(1),
        AlgebraicType::I16 => AlgebraicValue::I16(1),
        AlgebraicType::U16 => AlgebraicValue::U16(1),
        AlgebraicType::I32 => AlgebraicValue::I32(1),
        AlgebraicType::U32 => AlgebraicValue::U32(1),
        AlgebraicType::I64 => AlgebraicValue::I64(1),
        AlgebraicType::U64 => AlgebraicValue::U64(1),
        AlgebraicType::I128 => AlgebraicValue::I128(1.into()),
        AlgebraicType::U128 => AlgebraicValue::U128(1.into()),
        AlgebraicType::String => AlgebraicValue::String("dst_unique_conflict".into()),
        other => panic!("unsupported generated column type: {other:?}"),
    }
}

fn edge_value_for_type(rng: &mut DstRng, ty: &AlgebraicType, idx: usize) -> AlgebraicValue {
    match ty {
        AlgebraicType::Bool => AlgebraicValue::Bool(rng.index(2) == 0),
        AlgebraicType::I8 => [i8::MIN, -1, 0, 1, i8::MAX][rng.index(5)].into(),
        AlgebraicType::U8 => [0, 1, u8::MAX][rng.index(3)].into(),
        AlgebraicType::I16 => [i16::MIN, -1, 0, 1, i16::MAX][rng.index(5)].into(),
        AlgebraicType::U16 => [0, 1, u16::MAX][rng.index(3)].into(),
        AlgebraicType::I32 => [i32::MIN, -1, 0, 1, i32::MAX][rng.index(5)].into(),
        AlgebraicType::U32 => [0, 1, u32::MAX][rng.index(3)].into(),
        AlgebraicType::I64 => [i64::MIN, -1, 0, 1, i64::MAX][rng.index(5)].into(),
        AlgebraicType::U64 => [0, 1, u64::MAX.saturating_sub(idx as u64)][rng.index(3)].into(),
        AlgebraicType::I128 => {
            let value = [i128::MIN, -1, 0, 1, i128::MAX][rng.index(5)];
            AlgebraicValue::I128(value.into())
        }
        AlgebraicType::U128 => {
            let value = [0, 1, u128::MAX][rng.index(3)];
            AlgebraicValue::U128(value.into())
        }
        AlgebraicType::String => match rng.index(5) {
            0 => AlgebraicValue::String("".into()),
            1 => AlgebraicValue::String("same".into()),
            2 => AlgebraicValue::String("x".repeat(512).into()),
            3 => AlgebraicValue::String(format!("edge_{}", char::from_u32(0x2603).expect("valid char")).into()),
            _ => AlgebraicValue::String(format!("v{idx}_edge").into()),
        },
        other => panic!("unsupported generated column type: {other:?}"),
    }
}

impl SimRow {
    pub fn to_product_value(&self) -> ProductValue {
        ProductValue::from_iter(self.values.iter().cloned())
    }

    pub fn to_bsatn(&self) -> anyhow::Result<Vec<u8>> {
        Ok(spacetimedb_sats::bsatn::to_vec(&self.to_product_value())?)
    }

    pub fn from_product_value(value: ProductValue) -> Self {
        SimRow {
            values: value.elements.to_vec(),
        }
    }

    pub fn project_key(&self, cols: &[u16]) -> Self {
        let values = cols
            .iter()
            .map(|&col| self.values[col as usize].clone())
            .collect::<Vec<_>>();
        SimRow { values }
    }

    pub fn to_algebraic_value(&self) -> AlgebraicValue {
        match self.values.as_slice() {
            [value] => value.clone(),
            _ => ProductValue::from_iter(self.values.iter().cloned()).into(),
        }
    }

    pub fn id(&self) -> Option<u64> {
        match self.values.first() {
            Some(AlgebraicValue::U64(value)) => Some(*value),
            _ => None,
        }
    }
}
