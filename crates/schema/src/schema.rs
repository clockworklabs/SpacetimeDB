//! Schema data structures.
//! These are used at runtime by the vm to store the schema of the database.

use itertools::Itertools;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::db::auth::{StAccess, StTableType};
use spacetimedb_lib::db::error::{DefType, SchemaError};
use spacetimedb_lib::db::raw_def::*;
use spacetimedb_lib::relation::{Column, DbTable, FieldName, Header};
use spacetimedb_lib::{AlgebraicType, ProductType, ProductTypeElement};
use spacetimedb_primitives::*;
use spacetimedb_sats::product_value::InvalidFieldError;
use std::sync::Arc;

/// A data structure representing the schema of a database table.
///
/// This struct holds information about the table, including its identifier,
/// name, columns, indexes, constraints, sequences, type, and access rights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema {
    /// The unique identifier of the table within the database.
    pub table_id: TableId,
    /// The name of the table.
    pub table_name: Box<str>,
    /// The columns of the table.
    /// Inaccessible to prevent mutation.
    /// The ordering of the columns is significant. Columns are frequently identified by `ColId`, that is, position in this list.
    columns: Vec<ColumnSchema>,
    /// The indexes on the table.
    pub indexes: Vec<IndexSchema>,
    /// The constraints on the table.
    pub constraints: Vec<ConstraintSchema>,
    /// The sequences on the table.
    pub sequences: Vec<SequenceSchema>,
    /// Whether the table was created by a user or by the system.
    pub table_type: StTableType,
    /// The visibility of the table.
    pub table_access: StAccess,
    pub scheduled: Option<Box<str>>,
    /// Cache for `row_type_for_table` in the data store.
    row_type: ProductType,
}

