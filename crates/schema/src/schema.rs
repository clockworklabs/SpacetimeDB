use crate::def::*;
use spacetimedb_primitives::*;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::column_ordering::is_sorted_by;
use spacetimedb_sats::db::raw_def::IndexType;
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::relation::{Column, DbTable, FieldName, Header};
use spacetimedb_sats::{AlgebraicType, ProductType, ProductTypeElement};
use std::collections::BTreeMap;
use std::sync::Arc;

const PROBABLY_UNALLOCATED: u32 = u32::MAX;

/// Represents a schema definition for a database sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceSchema {
    pub sequence_id: SequenceId,
    pub table_id: TableId,
    /// The position of the column associated with this sequence.
    pub col_pos: ColId,
    pub increment: i128,
    pub start: i128,
    pub min_value: i128,
    pub max_value: i128,
    pub allocated: i128,
}

impl SequenceSchema {
    /// Creates a new [SequenceSchema] instance from a [SequenceDef] and a `table_id`.
    /// The `SchemaId` is set to a dummy value.
    ///
    /// # Arguments
    ///
    /// * `table_id` - The ID of the table associated with the sequence.
    /// * `table_def` - The [TableDef] to derive the schema from.
    /// * `sequence` - The [SequenceDef] to derive the schema from.
    pub fn from_def(table_id: TableId, table_def: &TableDef, sequence: &SequenceDef) -> Self {
        Self {
            sequence_id: SequenceId(PROBABLY_UNALLOCATED), // Will be replaced later when created
            table_id,
            col_pos: table_def
                .get_column_id(&sequence.column_name)
                .expect("malformed validated def?"),
            increment: 1,
            start: sequence.start.unwrap_or(1),
            min_value: sequence.min_value.unwrap_or(1),
            max_value: sequence.max_value.unwrap_or(i128::MAX),
            allocated: SEQUENCE_PREALLOCATION_AMOUNT,
        }
    }
}

/// A struct representing the schema of a database index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSchema {
    pub index_id: IndexId,
    pub table_id: TableId,
    pub index_type: IndexType,
    pub is_unique: bool,
    pub columns: ColList,
}

impl IndexSchema {
    /// Constructs an [IndexSchema] from a given [IndexDef] and `table_id`.
    ///
    /// Tagged with "unique" if there is a unique constraint on these columns on the `TableDef`.
    pub fn from_def(table_id: TableId, table_def: &TableDef, index: &IndexDef) -> Self {
        let sorted_column_names = {
            let mut sorted_column_names = index.column_names.clone();
            sorted_column_names.sort();
            sorted_column_names
        };
        let is_unique = table_def
            .unique_constraints
            .iter()
            .any(|unique_constraint| unique_constraint.column_names == sorted_column_names);

        IndexSchema {
            index_id: IndexId(PROBABLY_UNALLOCATED), // Set to 0 as it may be assigned later.
            table_id,
            index_type: index.index_type,
            is_unique,
            columns: table_def
                .get_column_list(&index.column_names)
                .expect("malformed validated def?"),
        }
    }
}

/// A struct representing the schema of a database column.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct ColumnSchema {
    pub table_id: TableId,
    /// Position of the column within the table.
    pub col_pos: ColId,
    pub col_name: Box<str>,
    pub col_type: AlgebraicType,
}

