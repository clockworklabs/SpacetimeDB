//! Shared schema and row model used by DST targets.

use serde::{de::Deserializer, ser::Serializer, Deserialize, Serialize};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductValue};

use crate::seed::DstRng;

/// Generated schema for one simulator case.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SchemaPlan {
    /// User-visible tables installed before the workload starts.
    pub tables: Vec<TablePlan>,
}

/// Table definition used by simulators.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum SerdeAlgebraicValue {
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
    String(String),
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

impl From<&AlgebraicValue> for SerdeAlgebraicValue {
    fn from(value: &AlgebraicValue) -> Self {
        match value {
            AlgebraicValue::Bool(value) => Self::Bool(*value),
            AlgebraicValue::I8(value) => Self::I8(*value),
            AlgebraicValue::U8(value) => Self::U8(*value),
            AlgebraicValue::I16(value) => Self::I16(*value),
            AlgebraicValue::U16(value) => Self::U16(*value),
            AlgebraicValue::I32(value) => Self::I32(*value),
            AlgebraicValue::U32(value) => Self::U32(*value),
            AlgebraicValue::I64(value) => Self::I64(*value),
            AlgebraicValue::U64(value) => Self::U64(*value),
            AlgebraicValue::I128(value) => Self::I128(value.0),
            AlgebraicValue::U128(value) => Self::U128(value.0),
            AlgebraicValue::String(value) => Self::String(value.to_string()),
            other => panic!("unsupported value in simulator row serde: {other:?}"),
        }
    }
}

impl From<SerdeAlgebraicValue> for AlgebraicValue {
    fn from(value: SerdeAlgebraicValue) -> Self {
        match value {
            SerdeAlgebraicValue::Bool(value) => Self::Bool(value),
            SerdeAlgebraicValue::I8(value) => Self::I8(value),
            SerdeAlgebraicValue::U8(value) => Self::U8(value),
            SerdeAlgebraicValue::I16(value) => Self::I16(value),
            SerdeAlgebraicValue::U16(value) => Self::U16(value),
            SerdeAlgebraicValue::I32(value) => Self::I32(value),
            SerdeAlgebraicValue::U32(value) => Self::U32(value),
            SerdeAlgebraicValue::I64(value) => Self::I64(value),
            SerdeAlgebraicValue::U64(value) => Self::U64(value),
            SerdeAlgebraicValue::I128(value) => Self::I128(value.into()),
            SerdeAlgebraicValue::U128(value) => Self::U128(value.into()),
            SerdeAlgebraicValue::String(value) => Self::String(value.into()),
        }
    }
}

impl Serialize for SimRow {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let values = self.values.iter().map(SerdeAlgebraicValue::from).collect::<Vec<_>>();
        values.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SimRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let values = Vec::<SerdeAlgebraicValue>::deserialize(deserializer)?
            .into_iter()
            .map(AlgebraicValue::from)
            .collect();
        Ok(Self { values })
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