impl TableSchema {
    /// Create a table schema.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        table_id: TableId,
        table_name: Box<str>,
        columns: Vec<ColumnSchema>,
        indexes: Vec<IndexSchema>,
        constraints: Vec<ConstraintSchema>,
        sequences: Vec<SequenceSchema>,
        table_type: StTableType,
        table_access: StAccess,
        scheduled: Option<Box<str>>,
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
            scheduled,
        }
    }

    /// Convert a table schema into a list of columns.
    pub fn into_columns(self) -> Vec<ColumnSchema> {
        self.columns
    }

    /// Get the columns of the table. Only immutable access to the columns is provided.
    /// The ordering of the columns is significant. Columns are frequently identified by `ColId`, that is, position in this list.
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
    pub fn update_constraint(&mut self, of: ConstraintSchema) {
        if let Some(x) = self
            .constraints
            .iter_mut()
            .find(|x| x.constraint_id == of.constraint_id)
        {
            *x = of;
        } else {
            self.constraints.push(of);
        }
    }

    /// Removes the given `index_id`
    pub fn remove_constraint(&mut self, constraint_id: ConstraintId) {
        self.constraints.retain(|x| x.constraint_id != constraint_id)
    }

    /// Concatenate the column names from the `columns`
    ///
    /// WARNING: If the `ColId` not exist, is skipped.
    /// TODO(Tyler): This should return an error and not allow this to be constructed
    /// if there is an invalid `ColId`
    pub fn generate_cols_name(&self, columns: &ColList) -> String {
        generate_cols_name(columns, |p| self.get_column(p.idx()).map(|c| &*c.col_name))
    }

    /// Check if the specified `field` exists in this [TableSchema].
    ///
    /// # Warning
    ///
    /// This function ignores the `table_id` when searching for a column.
    pub fn get_column_by_field(&self, field: FieldName) -> Option<&ColumnSchema> {
        self.get_column(field.col.idx())
    }

    /// Look up a list of columns by their positions in the table.
    /// Invalid column positions are permitted.
    pub fn get_columns(&self, columns: &ColList) -> Vec<(ColId, Option<&ColumnSchema>)> {
        columns.iter().map(|col| (col, self.columns.get(col.idx()))).collect()
    }

    /// Get a reference to a column by its position (`pos`) in the table.
    pub fn get_column(&self, pos: usize) -> Option<&ColumnSchema> {
        self.columns.get(pos)
    }

    /// Check if the `col_name` exist on this [TableSchema]
    pub fn get_column_by_name(&self, col_name: &str) -> Option<&ColumnSchema> {
        self.columns.iter().find(|x| &*x.col_name == col_name)
    }

    /// Check if the `col_name` exist on this [TableSchema]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_id_by_name(&self, col_name: &str) -> Option<ColId> {
        self.columns
            .iter()
            .position(|x| &*x.col_name == col_name)
            .map(|x| x.into())
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

    /// Get the constraints on this table.
    pub fn get_constraints(&self) -> Vec<(ColList, Constraints)> {
        self.constraints
            .iter()
            .map(|x| (x.columns.clone(), x.constraints))
            .collect()
    }

    /// Create a new [TableSchema] from a [RawTableDefV8] and a `table_id`.
    ///
    /// # Parameters
    ///
    /// - `table_id`: The unique identifier for the table.
    /// - `schema`: The `TableDef` containing the schema information.
    pub fn from_def(table_id: TableId, schema: RawTableDefV8) -> Self {
        let indexes = schema.generated_indexes().collect::<Vec<_>>();
        let sequences = schema.generated_sequences().collect::<Vec<_>>();
        let constraints = schema.generated_constraints().collect::<Vec<_>>();
        //Sort by columns so is likely to get PK first then the rest and maintain the order for
        //testing.
        TableSchema::new(
            table_id,
            schema.table_name.trim().into(),
            schema
                .columns
                .into_iter()
                .enumerate()
                .map(|(col_pos, x)| ColumnSchema::from_def(table_id, col_pos.into(), x))
                .collect(),
            schema
                .indexes
                .into_iter()
                .chain(indexes)
                .sorted_by_key(|x| x.columns.clone())
                .map(|x| IndexSchema::from_def(table_id, x))
                .collect(),
            schema
                .constraints
                .into_iter()
                .chain(constraints)
                .filter(|x| x.constraints.kind() != ConstraintKind::UNSET)
                .sorted_by_key(|x| x.columns.clone())
                .map(|x| ConstraintSchema::from_def(table_id, x))
                .collect(),
            schema
                .sequences
                .into_iter()
                .chain(sequences)
                .sorted_by_key(|x| x.col_pos)
                .map(|x| SequenceSchema::from_def(table_id, x))
                .collect(),
            schema.table_type,
            schema.table_access,
            schema.scheduled,
        )
    }

    /// Iterate all constraints on the table.
    pub fn column_constraints_iter(&self) -> impl Iterator<Item = (ColList, &Constraints)> {
        self.constraints.iter().map(|x| (x.columns.clone(), &x.constraints))
    }

    /// Resolves the constraints per each column. If the column don't have one, auto-generate [Constraints::unset()].
    ///
    /// This guarantee all columns can be queried for it constraints.
    pub fn column_constraints(&self) -> HashMap<ColList, Constraints> {
        let mut constraints: HashMap<ColList, Constraints> =
            self.column_constraints_iter().map(|(col, ct)| (col, *ct)).collect();

        for col in &self.columns {
            constraints
                .entry(ColList::new(col.col_pos))
                .or_insert(Constraints::unset());
        }

        constraints
    }

    /// Find the `pk` column. Because we run [Self::validated], only exist one `pk`.
    pub fn pk(&self) -> Option<&ColumnSchema> {
        self.column_constraints_iter()
            .find_map(|(col, x)| {
                if x.has_primary_key() {
                    Some(self.columns.iter().find(|x| ColList::new(x.col_pos) == col))
                } else {
                    None
                }
            })
            .flatten()
    }

    /// Verify the definitions of this schema are valid:
    /// - Check all names are not empty
    /// - All columns exists
    /// - Only 1 PK
    /// - Only 1 sequence per column
    /// - Only Btree Indexes
    ///
    /// Deprecated. This will eventually be replaced by the `schema` crate.
    pub fn validated(self) -> Result<Self, Vec<SchemaError>> {
        let mut errors = Vec::new();

        let pks: Vec<_> = self
            .column_constraints_iter()
            .filter_map(|(cols, ct)| {
                if ct.has_primary_key() {
                    Some(
                        self.get_columns(&cols)
                            .iter()
                            .map(|(col, schema)| {
                                if let Some(col) = schema {
                                    col.col_name.clone()
                                } else {
                                    format!("col_{col}").into()
                                }
                            })
                            .collect(),
                    )
                } else {
                    None
                }
            })
            .collect();
        if pks.len() > 1 {
            errors.push(SchemaError::MultiplePrimaryKeys {
                table: self.table_name.clone(),
                pks,
            });
        }

        if self.table_name.is_empty() {
            errors.push(SchemaError::EmptyTableName {
                table_id: self.table_id,
            });
        }

        let columns_not_found = self
            .sequences
            .iter()
            .map(|x| (DefType::Sequence, x.sequence_name.clone(), ColList::new(x.col_pos)))
            .chain(
                self.indexes
                    .iter()
                    .map(|x| (DefType::Index, x.index_name.clone(), x.columns.clone())),
            )
            .chain(
                self.constraints
                    .iter()
                    .map(|x| (DefType::Constraint, x.constraint_name.clone(), x.columns.clone())),
            )
            .filter_map(|(ty, name, cols)| {
                let empty: Vec<_> = self
                    .get_columns(&cols)
                    .iter()
                    .filter_map(|(col, x)| if x.is_none() { Some(*col) } else { None })
                    .collect();

                if empty.is_empty() {
                    None
                } else {
                    Some(SchemaError::ColumnsNotFound {
                        name,
                        table: self.table_name.clone(),
                        columns: empty,
                        ty,
                    })
                }
            });

        errors.extend(columns_not_found);

        errors.extend(self.columns.iter().filter_map(|x| {
            if x.col_name.is_empty() {
                Some(SchemaError::EmptyName {
                    table: self.table_name.clone(),
                    ty: DefType::Column,
                    id: x.col_pos.0,
                })
            } else {
                None
            }
        }));

        errors.extend(self.indexes.iter().filter_map(|x| {
            if x.index_name.is_empty() {
                Some(SchemaError::EmptyName {
                    table: self.table_name.clone(),
                    ty: DefType::Index,
                    id: x.index_id.0,
                })
            } else {
                None
            }
        }));

        //Verify not exist  'Constraints::unset()` they are equivalent to 'None'
        errors.extend(self.constraints.iter().filter_map(|x| {
            if x.constraints.kind() == ConstraintKind::UNSET {
                Some(SchemaError::ConstraintUnset {
                    table: self.table_name.clone(),
                    name: x.constraint_name.clone(),
                    columns: x.columns.clone(),
                })
            } else {
                None
            }
        }));

        errors.extend(self.constraints.iter().filter_map(|x| {
            if x.constraint_name.is_empty() {
                Some(SchemaError::EmptyName {
                    table: self.table_name.clone(),
                    ty: DefType::Constraint,
                    id: x.constraint_id.0,
                })
            } else {
                None
            }
        }));

        errors.extend(self.sequences.iter().filter_map(|x| {
            if x.sequence_name.is_empty() {
                Some(SchemaError::EmptyName {
                    table: self.table_name.clone(),
                    ty: DefType::Sequence,
                    id: x.sequence_id.0,
                })
            } else {
                None
            }
        }));

        // We only support BTree indexes
        errors.extend(self.indexes.iter().filter_map(|x| {
            if x.index_type != IndexType::BTree {
                Some(SchemaError::OnlyBtree {
                    table: self.table_name.clone(),
                    index: x.index_name.clone(),
                    index_type: x.index_type,
                })
            } else {
                None
            }
        }));

        // Verify we don't have more than 1 auto_inc for the same column
        if let Some(err) = self
            .sequences
            .iter()
            .group_by(|&seq| seq.col_pos)
            .into_iter()
            .find_map(|(key, group)| {
                let count = group.count();
                if count > 1 {
                    Some(SchemaError::OneAutoInc {
                        table: self.table_name.clone(),
                        field: self.columns[key.idx()].col_name.clone(),
                    })
                } else {
                    None
                }
            })
        {
            errors.push(err);
        }

        if errors.is_empty() {
            Ok(self)
        } else {
            Err(errors)
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
        let constraints = value.get_constraints();
        let fields = value
            .columns
            .iter()
            .map(|x| Column::new(FieldName::new(value.table_id, x.col_pos), x.col_type.clone()))
            .collect();

        Header::new(value.table_id, value.table_name.clone(), fields, constraints)
    }
}