impl ColumnSchema {
    /// Constructs a [ColumnSchema] from a given [ColumnDef] and `table_id`.
    ///
    /// # Arguments
    ///
    /// * `table_id`: Identifier of the table to which the column belongs.
    /// * `col_pos`: Position of the column within the table.
    /// * `column`: The `ColumnDef` containing column information.
    pub fn from_def(table_id: TableId, table_def: &TableDef, column_def: &ColumnDef) -> Self {
        ColumnSchema {
            table_id,
            col_pos: table_def
                .get_column_id(&column_def.col_name)
                .expect("malformed validated def?"),
            col_name: (&*column_def.col_name).into(),
            col_type: column_def.col_type.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniqueConstraintSchema {
    pub constraint_id: ConstraintId,
    pub table_id: TableId,
    pub columns: ColList,
}

impl UniqueConstraintSchema {
    pub fn from_def(table_id: TableId, table_def: &TableDef, constraint: &UniqueConstraintDef) -> Self {
        UniqueConstraintSchema {
            constraint_id: ConstraintId(PROBABLY_UNALLOCATED), // Set to 0 as it may be assigned later.
            table_id,
            columns: table_def
                .get_column_list(&constraint.column_names)
                .expect("malformed validated def?"),
        }
    }
}

/// Some kind of constraint on the table.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TableConstraintSchema {
    Unique(UniqueConstraintSchema),
}

impl TableConstraintSchema {
    pub fn constraint_id(&self) -> ConstraintId {
        match self {
            TableConstraintSchema::Unique(x) => x.constraint_id,
        }
    }

    pub fn table_id(&self) -> TableId {
        match self {
            TableConstraintSchema::Unique(x) => x.table_id,
        }
    }
}

/// A data structure representing the schema of a database table.
///
/// This struct holds information about the table, including its identifier,
/// name, columns, indexes, constraints, sequences, type, and access rights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema {
    pub table_id: TableId,
    pub table_name: Box<str>,
    columns: Vec<ColumnSchema>,
    pub indexes: Vec<IndexSchema>,
    pub constraints: Vec<TableConstraintSchema>,
    pub sequences: Vec<SequenceSchema>,
    pub table_type: StTableType,
    pub table_access: StAccess,
    /// Cache for `row_type_for_table` in the data store.
    row_type: ProductType,
}

impl TableSchema {
    /// Construct a new TableSchema.
    ///
    /// Does NOT generate indexes for constraints! You will need to do this manually.
    #[allow(clippy::too_many_arguments, clippy::boxed_local)]
    pub fn new(
        table_id: TableId,
        table_name: Box<str>,
        columns: Vec<ColumnSchema>,
        indexes: Vec<IndexSchema>,
        constraints: Vec<TableConstraintSchema>,
        sequences: Vec<SequenceSchema>,
        table_type: StTableType,
        table_access: StAccess,
    ) -> Self {
        let row_type = ProductType::new(
            columns
                .iter()
                .map(|c| ProductTypeElement {
                    name: Some(c.col_name.clone()),
                    algebraic_type: c.col_type.clone(),
                })
                .collect(),
        );

        Self {
            table_id,
            table_name,
            columns,
            indexes,
            constraints,
            sequences,
            table_type,
            table_access,
            row_type,
        }
    }

    pub fn into_columns(self) -> Vec<ColumnSchema> {
        self.columns
    }

    /// IMPORTANT: Ban changes from outside so [Self::row_type] won't get invalidated.
    pub fn columns(&self) -> &[ColumnSchema] {
        &self.columns
    }

    /// Clear all the [Self::indexes], [Self::sequences] & [Self::constraints]
    pub fn clear_adjacent_schemas(&mut self) {
        self.indexes.clear();
        self.sequences.clear();
        self.constraints.clear();
    }

    // Crud operation on adjacent schemas

    /// Add OR replace the [SequenceSchema]
    pub fn update_sequence(&mut self, of: SequenceSchema) {
        if let Some(x) = self.sequences.iter_mut().find(|x| x.sequence_id == of.sequence_id) {
            *x = of;
        } else {
            self.sequences.push(of);
        }
    }

    /// Removes the given `sequence_id`
    pub fn remove_sequence(&mut self, sequence_id: SequenceId) {
        self.sequences.retain(|x| x.sequence_id != sequence_id)
    }

    /// Add OR replace the [IndexSchema]
    pub fn update_index(&mut self, of: IndexSchema) {
        if let Some(x) = self.indexes.iter_mut().find(|x| x.index_id == of.index_id) {
            *x = of;
        } else {
            self.indexes.push(of);
        }
    }

