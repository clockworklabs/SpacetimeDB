use spacetimedb_lib::AlgebraicType;

use super::workload::Row;
use crate::schema::{IndexAlgorithm, SchemaPlan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountState {
    pub row_counts: Vec<TableRowCount>,
    pub table_rows: Vec<TableRows>,
    pub schema: SchemaState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableRowCount {
    pub table: usize,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRows {
    pub table: usize,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaState {
    pub tables: Vec<TableSchemaState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchemaState {
    pub table: usize,
    pub name: String,
    pub is_public: bool,
    pub is_event: bool,
    pub primary_key: Option<usize>,
    pub columns: Vec<ColumnState>,
    pub indexes: Vec<IndexState>,
    pub unique_constraints: Vec<UniqueConstraintState>,
    pub sequences: Vec<SequenceState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnState {
    pub name: String,
    pub ty: AlgebraicType,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexState {
    pub columns: Vec<usize>,
    pub algorithm: IndexAlgorithmState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IndexAlgorithmState {
    BTree,
    Hash,
    Direct,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UniqueConstraintState {
    pub columns: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SequenceState {
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDelta {
    pub tables: Vec<TableDelta>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDelta {
    pub table: usize,
    pub inserts: Vec<Row>,
    pub deletes: Vec<Row>,
    pub truncated: bool,
}

pub fn schema_state_for_plan(schema: &SchemaPlan) -> SchemaState {
    SchemaState {
        tables: schema
            .tables
            .iter()
            .enumerate()
            .map(|(table, table_plan)| {
                let mut indexes = table_plan
                    .indexes
                    .iter()
                    .map(|index| IndexState {
                        columns: index.columns.clone(),
                        algorithm: match index.algorithm {
                            IndexAlgorithm::BTree => IndexAlgorithmState::BTree,
                            IndexAlgorithm::Hash => IndexAlgorithmState::Hash,
                        },
                    })
                    .collect::<Vec<_>>();
                indexes.sort();

                let mut unique_constraints = table_plan
                    .unique_constraints
                    .iter()
                    .map(|constraint| UniqueConstraintState {
                        columns: constraint.columns.clone(),
                    })
                    .collect::<Vec<_>>();
                unique_constraints.sort();

                let mut sequences = table_plan
                    .sequences
                    .iter()
                    .map(|sequence| SequenceState {
                        column: sequence.column,
                    })
                    .collect::<Vec<_>>();
                sequences.sort();

                TableSchemaState {
                    table,
                    name: table_plan.name.clone(),
                    is_public: table_plan.is_public,
                    is_event: table_plan.is_event,
                    primary_key: table_plan.primary_key,
                    columns: table_plan
                        .columns
                        .iter()
                        .map(|column| ColumnState {
                            name: column.name.clone(),
                            ty: column.ty.to_algebraic(),
                        })
                        .collect(),
                    indexes,
                    unique_constraints,
                    sequences,
                }
            })
            .collect(),
    }
}
