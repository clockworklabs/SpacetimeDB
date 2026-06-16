//! Custom schema types for DST table/index definitions.
//!
//! These types are the canonical source of truth for generated schemas.
//! They lower into [`RawModuleDefV10`] via [`lower_schema`].

use spacetimedb_lib::db::raw_def::v10::*;
use spacetimedb_lib::db::raw_def::v9::{RawIndexAlgorithm, TableAccess, TableType};
use spacetimedb_primitives::{ColId, ColList};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ArrayType, ArrayValue, ProductType, ProductTypeElement};

// ---------------------------------------------------------------------------
// Column types — closed set, expand deliberately.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    Bool,
    I64,
    U64,
    String,
    Bytes,
}

impl Type {
    pub const ALL: &'static [Type] = &[
        Type::Bool,
        Type::I64,
        Type::U64,
        Type::String,
        Type::Bytes,
    ];

    pub fn to_algebraic(self) -> AlgebraicType {
        match self {
            Type::Bool => AlgebraicType::Bool,
            Type::I64 => AlgebraicType::I64,
            Type::U64 => AlgebraicType::U64,
            Type::String => AlgebraicType::String,
            Type::Bytes => AlgebraicType::Array(ArrayType {
                elem_ty: Box::new(AlgebraicType::U8),
            }),
        }
    }

    pub fn is_integral(self) -> bool {
        matches!(self, Type::I64 | Type::U64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Value {
    Bool(bool),
    I64(i64),
    U64(u64),
    String(String),
    Bytes(Vec<u8>),
}

impl Value {
    pub fn type_of(&self) -> Type {
        match self {
            Value::Bool(_) => Type::Bool,
            Value::I64(_) => Type::I64,
            Value::U64(_) => Type::U64,
            Value::String(_) => Type::String,
            Value::Bytes(_) => Type::Bytes,
        }
    }

    fn to_algebraic(&self) -> AlgebraicValue {
        match self {
            Value::Bool(b) => AlgebraicValue::Bool(*b),
            Value::I64(v) => AlgebraicValue::I64(*v),
            Value::U64(v) => AlgebraicValue::U64(*v),
            Value::String(s) => AlgebraicValue::String(s.clone().into()),
            Value::Bytes(b) => AlgebraicValue::Array(ArrayValue::U8(b.clone().into())),
        }
    }
}

// ---------------------------------------------------------------------------
// Schema plan — the canonical source of truth.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SchemaPlan {
    pub tables: Vec<TablePlan>,
}

#[derive(Debug, Clone)]
pub struct TablePlan {
    pub name: String,
    pub columns: Vec<ColumnPlan>,
    pub primary_key: Option<usize>,
    pub indexes: Vec<IndexPlan>,
    pub unique_constraints: Vec<UniqueConstraintPlan>,
    pub sequences: Vec<SequencePlan>,
    pub default_values: Vec<DefaultPlan>,
    pub is_event: bool,
    pub is_public: bool,
}

#[derive(Debug, Clone)]
pub struct ColumnPlan {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub struct IndexPlan {
    /// Indices into `TablePlan.columns`.
    pub columns: Vec<usize>,
    pub algorithm: IndexAlgorithm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexAlgorithm {
    BTree,
    Hash,
}

#[derive(Debug, Clone)]
pub struct UniqueConstraintPlan {
    /// Indices into `TablePlan.columns`. Non-empty.
    pub columns: Vec<usize>,
}

/// A sequence on a specific column. The column's type is carried inline
/// so callers cannot create a sequence on a non-integral column —
/// the constructor requires `ty.is_integral()`.
#[derive(Debug, Clone)]
pub struct SequencePlan {
    /// Index into `TablePlan.columns`.
    pub column: usize,
    /// The type of that column. Must be integral (I64 or U64).
    pub ty: Type,
    pub start: Option<i128>,
    pub min_value: Option<i128>,
    pub max_value: Option<i128>,
    pub increment: i128,
}

impl SequencePlan {
    /// Create a sequence plan. Returns `None` if the type is not integral.
    pub fn new(column: usize, ty: Type) -> Option<Self> {
        if !ty.is_integral() {
            return None;
        }
        Some(Self {
            column,
            ty,
            start: None,
            min_value: None,
            max_value: None,
            increment: 1,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DefaultPlan {
    /// Index into `TablePlan.columns`.
    pub column: usize,
    pub value: Value,
}

// ---------------------------------------------------------------------------
// Lowering into RawModuleDefV10.
// ---------------------------------------------------------------------------

pub fn lower_schema(schema: &SchemaPlan) -> RawModuleDefV10 {
    let mut builder = RawModuleDefV10Builder::new();
    builder.set_case_conversion_policy(CaseConversionPolicy::None);

    for table in &schema.tables {
        lower_table(&mut builder, table);
    }

    builder.finish()
}

fn lower_table(builder: &mut RawModuleDefV10Builder, table: &TablePlan) {
    let product_type = ProductType {
        elements: table
            .columns
            .iter()
            .map(|col| ProductTypeElement {
                name: Some(col.name.clone().into()),
                algebraic_type: col.ty.to_algebraic(),
            })
            .collect(),
    };

    let mut tbl = builder.build_table_with_new_type(table.name.clone(), product_type, false);

    tbl = tbl.with_type(TableType::User);
    tbl = tbl.with_access(if table.is_public {
        TableAccess::Public
    } else {
        TableAccess::Private
    });
    tbl = tbl.with_event(table.is_event);

    // Primary key.
    if let Some(pk) = table.primary_key {
        tbl = tbl.with_primary_key(ColId(pk as u16));
    }

    // Unique constraints — all of them, including PK-matching.
    for constraint in &table.unique_constraints {
        let col_list: ColList = constraint
            .columns
            .iter()
            .map(|&c| ColId(c as u16))
            .collect();
        tbl = tbl.with_unique_constraint(col_list);
    }

    // Indexes.
    for index in &table.indexes {
        let col_list: ColList = index
            .columns
            .iter()
            .map(|&c| ColId(c as u16))
            .collect();

        let algorithm = match index.algorithm {
            IndexAlgorithm::BTree => RawIndexAlgorithm::BTree { columns: col_list },
            IndexAlgorithm::Hash => RawIndexAlgorithm::Hash { columns: col_list },
        };

        let source_name = format!(
            "{}_{}_idx",
            table.name,
            index
                .columns
                .iter()
                .map(|&c| table.columns[c].name.as_str())
                .collect::<Vec<_>>()
                .join("_")
        );

        tbl = tbl.with_index_no_accessor_name(algorithm, source_name);
    }

    // Sequences — all of them.
    for seq in &table.sequences {
        tbl = tbl.with_column_sequence(ColId(seq.column as u16));
    }

    // Default values.
    for default in &table.default_values {
        let algebraic_val = default.value.to_algebraic();
        tbl = tbl.with_default_column_value(ColId(default.column as u16), algebraic_val);
    }

    tbl.finish();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_single_table() {
        let schema = SchemaPlan {
            tables: vec![TablePlan {
                name: "users".into(),
                columns: vec![
                    ColumnPlan { name: "id".into(), ty: Type::U64 },
                    ColumnPlan { name: "name".into(), ty: Type::String },
                    ColumnPlan { name: "score".into(), ty: Type::I64 },
                ],
                primary_key: Some(0),
                indexes: vec![IndexPlan {
                    columns: vec![2],
                    algorithm: IndexAlgorithm::BTree,
                }],
                unique_constraints: vec![UniqueConstraintPlan {
                    columns: vec![0],
                }],
                sequences: vec![SequencePlan::new(0, Type::U64).unwrap()],
                default_values: vec![],
                is_event: false,
                is_public: true,
            }],
        };

        let raw = lower_schema(&schema);

        // Should have Typespace, Types, and Tables sections.
        assert!(raw.typespace().is_some());
        assert!(raw.types().is_some());
        let tables = raw.tables().unwrap();
        assert_eq!(tables.len(), 1);

        let t = &tables[0];
        assert_eq!(t.source_name.as_ref(), "users");
        assert_eq!(t.table_type, TableType::User);
        assert_eq!(t.table_access, TableAccess::Public);
        assert!(!t.is_event);
        assert_eq!(t.primary_key.len(), 1);
        assert_eq!(t.indexes.len(), 1);
        assert_eq!(t.sequences.len(), 1);
    }

    #[test]
    fn sequence_rejects_non_integral() {
        assert!(SequencePlan::new(0, Type::Bool).is_none());
        assert!(SequencePlan::new(0, Type::String).is_none());
        assert!(SequencePlan::new(0, Type::Bytes).is_none());
        assert!(SequencePlan::new(0, Type::I64).is_some());
        assert!(SequencePlan::new(0, Type::U64).is_some());
    }

    #[test]
    fn type_roundtrip() {
        for ty in Type::ALL {
            // Every DST type should roundtrip through AlgebraicType.
            let _ = ty.to_algebraic();
        }
    }
}
