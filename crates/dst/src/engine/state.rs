use spacetimedb_lib::db::auth::StAccess;
use spacetimedb_lib::AlgebraicType;
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_schema::def::IndexAlgorithm as SchemaIndexAlgorithm;
use spacetimedb_schema::schema::{Schema, TableSchema};

use super::row::Row;
use crate::schema::{to_module_def, SchemaPlan};

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

impl From<&SchemaIndexAlgorithm> for IndexAlgorithmState {
    fn from(algorithm: &SchemaIndexAlgorithm) -> Self {
        match algorithm {
            SchemaIndexAlgorithm::BTree(_) => Self::BTree,
            SchemaIndexAlgorithm::Hash(_) => Self::Hash,
            SchemaIndexAlgorithm::Direct(_) => Self::Direct,
            _ => Self::Unknown,
        }
    }
}

impl IndexState {
    fn from_schema(algorithm: &SchemaIndexAlgorithm) -> Self {
        Self {
            columns: schema_index_columns(algorithm),
            algorithm: algorithm.into(),
        }
    }
}

impl UniqueConstraintState {
    fn from_schema_columns(columns: impl IntoIterator<Item = ColId>) -> Self {
        Self {
            columns: columns.into_iter().map(|col| col.0 as usize).collect(),
        }
    }
}

impl SequenceState {
    fn from_schema_column(column: ColId) -> Self {
        Self {
            column: column.0 as usize,
        }
    }
}

pub fn schema_state_for_plan(schema: &SchemaPlan) -> SchemaState {
    schema_state_for_valid_plan(schema).expect("model schema must lower to a valid module definition")
}

fn schema_state_for_valid_plan(schema: &SchemaPlan) -> anyhow::Result<SchemaState> {
    let module_def = to_module_def(schema)?;

    let tables = schema
        .tables
        .iter()
        .enumerate()
        .map(|(table, table_plan)| {
            let table_def = module_def
                .tables()
                .find(|table_def| &*table_def.accessor_name == table_plan.name)
                .ok_or_else(|| anyhow::anyhow!("validated schema is missing table accessor {:?}", table_plan.name))?;
            let table_schema = TableSchema::from_module_def(&module_def, table_def, (), TableId::SENTINEL);
            Ok(table_schema_state_for_schema(table, &table_schema))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(SchemaState { tables })
}

pub fn table_schema_state_for_schema(table: usize, schema: &TableSchema) -> TableSchemaState {
    TableSchemaState {
        table,
        name: schema.table_name.to_string(),
        is_public: schema.table_access == StAccess::Public,
        is_event: schema.is_event,
        primary_key: schema.primary_key.map(|col| col.0 as usize),
        columns: schema
            .columns
            .iter()
            .map(|column| ColumnState {
                name: column.col_name.to_string(),
                ty: column.col_type.clone(),
            })
            .collect(),
        indexes: sorted(
            schema
                .indexes
                .iter()
                .map(|index| IndexState::from_schema(&index.index_algorithm)),
        ),
        unique_constraints: sorted(schema.constraints.iter().filter_map(|constraint| {
            constraint
                .data
                .unique_columns()
                .map(|columns| UniqueConstraintState::from_schema_columns(columns.iter()))
        })),
        sequences: sorted(
            schema
                .sequences
                .iter()
                .map(|sequence| SequenceState::from_schema_column(sequence.col_pos)),
        ),
    }
}

fn schema_index_columns(algorithm: &SchemaIndexAlgorithm) -> Vec<usize> {
    algorithm.columns().iter().map(|col| col.0 as usize).collect()
}

fn sorted<T: Ord>(values: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort();
    values
}