impl From<TableSchema> for Header {
    fn from(schema: TableSchema) -> Self {
        Header {
            table_id: schema.table_id,
            table_name: schema.table_name.clone(),
            fields: schema
                .columns()
                .iter()
                .cloned()
                .map(|schema| schema.into())
                .collect_vec(),
            constraints: schema
                .constraints
                .into_iter()
                .map(|schema| (schema.columns, schema.constraints))
                .collect_vec(),
        }
    }
}

impl From<TableSchema> for RawTableDefV8 {
    fn from(value: TableSchema) -> Self {
        Self {
            table_name: value.table_name,
            columns: value.columns.into_iter().map(Into::into).collect(),
            indexes: value.indexes.into_iter().map(Into::into).collect(),
            constraints: value.constraints.into_iter().map(Into::into).collect(),
            sequences: value.sequences.into_iter().map(Into::into).collect(),
            table_type: value.table_type,
            table_access: value.table_access,
            scheduled: value.scheduled,
        }
    }
}

/// A struct representing the schema of a database column.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct ColumnSchema {
    /// The ID of the table this column is attached to.
    pub table_id: TableId,
    /// The position of the column within the table.
    pub col_pos: ColId,
    /// The name of the column. Unique within the table.
    pub col_name: Box<str>,
    /// The type of the column.
    pub col_type: AlgebraicType,
}

