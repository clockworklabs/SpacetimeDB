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
    /// Optional secondary indexed column used to exercise index installation paths.
    pub secondary_index_col: Option<u16>,
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
    U64(u64),
    String(String),
    Bool(bool),
}

pub fn generate_supported_type(rng: &mut DstRng) -> AlgebraicType {
    match rng.index(3) {
        0 => AlgebraicType::U64,
        1 => AlgebraicType::String,
        _ => AlgebraicType::Bool,
    }
}

pub fn generate_value_for_type(rng: &mut DstRng, ty: &AlgebraicType, idx: usize) -> AlgebraicValue {
    match ty {
        AlgebraicType::U64 => AlgebraicValue::U64((rng.next_u64() % 1000) + idx as u64),
        AlgebraicType::String => AlgebraicValue::String(format!("v{}_{}", idx, rng.next_u64() % 10_000).into()),
        AlgebraicType::Bool => AlgebraicValue::Bool(rng.index(2) == 0),
        other => panic!("unsupported generated column type: {other:?}"),
    }
}

impl From<&AlgebraicValue> for SerdeAlgebraicValue {
    fn from(value: &AlgebraicValue) -> Self {
        match value {
            AlgebraicValue::U64(value) => Self::U64(*value),
            AlgebraicValue::String(value) => Self::String(value.to_string()),
            AlgebraicValue::Bool(value) => Self::Bool(*value),
            other => panic!("unsupported value in simulator row serde: {other:?}"),
        }
    }
}

impl From<SerdeAlgebraicValue> for AlgebraicValue {
    fn from(value: SerdeAlgebraicValue) -> Self {
        match value {
            SerdeAlgebraicValue::U64(value) => Self::U64(value),
            SerdeAlgebraicValue::String(value) => Self::String(value.into()),
            SerdeAlgebraicValue::Bool(value) => Self::Bool(value),
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

    pub fn id(&self) -> Option<u64> {
        match self.values.first() {
            Some(AlgebraicValue::U64(value)) => Some(*value),
            _ => None,
        }
    }
}