    /// Removes the given `index_id`
    pub fn remove_index(&mut self, index_id: IndexId) {
        self.indexes.retain(|x| x.index_id != index_id)
    }

    /// Add OR replace the [ConstraintSchema]
    pub fn update_constraint(&mut self, of: TableConstraintSchema) {
        if let Some(x) = self
            .constraints
            .iter_mut()
            .find(|x| x.constraint_id() == of.constraint_id())
        {
            *x = of;
        } else {
            self.constraints.push(of);
        }
    }

    /// Removes the given `index_id`
    pub fn remove_constraint(&mut self, constraint_id: ConstraintId) {
        self.constraints.retain(|x| x.constraint_id() != constraint_id)
    }

    /// Check if the specified `field` exists in this [TableSchema].
    ///
    /// # Warning
    ///
    /// This function ignores the `table_id` when searching for a column.
    pub fn get_column_by_field(&self, field: FieldName) -> Option<&ColumnSchema> {
        self.get_column(field.col.idx())
    }

    pub fn get_columns(&self, columns: &ColList) -> Vec<(ColId, Option<&ColumnSchema>)> {
        columns.iter().map(|col| (col, self.columns.get(col.idx()))).collect()
    }

    /// Get a reference to a column by its position (`pos`) in the table.
    pub fn get_column(&self, pos: usize) -> Option<&ColumnSchema> {
        self.columns.get(pos)
    }

    /// Check if the `col_name` exist on this [TableSchema]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_name(&self, col_name: &str) -> Option<&ColumnSchema> {
        self.columns.iter().find(|x| &*x.col_name == col_name)
    }

    /// Project the fields from the supplied `indexes`.
    pub fn project(&self, indexes: impl Iterator<Item = ColId>) -> Result<Vec<&ColumnSchema>, InvalidFieldError> {
        indexes
            .map(|index| self.get_column(index.0 as usize).ok_or_else(|| index.into()))
            .collect()
    }

    /// Utility for project the fields from the supplied `indexes` that is a [ColList],
    /// used for when the list of field indexes have at least one value.
    pub fn project_not_empty(&self, indexes: ColList) -> Result<Vec<&ColumnSchema>, InvalidFieldError> {
        self.project(indexes.iter())
    }

    /// IMPORTANT: Is required to have this cached to avoid a perf drop on datastore operations
    pub fn get_row_type(&self) -> &ProductType {
        &self.row_type
    }

    /// Utility to avoid cloning in `row_type_for_table`
    pub fn into_row_type(self) -> ProductType {
        self.row_type
    }