impl ColumnSchema {
    /// Constructs a [ColumnSchema] from a given [ColumnDef] and `table_id`.
    ///
    /// # Parameters
    ///
    /// * `table_id`: Identifier of the table to which the column belongs.
    /// * `col_pos`: Position of the column within the table.
    /// * `column`: The `ColumnDef` containing column information.
    pub fn from_def(table_id: TableId, col_pos: ColId, column: RawColumnDefV8) -> Self {
        ColumnSchema {
            table_id,
            col_pos,
            col_name: column.col_name.trim().into(),
            col_type: column.col_type,
        }
    }
}

impl From<ColumnSchema> for RawColumnDefV8 {
    fn from(value: ColumnSchema) -> Self {
        Self {
            col_name: value.col_name,
            col_type: value.col_type,
        }
    }
}

impl From<&ColumnSchema> for ProductTypeElement {
    fn from(value: &ColumnSchema) -> Self {
        Self {
            name: Some(value.col_name.clone()),
            algebraic_type: value.col_type.clone(),
        }
    }
}

impl From<ColumnSchema> for Column {
    fn from(schema: ColumnSchema) -> Self {
        Column {
            field: FieldName {
                table: schema.table_id,
                col: schema.col_pos,
            },
            algebraic_type: schema.col_type,
        }
    }
}

/// Contextualizes a reference to a [ColumnSchema] with the name of the table the column is attached to.
#[derive(Debug, Clone)]
pub struct ColumnSchemaRef<'a> {
    /// The column we are referring to.
    pub column: &'a ColumnSchema,
    /// The name of the table the column is attached to.
    pub table_name: &'a str,
}

impl From<ColumnSchemaRef<'_>> for ProductTypeElement {
    fn from(value: ColumnSchemaRef) -> Self {
        ProductTypeElement::new(value.column.col_type.clone(), Some(value.column.col_name.clone()))
    }
}

/// Represents a schema definition for a database sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceSchema {
    /// The unique identifier for the sequence within a database.
    pub sequence_id: SequenceId,
    /// The name of the sequence.
    /// Deprecated. In the future, sequences will be identified by col_pos.
    pub sequence_name: Box<str>,
    /// The ID of the table associated with the sequence.
    pub table_id: TableId,
    /// The position of the column associated with this sequence.
    pub col_pos: ColId,
    /// The increment value for the sequence.
    pub increment: i128,
    /// The starting value for the sequence.
    pub start: i128,
    /// The minimum value for the sequence.
    pub min_value: i128,
    /// The maximum value for the sequence.
    pub max_value: i128,
    /// How many values have already been allocated for the sequence.
    pub allocated: i128,
}

impl SequenceSchema {
    /// Creates a new [SequenceSchema] instance from a [RawSequenceDefV8] and a `table_id`.
    ///
    /// # Parameters
    ///
    /// * `table_id` - The ID of the table associated with the sequence.
    /// * `sequence` - The [RawSequenceDefV8] to derive the schema from.
    ///
    /// # Example
    ///
    /// ```
    /// use spacetimedb_schema::schema::*;
    /// use spacetimedb_lib::db::raw_def::*;
    ///
    /// let sequence_def = RawSequenceDefV8::for_column("my_table".into(), "my_sequence".into(), 1.into());
    /// let schema = SequenceSchema::from_def(42.into(), sequence_def);
    ///
    /// assert_eq!(&*schema.sequence_name, "seq_my_table_my_sequence");
    /// assert_eq!(schema.table_id, 42.into());
    /// ```
    pub fn from_def(table_id: TableId, sequence: RawSequenceDefV8) -> Self {
        Self {
            sequence_id: SequenceId(0), // Will be replaced later when created
            sequence_name: sequence.sequence_name.trim().into(),
            table_id,
            col_pos: sequence.col_pos,
            increment: sequence.increment,
            start: sequence.start.unwrap_or(1),
            min_value: sequence.min_value.unwrap_or(1),
            max_value: sequence.max_value.unwrap_or(i128::MAX),
            allocated: sequence.allocated,
        }
    }
}

impl From<SequenceSchema> for RawSequenceDefV8 {
    fn from(value: SequenceSchema) -> Self {
        RawSequenceDefV8 {
            sequence_name: value.sequence_name,
            col_pos: value.col_pos,
            increment: value.increment,
            start: Some(value.start),
            min_value: Some(value.min_value),
            max_value: Some(value.max_value),
            allocated: value.allocated,
        }
    }
}

/// A struct representing the schema of a database index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSchema {
    /// The unique ID of the index within the schema.
    pub index_id: IndexId,
    /// The ID of the table associated with the index.
    pub table_id: TableId,
    /// The type of the index.
    pub index_type: IndexType,
    /// The name of the index. This should not be assumed to follow any particular format.
    /// Unique within the table.
    // TODO(jgilles): check and/or specify if this is currently unique within the database.
    pub index_name: Box<str>,
    /// If the index is unique.
    pub is_unique: bool,
    /// The list of columns associated with the index.
    /// This is truly a list: the order of the columns is significant.
    /// The columns are projected and serialized to bitstrings in this order,
    /// which affects the order of elements within a BTreeIndex.
    pub columns: ColList,
}

impl IndexSchema {
    /// Constructs an [IndexSchema] from a given [IndexDef] and `table_id`.
    pub fn from_def(table_id: TableId, index: RawIndexDefV8) -> Self {
        IndexSchema {
            index_id: IndexId(0), // Set to 0 as it may be assigned later.
            table_id,
            index_type: index.index_type,
            index_name: index.index_name.trim().into(),
            is_unique: index.is_unique,
            columns: index.columns,
        }
    }
}

impl From<IndexSchema> for RawIndexDefV8 {
    fn from(value: IndexSchema) -> Self {
        Self {
            index_name: value.index_name,
            columns: value.columns,
            is_unique: value.is_unique,
            index_type: value.index_type,
        }
    }
}

/// A struct representing the schema of a database constraint.
///
/// This struct holds information about a database constraint, including its unique identifier,
/// name, the table it belongs to, and the columns it is associated with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintSchema {
    /// The unique ID of the constraint within the database.
    pub constraint_id: ConstraintId,
    /// The name of the constraint.
    /// Deprecated, in the future constraints will be identified by the columns they refer to and their constraint type.
    pub constraint_name: Box<str>,
    /// The constraints applied to the columns
    pub constraints: Constraints,
    /// The ID of the table the constraint applies to.
    pub table_id: TableId,
    /// The columns the constraint applies to.
    pub columns: ColList,
}