    /// Create a new [TableSchema] from a [TableDef] and a `table_id`.
    ///
    /// This generates an index for each unique constraint on the table (if there is not already
    /// an index on the same columns.)
    ///
    /// # Parameters
    ///
    /// - `table_id`: The unique identifier for the table.
    /// - `database_def`: The `DatabaseDef` containing the schema information.
    /// - `table_def`: The specific `TableDef` within the `DatabaseDef` we are interested in.
    pub fn from_def(table_id: TableId, _database_def: &DatabaseDef, table_def: &TableDef) -> Self {
        // this is somewhat redundant as the DatabaseDef already has a row type for the table.
        // it might have references though, so just recreate it for now.
        let row_type = ProductType::new(
            table_def
                .columns
                .iter()
                .map(|c| ProductTypeElement {
                    name: Some((&*c.col_name).into()),
                    algebraic_type: c.col_type.clone(),
                })
                .collect(),
        );

        let table_name = (&*table_def.table_name).into();
        let columns = table_def
            .columns
            .iter()
            .map(|col_def| ColumnSchema::from_def(table_id, table_def, col_def))
            .collect();

        let constraints = table_def
            .unique_constraints
            .iter()
            .map(|unique_constraint_def| {
                TableConstraintSchema::Unique(UniqueConstraintSchema::from_def(
                    table_id,
                    table_def,
                    unique_constraint_def,
                ))
            })
            .collect();

        let sequences = table_def
            .sequences
            .iter()
            .map(|sequence_def| SequenceSchema::from_def(table_id, table_def, sequence_def))
            .collect();

        let indexes: Vec<_> = table_def
            .indexes
            .iter()
            .map(|index_def| IndexSchema::from_def(table_id, table_def, index_def))
            .collect();

        let generated_indexes = {
            let mut generated_indexes = vec![];

            let indexes_by_sorted_columns = table_def
                .indexes
                .iter()
                .enumerate()
                .map(|(i, index_def)| {
                    let mut sorted_column_names = index_def.column_names.clone();
                    sorted_column_names.sort();
                    (sorted_column_names, &indexes[i])
                })
                .collect::<BTreeMap<_, _>>();

            for unique_constraint in &table_def.unique_constraints {
                assert!(
                    is_sorted_by(&unique_constraint.column_names, |a, b| a.cmp(b)),
                    "malformed validated def"
                );

                if let Some(index) = indexes_by_sorted_columns.get(&unique_constraint.column_names) {
                    // IndexSchema::from_def should have marked the relevant index as unique.
                    assert!(index.is_unique, "unique constraint corresponds to existing index, created IndexSchema should have been tagged as unique");
                    continue;
                }

                generated_indexes.push(IndexSchema {
                    index_id: IndexId(PROBABLY_UNALLOCATED), // Set to 0 as it may be assigned later.
                    table_id,
                    index_type: IndexType::BTree,
                    is_unique: true,
                    columns: table_def
                        .get_column_list(&unique_constraint.column_names)
                        .expect("malformed validated def?"),
                })
            }

            generated_indexes
        };

        let indexes = indexes.into_iter().chain(generated_indexes).collect();

        Self {
            table_id,
            table_name,
            columns,
            indexes,
            constraints,
            sequences,
            table_type: table_def.table_type,
            table_access: table_def.table_access,
            row_type,
        }
    }
}

impl From<&TableSchema> for ProductType {
    fn from(value: &TableSchema) -> Self {
        ProductType::new(
            value
                .columns
                .iter()
                .map(|c| ProductTypeElement {
                    name: Some(c.col_name.clone()),
                    algebraic_type: c.col_type.clone(),
                })
                .collect(),
        )
    }
}

impl From<&TableSchema> for DbTable {
    fn from(value: &TableSchema) -> Self {
        DbTable::new(
            Arc::new(value.into()),
            value.table_id,
            value.table_type,
            value.table_access,
        )
    }
}

impl From<&TableSchema> for Header {
    fn from(value: &TableSchema) -> Self {
        let fields = value
            .columns
            .iter()
            .map(|x| Column::new(FieldName::new(value.table_id, x.col_pos), x.col_type.clone()))
            .collect();

        let unique_constraints = value
            .constraints
            .iter()
            .map(|x| match x {
                TableConstraintSchema::Unique(x) => x.columns.clone(),
            })
            .collect();

        Header::new(value.table_id, value.table_name.clone(), fields, unique_constraints)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::DatabaseDef;
    use spacetimedb_sats::db::raw_def::*;

    #[test]
    fn generated_indexes() {
        let def: DatabaseDef = RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![
                        RawColumnDef::new("a", AlgebraicType::U64),
                        RawColumnDef::new("b", AlgebraicType::U32),
                    ],
                )
                // this constraint has a matching index, which should be used
                .with_unique_constraint(&["a"])
                .with_index(&["a"], IndexType::BTree)
                // this constraint has no matching index, so one should be generated
                .with_unique_constraint(&["b"]),
            )
            .try_into()
            .unwrap();

        let table_schema = TableSchema::from_def(TableId(0), &def, def.tables.values().next().unwrap());

        assert_eq!(table_schema.indexes.len(), 2);

        assert_eq!(table_schema.indexes[0].columns, ColList::new(ColId(0)));
        assert!(table_schema.indexes[0].is_unique);

        assert_eq!(table_schema.indexes[1].columns, ColList::new(ColId(1)));
        assert!(table_schema.indexes[1].is_unique);
    }
}