impl ConstraintSchema {
    /// Constructs a `ConstraintSchema` from a given `ConstraintDef` and table identifier.
    ///
    /// # Parameters
    ///
    /// * `table_id`: Identifier of the table to which the constraint belongs.
    /// * `constraint`: The `ConstraintDef` containing constraint information.
    pub fn from_def(table_id: TableId, constraint: RawConstraintDefV8) -> Self {
        ConstraintSchema {
            constraint_id: ConstraintId(0), // Set to 0 as it may be assigned later.
            constraint_name: constraint.constraint_name.trim().into(),
            constraints: constraint.constraints,
            table_id,
            columns: constraint.columns,
        }
    }
}

impl From<ConstraintSchema> for RawConstraintDefV8 {
    fn from(value: ConstraintSchema) -> Self {
        Self {
            constraint_name: value.constraint_name,
            constraints: value.constraints,
            columns: value.columns,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_primitives::col_list;

    fn table_def() -> RawTableDefV8 {
        RawTableDefV8::new(
            "test".into(),
            vec![
                RawColumnDefV8::sys("id", AlgebraicType::U64),
                RawColumnDefV8::sys("name", AlgebraicType::String),
                RawColumnDefV8::sys("age", AlgebraicType::I16),
                RawColumnDefV8::sys("x", AlgebraicType::F32),
                RawColumnDefV8::sys("y", AlgebraicType::F64),
            ],
        )
    }

    // Verify we generate indexes from constraints
    #[test]
    fn test_idx_generated() {
        let t = table_def()
            .with_column_constraint(Constraints::unique(), ColId(0))
            .with_column_constraint(Constraints::unique(), col_list![0, 1])
            .with_column_constraint(Constraints::indexed(), ColId(1))
            .with_column_constraint(Constraints::primary_key(), ColId(2))
            //This will be ignored
            .with_column_constraint(Constraints::unset(), ColId(3));

        let mut s = TableSchema::from_def(TableId(0), t).validated().unwrap();
        s.indexes.sort_by_key(|x| x.columns.clone());

        #[rustfmt::skip]
        assert_eq!(
            s.indexes,
            vec![
                IndexSchema::from_def(TableId(0), RawIndexDefV8::btree("idx_test_id_unique".into(), ColId(0), true)),
                IndexSchema::from_def(TableId(0), RawIndexDefV8::btree("idx_test_id_name_unique".into(), col_list![0, 1], true)),
                IndexSchema::from_def(TableId(0), RawIndexDefV8::btree("idx_test_name_indexed_non_unique".into(), ColId(1), false)),
                IndexSchema::from_def(TableId(0), RawIndexDefV8::btree("idx_test_age_primary_key_unique".into(), ColId(2), true)),
            ]
        );
    }

    // Verify we generate sequences from constraints
    #[test]
    fn test_seq_generated() {
        let t = table_def()
            .with_column_constraint(Constraints::identity(), ColId(0))
            .with_column_constraint(Constraints::primary_key_identity(), ColId(1));

        let mut s = TableSchema::from_def(TableId(0), t).validated().unwrap();
        s.sequences.sort_by_key(|x| x.col_pos);

        #[rustfmt::skip]
        assert_eq!(
            s.sequences,
            vec![
                SequenceSchema::from_def(
                    TableId(0),
                    RawSequenceDefV8::for_column("test", "id_identity", ColId(0))
                ),
                SequenceSchema::from_def(
                    TableId(0),
                    RawSequenceDefV8::for_column("test", "name_primary_key_auto", ColId(1))
                ),
            ]
        );
    }

    // Verify we generate constraints from indexes
    #[test]
    fn test_ct_generated() {
        let t = table_def()
            .with_column_index(ColId(0), true)
            .with_column_index(ColId(1), false)
            .with_column_index(col_list![0, 1], true);

        let mut s = TableSchema::from_def(TableId(0), t).validated().unwrap();
        s.constraints.sort_by_key(|x| x.columns.clone());

        #[rustfmt::skip]
        assert_eq!(
            s.constraints,
            vec![
                ConstraintSchema::from_def(
                    TableId(0),
                    RawConstraintDefV8::new("ct_test_id_unique".into(), Constraints::unique(), ColId(0))
                ),
                ConstraintSchema::from_def(
                    TableId(0),
                    RawConstraintDefV8::new("ct_test_id_name_unique".into(), Constraints::unique(), col_list![0, 1])
                ),
                ConstraintSchema::from_def(
                    TableId(0),
                    RawConstraintDefV8::new("ct_test_name_indexed".into(), Constraints::indexed(), ColId(1))
                ),
            ]
        );
    }

    // Verify that if we add a Constraint + Index for the same column, we get at the end the correct definitions
    #[test]
    fn test_idx_ct_clash() {
        // The `Constraint::unset()` should be removed
        let t = table_def().with_column_index(ColId(0), true).with_constraints(
            table_def()
                .columns
                .iter()
                .enumerate()
                .map(|(pos, x)| {
                    RawConstraintDefV8::for_column("test", &x.col_name, Constraints::unset(), ColId(pos as u32))
                })
                .collect(),
        );

        let s = TableSchema::from_def(TableId(0), t).validated();
        assert!(s.is_ok());

        let s = s.unwrap();

        assert_eq!(
            s.indexes,
            vec![IndexSchema::from_def(
                TableId(0),
                RawIndexDefV8::btree("idx_test_id_unique".into(), ColId(0), true)
            )]
        );
        assert_eq!(
            s.constraints,
            vec![ConstraintSchema::from_def(
                TableId(0),
                RawConstraintDefV8::new("ct_test_id_unique".into(), Constraints::unique(), ColId(0))
            )]
        );

        // We got a duplication, both means 'UNIQUE'
        let t = table_def()
            .with_column_index(ColId(0), true)
            .with_column_constraint(Constraints::unique(), ColId(0));

        let s = TableSchema::from_def(TableId(0), t).validated();
        assert!(s.is_ok());

        let s = s.unwrap();

        assert_eq!(
            s.indexes,
            vec![IndexSchema::from_def(
                TableId(0),
                RawIndexDefV8::btree("idx_test_id_unique".into(), ColId(0), true)
            )]
        );
        assert_eq!(
            s.constraints,
            vec![ConstraintSchema::from_def(
                TableId(0),
                RawConstraintDefV8::new("ct_test_id_unique".into(), Constraints::unique(), ColId(0))
            )]
        );
    }

    // Not empty names
    #[test]
    fn test_validate_empty() {
        let t = table_def();

        // Empty names
        let mut t_name = t.clone();
        t_name.table_name = "".into();
        assert_eq!(
            TableSchema::from_def(TableId(0), t_name).validated(),
            Err(vec![SchemaError::EmptyTableName { table_id: TableId(0) }])
        );

        let mut t_col = t.clone();
        t_col.columns.push(RawColumnDefV8::sys("", AlgebraicType::U64));
        assert_eq!(
            TableSchema::from_def(TableId(0), t_col).validated(),
            Err(vec![SchemaError::EmptyName {
                table: "test".into(),
                ty: DefType::Column,
                id: 5
            },])
        );

        let mut t_ct = t.clone();
        t_ct.constraints
            .push(RawConstraintDefV8::new("".into(), Constraints::primary_key(), ColId(0)));
        assert_eq!(
            TableSchema::from_def(TableId(0), t_ct).validated(),
            Err(vec![SchemaError::EmptyName {
                table: "test".into(),
                ty: DefType::Constraint,
                id: 0,
            },])
        );

        // TODO(Tyler): I am disabling this because it's actually not correct.
        // Previously Mario was checking for __ to see if the name was missing from the
        // column, but it's completely valid for an index to have __ in the name.
        // This will have to be checked another way.
        //
        // let mut t_idx = t.clone();
        // t_idx.indexes.push(IndexDef::for_column("", "", ColId(0), false));
        // assert_eq!(
        //     t_idx.into_schema(TableId(0)).validated(),
        //     Err(vec![
        //         SchemaError::EmptyName {
        //             table: "test".to_string(),
        //             ty: DefType::Index,
        //             id: 0,
        //         },
        //         SchemaError::EmptyName {
        //             table: "test".to_string(),
        //             ty: DefType::Constraint,
        //             id: 0,
        //         },
        //     ])
        // );
        //
        // let mut t_seq = t.clone();
        // t_seq.sequences.push(RawSequenceDefV8::for_column("", "", ColId(0)));
        // assert_eq!(
        //     t_seq.into_schema(TableId(0)).validated(),
        //     Err(vec![
        //         SchemaError::EmptyName {
        //             table: "test".to_string(),
        //             ty: DefType::Sequence,
        //             id: 0,
        //         },
        //     ])
        // );
    }

    // Verify only one PK
    #[test]
    fn test_pkey() {
        let t = table_def()
            .with_column_constraint(Constraints::primary_key(), ColId(0))
            .with_column_constraint(Constraints::primary_key_auto(), ColId(1))
            .with_column_constraint(Constraints::primary_key_identity(), ColId(2));

        assert_eq!(
            TableSchema::from_def(TableId(0), t).validated(),
            Err(vec![SchemaError::MultiplePrimaryKeys {
                table: "test".into(),
                pks: vec!["id".into(), "name".into(), "age".into()],
            }])
        );
    }

    // All columns must exist
    #[test]
    fn test_column_exist() {
        let t = table_def()
            .with_column_sequence(ColId(1001))
            .with_column_constraint(Constraints::unique(), ColId(1002))
            .with_column_index(ColId(1003), false)
            .with_column_sequence(ColId(1004));

        let mut errs = TableSchema::from_def(TableId(0), t).validated().err().unwrap();
        errs.retain(|x| matches!(x, SchemaError::ColumnsNotFound { .. }));

        errs.sort_by_key(|x| {
            if let SchemaError::ColumnsNotFound { columns, name, .. } = x {
                (columns.clone(), name.clone())
            } else {
                (Vec::new(), "".into())
            }
        });

        assert_eq!(
            errs,
            vec![
                SchemaError::ColumnsNotFound {
                    name: "seq_test_".into(),
                    table: "test".into(),
                    columns: vec![ColId(1001)],
                    ty: DefType::Sequence,
                },
                SchemaError::ColumnsNotFound {
                    name: "ct_test__unique".into(),
                    table: "test".into(),
                    columns: vec![ColId(1002)],
                    ty: DefType::Constraint,
                },
                SchemaError::ColumnsNotFound {
                    name: "idx_test__unique".into(),
                    table: "test".into(),
                    columns: vec![ColId(1002)],
                    ty: DefType::Index,
                },
                SchemaError::ColumnsNotFound {
                    name: "ct_test__indexed".into(),
                    table: "test".into(),
                    columns: vec![ColId(1003)],
                    ty: DefType::Constraint,
                },
                SchemaError::ColumnsNotFound {
                    name: "idx_test__non_unique".into(),
                    table: "test".into(),
                    columns: vec![ColId(1003)],
                    ty: DefType::Index,
                },
                SchemaError::ColumnsNotFound {
                    name: "seq_test_".into(),
                    table: "test".into(),
                    columns: vec![ColId(1004)],
                    ty: DefType::Sequence,
                },
            ]
        );
    }

    // Only one auto_inc
    #[test]
    fn test_validate_auto_inc() {
        let t = table_def()
            .with_column_sequence(ColId(0))
            .with_column_sequence(ColId(0));

        assert_eq!(
            TableSchema::from_def(TableId(0), t).validated(),
            Err(vec![SchemaError::OneAutoInc {
                table: "test".into(),
                field: "id".into(),
            }])
        );
    }

    // Only BTree indexes
    #[test]
    fn test_validate_btree() {
        let t = table_def().with_indexes(vec![RawIndexDefV8 {
            index_name: "bad".into(),
            is_unique: false,
            index_type: IndexType::Hash,
            columns: ColList::new(0.into()),
        }]);

        assert_eq!(
            TableSchema::from_def(TableId(0), t).validated(),
            Err(vec![SchemaError::OnlyBtree {
                table: "test".into(),
                index: "bad".into(),
                index_type: IndexType::Hash,
            }])
        );
    }
}
