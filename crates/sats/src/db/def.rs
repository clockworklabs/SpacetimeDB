use derive_more::Display;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};

use crate::db::auth::{StAccess, StTableType};
use crate::db::error::{DefType, SchemaError};
use crate::product_value::InvalidFieldError;
use crate::relation::{Column, DbTable, FieldName, FieldOnly, Header, TableField};
use crate::{de, impl_deserialize, impl_serialize, ser};
use crate::{AlgebraicType, ProductType, ProductTypeElement};
use spacetimedb_primitives::*;

/// The default preallocation amount for sequences.
pub const SEQUENCE_PREALLOCATION_AMOUNT: i128 = 4_096;

impl_deserialize!([] Constraints, de => Self::try_from(de.deserialize_u8()?)
    .map_err(|_| de::Error::custom("invalid bitflags for `Constraints`"))
);
impl_serialize!([] Constraints, (self, ser) => ser.serialize_u8(self.bits()));

/// Represents a schema definition for a database sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceSchema {
    pub sequence_id: SequenceId,
    pub sequence_name: String,
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
    ///
    /// # Arguments
    ///
    /// * `table_id` - The ID of the table associated with the sequence.
    /// * `sequence` - The [SequenceDef] to derive the schema from.
    ///
    /// # Example
    ///
    /// ```
    /// use spacetimedb_sats::db::def::*;
    ///
    /// let sequence_def = SequenceDef::for_column("my_table".into(), "my_sequence".into(), 1.into());
    /// let schema = SequenceSchema::from_def(42.into(), sequence_def);
    ///
    /// assert_eq!(schema.sequence_name, "seq_my_table_my_sequence");
    /// assert_eq!(schema.table_id, 42.into());
    /// ```
    pub fn from_def(table_id: TableId, sequence: SequenceDef) -> Self {
        Self {
            sequence_id: SequenceId(0), // Will be replaced later when created
            sequence_name: sequence.sequence_name.trim().to_string(),
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

/// Represents a sequence definition for a database table column.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, ser::Serialize, de::Deserialize)]
#[sats(crate = crate)]
pub struct SequenceDef {
    pub sequence_name: String,
    /// The position of the column associated with this sequence.
    pub col_pos: ColId,
    pub increment: i128,
    pub start: Option<i128>,
    pub min_value: Option<i128>,
    pub max_value: Option<i128>,
    pub allocated: i128,
}

impl SequenceDef {
    /// Creates a new [SequenceDef] instance for a specific table and column.
    ///
    /// # Arguments
    ///
    /// * `table` - The name of the table.
    /// * `seq_name` - The name of the sequence.
    /// * `col_pos` - The position of the column in the `table`.
    ///
    /// # Example
    ///
    /// ```
    /// use spacetimedb_sats::db::def::*;
    ///
    /// let sequence_def = SequenceDef::for_column("my_table", "my_sequence", 1.into());
    /// assert_eq!(sequence_def.sequence_name, "seq_my_table_my_sequence");
    /// ```
    pub fn for_column(table: &str, column_or_name: &str, col_pos: ColId) -> Self {
        //removes the auto-generated suffix...
        let seq_name = column_or_name.trim_start_matches(&format!("ct_{}_", table));

        SequenceDef {
            sequence_name: format!("seq_{}_{}", table, seq_name),
            col_pos,
            increment: 1,
            start: None,
            min_value: None,
            max_value: None,
            allocated: SEQUENCE_PREALLOCATION_AMOUNT,
        }
    }
}

impl From<SequenceSchema> for SequenceDef {
    fn from(value: SequenceSchema) -> Self {
        SequenceDef {
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
    pub index_id: IndexId,
    pub table_id: TableId,
    pub index_type: IndexType,
    pub index_name: String,
    pub is_unique: bool,
    pub columns: ColList,
}

impl IndexSchema {
    /// Constructs an [IndexSchema] from a given [IndexDef] and `table_id`.
    pub fn from_def(table_id: TableId, index: IndexDef) -> Self {
        IndexSchema {
            index_id: IndexId(0), // Set to 0 as it may be assigned later.
            table_id,
            index_type: index.index_type,
            index_name: index.index_name.trim().to_string(),
            is_unique: index.is_unique,
            columns: index.columns,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Display, de::Deserialize, ser::Serialize)]
#[sats(crate = crate)]
pub enum IndexType {
    BTree = 0,
    Hash = 1,
}

impl From<IndexType> for u8 {
    fn from(value: IndexType) -> Self {
        value as u8
    }
}

impl TryFrom<u8> for IndexType {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(IndexType::BTree),
            1 => Ok(IndexType::Hash),
            _ => Err(()),
        }
    }
}

/// A struct representing the definition of a database index.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, ser::Serialize, de::Deserialize)]
#[sats(crate = crate)]
pub struct IndexDef {
    pub index_name: String,
    pub is_unique: bool,
    pub index_type: IndexType,
    /// List of column positions that compose the index.
    pub columns: ColList,
}

impl IndexDef {
    /// Creates a new [IndexDef] with the provided parameters.
    ///
    /// WARNING: Only [IndexType::Btree] are supported for now...
    ///
    /// # Arguments
    ///
    /// * `index_name`: The name of the index.
    /// * `columns`: List of column positions that compose the index.
    /// * `is_unique`: Indicates whether the index enforces uniqueness.
    pub fn btree(index_name: String, columns: impl Into<ColList>, is_unique: bool) -> Self {
        Self {
            columns: columns.into(),
            index_name,
            is_unique,
            index_type: IndexType::BTree,
        }
    }

    /// Creates an [IndexDef] of type [IndexType::BTree] for a specific column of a table.
    ///
    /// This method generates an index name based on the table name, index name, column positions, and uniqueness constraint.
    ///
    /// # Example
    ///
    /// ```
    /// use spacetimedb_primitives::ColList;
    /// use spacetimedb_sats::db::def::*;
    ///
    /// let index_def = IndexDef::for_column("my_table", "test", ColList::new(1u32.into()), true);
    /// assert_eq!(index_def.index_name, "idx_my_table_test_unique");
    /// ```
    pub fn for_column(table: &str, index_or_name: &str, columns: impl Into<ColList>, is_unique: bool) -> Self {
        let unique = if is_unique { "unique" } else { "non_unique" };

        // Removes the auto-generated suffix from the index name.
        let name = index_or_name.trim_start_matches(&format!("ct_{}_", table));

        // Constructs the index name using a predefined format.
        // No duplicate the `kind_name` that was added by an constraint
        if name.ends_with(&unique) {
            Self::btree(format!("idx_{table}_{name}"), columns, is_unique)
        } else {
            Self::btree(format!("idx_{table}_{name}_{unique}"), columns, is_unique)
        }
    }
}

impl From<IndexSchema> for IndexDef {
    fn from(value: IndexSchema) -> Self {
        Self {
            index_name: value.index_name,
            columns: value.columns,
            is_unique: value.is_unique,
            index_type: value.index_type,
        }
    }
}

/// A struct representing the schema of a database column.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct ColumnSchema {
    pub table_id: TableId,
    /// Position of the column within the table.
    pub col_pos: ColId,
    pub col_name: String,
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
    pub fn from_def(table_id: TableId, col_pos: ColId, column: ColumnDef) -> Self {
        ColumnSchema {
            table_id,
            col_pos,
            col_name: column.col_name.trim().into(),
            col_type: column.col_type,
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

/// For get the original `table_name` for where a [ColumnSchema] belongs.
#[derive(Debug, Clone)]
pub struct FieldDef<'a> {
    pub column: &'a ColumnSchema,
    pub table_name: &'a str,
}

impl From<FieldDef<'_>> for FieldName {
    fn from(value: FieldDef) -> Self {
        FieldName::named(value.table_name, &value.column.col_name)
    }
}

impl From<FieldDef<'_>> for ProductTypeElement {
    fn from(value: FieldDef) -> Self {
        ProductTypeElement::new(value.column.col_type.clone(), Some(value.column.col_name.clone()))
    }
}

/// A struct representing the definition of a database column.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, ser::Serialize, de::Deserialize)]
#[sats(crate = crate)]
pub struct ColumnDef {
    pub col_name: String,
    pub col_type: AlgebraicType,
}

impl From<ProductType> for Vec<ColumnDef> {
    fn from(value: ProductType) -> Self {
        value
            .elements
            .into_iter()
            .enumerate()
            .map(|(pos, col)| {
                let col_name = if let Some(name) = col.name {
                    name
                } else {
                    format!("col_{pos}")
                };

                ColumnDef {
                    col_name,
                    col_type: col.algebraic_type,
                }
            })
            .collect()
    }
}

impl ColumnDef {
    /// Creates a new [ColumnDef] for a system field with the specified data type.
    ///
    /// This method is typically used to define system columns with predefined names and data types.
    ///
    /// # Arguments
    ///
    /// * `field_name`: The name for which to create a column definition.
    /// * `col_type`: The [AlgebraicType] of the column.
    ///
    pub fn sys(field_name: &str, col_type: AlgebraicType) -> Self {
        Self {
            col_name: field_name.into(),
            col_type,
        }
    }
}

impl From<ColumnSchema> for ColumnDef {
    fn from(value: ColumnSchema) -> Self {
        Self {
            col_name: value.col_name,
            col_type: value.col_type,
        }
    }
}

/// A struct representing the schema of a database constraint.
///
/// This struct holds information about a database constraint, including its unique identifier,
/// name, type (kind), the table it belongs to, and the columns it is associated with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintSchema {
    pub constraint_id: ConstraintId,
    pub constraint_name: String,
    pub constraints: Constraints,
    pub table_id: TableId,
    pub columns: ColList,
}

impl ConstraintSchema {
    /// Constructs a `ConstraintSchema` from a given `ConstraintDef` and table identifier.
    ///
    /// # Arguments
    ///
    /// * `table_id`: Identifier of the table to which the constraint belongs.
    /// * `constraint`: The `ConstraintDef` containing constraint information.
    pub fn from_def(table_id: TableId, constraint: ConstraintDef) -> Self {
        ConstraintSchema {
            constraint_id: ConstraintId(0), // Set to 0 as it may be assigned later.
            constraint_name: constraint.constraint_name.trim().to_string(),
            constraints: constraint.constraints,
            table_id,
            columns: constraint.columns,
        }
    }
}

/// A struct representing the definition of a database constraint.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, ser::Serialize, de::Deserialize)]
#[sats(crate = crate)]
pub struct ConstraintDef {
    pub constraint_name: String,
    pub constraints: Constraints,
    /// List of column positions associated with the constraint.
    pub columns: ColList,
}

impl ConstraintDef {
    /// Creates a new [ConstraintDef] with the specified parameters.
    ///
    /// # Arguments
    ///
    /// * `constraint_name`: The name of the constraint.
    /// * `constraints`: The constraints.
    /// * `columns`: List of column positions associated with the constraint.
    pub fn new(constraint_name: String, constraints: Constraints, columns: impl Into<ColList>) -> Self {
        Self {
            constraint_name,
            constraints,
            columns: columns.into(),
        }
    }

    /// Creates a `ConstraintDef` for a specific column of a table.
    ///
    /// This method generates a constraint name based on the table name, column name, and constraint type.
    ///
    /// # Arguments
    ///
    /// * `table`: The name of the table to which the constraint belongs.
    /// * `column_name`: The name of the column associated with the constraint.
    /// * `constraints`: The constraints.
    /// * `columns`: List of column positions associated with the constraint.
    ///
    /// # Example
    ///
    /// ```
    /// use spacetimedb_primitives::{Constraints, ColList};
    /// use spacetimedb_sats::db::def::*;
    ///
    /// let constraint_def = ConstraintDef::for_column("my_table", "test", Constraints::identity(), ColList::new(1u32.into()));
    /// assert_eq!(constraint_def.constraint_name, "ct_my_table_test_identity");
    /// ```
    pub fn for_column(
        table: &str,
        column_or_name: &str,
        constraints: Constraints,
        columns: impl Into<ColList>,
    ) -> Self {
        //removes the auto-generated suffix...
        let name = column_or_name.trim_start_matches(&format!("idx_{}_", table));

        let kind_name = format!("{:?}", constraints.kind()).to_lowercase();
        // No duplicate the `kind_name` that was added by an index
        if name.ends_with(&kind_name) {
            Self::new(format!("ct_{table}_{name}"), constraints, columns)
        } else {
            Self::new(format!("ct_{table}_{name}_{kind_name}"), constraints, columns)
        }
    }
}

impl From<ConstraintSchema> for ConstraintDef {
    fn from(value: ConstraintSchema) -> Self {
        Self {
            constraint_name: value.constraint_name,
            constraints: value.constraints,
            columns: value.columns,
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
    pub table_name: String,
    columns: Vec<ColumnSchema>,
    pub indexes: Vec<IndexSchema>,
    pub constraints: Vec<ConstraintSchema>,
    pub sequences: Vec<SequenceSchema>,
    pub table_type: StTableType,
    pub table_access: StAccess,
    /// Cache for `row_type_for_table` in the data store.
    row_type: ProductType,
}

impl TableSchema {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        table_id: TableId,
        table_name: String,
        columns: Vec<ColumnSchema>,
        indexes: Vec<IndexSchema>,
        constraints: Vec<ConstraintSchema>,
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

    /// Check if the specified `field` exists in this [TableSchema].
    ///
    /// This function can handle both named and positional fields.
    ///
    /// # Warning
    ///
    /// This function ignores the `table_name` when searching for a column.
    pub fn get_column_by_field(&self, field: &FieldName) -> Option<&ColumnSchema> {
        match field.field() {
            FieldOnly::Name(x) => self.get_column_by_name(x),
            FieldOnly::Pos(x) => self.get_column(x),
        }
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
        self.columns.iter().find(|x| x.col_name == col_name)
    }

    /// Check if there is an index for this [FieldName]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_index_by_field(&self, field: &FieldName) -> Option<&IndexSchema> {
        let ColumnSchema { col_pos, .. } = self.get_column_by_field(field)?;
        self.indexes.iter().find(|IndexSchema { columns, .. }| {
            let mut cols = columns.iter();
            cols.next() == Some(*col_pos) && cols.next().is_none()
        })
    }

    /// Turn a [TableField] that could be an unqualified field `id` into `table.id`
    pub fn normalize_field(&self, or_use: &TableField) -> FieldName {
        FieldName::named(or_use.table.unwrap_or(&self.table_name), or_use.field)
    }

    /// Project the fields from the supplied `indexes`.
    pub fn project(&self, indexes: impl Iterator<Item = ColId>) -> Result<Vec<&ColumnSchema>, InvalidFieldError> {
        indexes
            .map(|index| {
                self.get_column(index.0 as usize).ok_or(InvalidFieldError {
                    col_pos: index,
                    name: None,
                })
            })
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

    pub fn get_constraints(&self) -> Vec<(ColList, Constraints)> {
        self.constraints
            .iter()
            .map(|x| (x.columns.clone(), x.constraints))
            .collect()
    }

    /// Create a new [TableSchema] from a [TableDef] and a `table_id`.
    ///
    /// # Parameters
    ///
    /// - `table_id`: The unique identifier for the table.
    /// - `schema`: The `TableDef` containing the schema information.
    pub fn from_def(table_id: TableId, schema: TableDef) -> Self {
        let indexes = schema.generated_indexes().collect::<Vec<_>>();
        let sequences = schema.generated_sequences().collect::<Vec<_>>();
        let constraints = schema.generated_constraints().collect::<Vec<_>>();
        //Sort by columns so is likely to get PK first then the rest and maintain the order for
        //testing.
        TableSchema::new(
            table_id,
            schema.table_name.trim().to_string(),
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
        )
    }

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
                                    format!("col_{col}")
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
        DbTable::new(value.into(), value.table_id, value.table_type, value.table_access)
    }
}

impl From<&TableSchema> for Header {
    fn from(value: &TableSchema) -> Self {
        let constraints = value.get_constraints();
        let fields = value
            .columns
            .iter()
            .enumerate()
            .map(|(pos, x)| {
                Column::new(
                    FieldName::named(&value.table_name, &x.col_name),
                    x.col_type.clone(),
                    ColId(pos as u32),
                )
            })
            .collect();

        Header::new(value.table_name.clone(), fields, constraints)
    }
}

/// A data structure representing the definition of a database table.
///
/// This struct holds information about the table, including its name, columns, indexes,
/// constraints, sequences, type, and access rights.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, ser::Serialize, de::Deserialize)]
#[sats(crate = crate)]
pub struct TableDef {
    pub table_name: String,
    pub columns: Vec<ColumnDef>,
    pub indexes: Vec<IndexDef>,
    pub constraints: Vec<ConstraintDef>,
    pub sequences: Vec<SequenceDef>,
    pub table_type: StTableType,
    pub table_access: StAccess,
}

impl TableDef {
    /// Create a new `TableDef` instance with the specified `table_name` and `columns`.
    ///
    /// # Parameters
    ///
    /// - `table_name`: The name of the table.
    /// - `columns`: A `vec` of `ColumnDef` instances representing the columns of the table.
    ///
    pub fn new(table_name: String, columns: Vec<ColumnDef>) -> Self {
        Self {
            table_name,
            columns,
            indexes: vec![],
            constraints: vec![],
            sequences: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }
    }

    /// Set the type of the table and return a new `TableDef` instance with the updated type.
    pub fn with_type(self, table_type: StTableType) -> Self {
        let mut x = self;
        x.table_type = table_type;
        x
    }

    /// Set the access rights for the table and return a new `TableDef` instance with the updated access rights.
    pub fn with_access(self, table_access: StAccess) -> Self {
        let mut x = self;
        x.table_access = table_access;
        x
    }

    /// Set the constraints for the table and return a new `TableDef` instance with the updated constraints.
    pub fn with_constraints(self, constraints: Vec<ConstraintDef>) -> Self {
        let mut x = self;
        x.constraints = constraints;
        x
    }

    /// Concatenate the column names from the `columns`
    ///
    /// WARNING: If the `ColId` not exist, is skipped.
    /// TODO(Tyler): This should return an error and not allow this to be constructed
    /// if there is an invalid `ColId`
    fn generate_cols_name(&self, columns: &ColList) -> String {
        let mut column_name = Vec::with_capacity(columns.len() as usize);
        for col_pos in columns.iter() {
            if let Some(col) = self.get_column(col_pos.idx()) {
                column_name.push(col.col_name.as_str())
            }
        }

        column_name.join("_")
    }

    /// Generate a [ConstraintDef] using the supplied `columns`.
    pub fn with_column_constraint(self, kind: Constraints, columns: impl Into<ColList>) -> Self {
        let mut x = self;
        let columns = columns.into();

        x.constraints.push(ConstraintDef::for_column(
            &x.table_name,
            &x.generate_cols_name(&columns),
            kind,
            columns,
        ));
        x
    }

    /// Set the indexes for the table and return a new `TableDef` instance with the updated indexes.
    pub fn with_indexes(self, indexes: Vec<IndexDef>) -> Self {
        let mut x = self;
        x.indexes = indexes;
        x
    }

    /// Generate a [IndexDef] using the supplied `columns`.
    pub fn with_column_index(self, columns: impl Into<ColList>, is_unique: bool) -> Self {
        let mut x = self;
        let columns = columns.into();
        x.indexes.push(IndexDef::for_column(
            &x.table_name,
            &x.generate_cols_name(&columns),
            columns,
            is_unique,
        ));
        x
    }

    /// Set the sequences for the table and return a new `TableDef` instance with the updated sequences.
    pub fn with_sequences(self, sequences: Vec<SequenceDef>) -> Self {
        let mut x = self;
        x.sequences = sequences;
        x
    }

    /// Generate a [SequenceDef] using the supplied `columns`.
    pub fn with_column_sequence(self, columns: ColId) -> Self {
        let mut x = self;

        x.sequences.push(SequenceDef::for_column(
            &x.table_name,
            &x.generate_cols_name(&ColList::new(columns)),
            columns,
        ));
        x
    }

    /// Create a `TableDef` from a product type and table name.
    ///
    /// NOTE: If the [ProductType.name] is `None` then it auto-generate a name like `col_{col_pos}`
    pub fn from_product(table_name: &str, row: ProductType) -> Self {
        Self::new(
            table_name.into(),
            row.elements
                .into_iter()
                .enumerate()
                .map(|(col_pos, e)| ColumnDef {
                    col_name: e.name.unwrap_or_else(|| format!("col_{col_pos}")),
                    col_type: e.algebraic_type,
                })
                .collect::<Vec<_>>(),
        )
    }

    /// Get an iterator deriving [IndexDef] from the constraints that require them like `UNIQUE`.
    ///
    /// It looks into [Self::constraints] for possible duplicates and remove them from the result
    pub fn generated_indexes(&self) -> impl Iterator<Item = IndexDef> + '_ {
        self.constraints.iter().filter_map(|x| {
            if x.constraints.has_indexed() {
                let is_unique = x.constraints.has_unique();
                let idx = IndexDef::for_column(&self.table_name, &x.constraint_name, x.columns.clone(), is_unique);
                if self
                    .indexes
                    .binary_search_by(|x| x.index_name.cmp(&idx.index_name))
                    .is_ok()
                {
                    return None;
                }
                Some(idx)
            } else {
                None
            }
        })
    }

    /// Get an iterator deriving [SequenceDef] from the constraints that require them like `IDENTITY`.
    ///
    /// It looks into [Self::constraints] for possible duplicates and remove them from the result
    pub fn generated_sequences(&self) -> impl Iterator<Item = SequenceDef> + '_ {
        self.constraints.iter().filter_map(|x| {
            if x.constraints.has_autoinc() {
                let col_id = x.columns.head();

                let seq = SequenceDef::for_column(&self.table_name, &x.constraint_name, col_id);
                if self
                    .sequences
                    .binary_search_by(|x| x.sequence_name.cmp(&seq.sequence_name))
                    .is_ok()
                {
                    return None;
                }
                Some(seq)
            } else {
                None
            }
        })
    }

    /// Get an iterator deriving [ConstraintDef] from the indexes that require them like `UNIQUE`.
    ///
    /// It looks into [Self::constraints] for possible duplicates and remove them from the result
    pub fn generated_constraints(&self) -> impl Iterator<Item = ConstraintDef> + '_ {
        let cols: HashSet<_> = self
            .constraints
            .iter()
            .filter_map(|x| {
                if x.constraints.kind() != ConstraintKind::UNSET {
                    Some(&x.columns)
                } else {
                    None
                }
            })
            .collect();

        self.indexes.iter().filter_map(move |idx| {
            if !cols.contains(&idx.columns) {
                Some(ConstraintDef::for_column(
                    &self.table_name,
                    &idx.index_name,
                    if idx.is_unique {
                        Constraints::unique()
                    } else {
                        Constraints::indexed()
                    },
                    idx.columns.clone(),
                ))
            } else {
                None
            }
        })
    }

    /// Create a new [TableSchema] from [Self] and a `table id`.
    pub fn into_schema(self, table_id: TableId) -> TableSchema {
        TableSchema::from_def(table_id, self)
    }

    /// Check if the `name` of the [FieldName] exist on this [TableDef]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_field(&self, field: &FieldName) -> Option<&ColumnDef> {
        match field.field() {
            FieldOnly::Name(x) => self.get_column_by_name(x),
            FieldOnly::Pos(x) => self.get_column(x),
        }
    }

    pub fn get_column(&self, pos: usize) -> Option<&ColumnDef> {
        self.columns.get(pos)
    }

    /// Check if the `col_name` exist on this [TableSchema]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_name(&self, col_name: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|x| x.col_name == col_name)
    }
}

impl From<TableSchema> for TableDef {
    fn from(value: TableSchema) -> Self {
        Self {
            table_name: value.table_name,
            columns: value.columns.into_iter().map(Into::into).collect(),
            indexes: value.indexes.into_iter().map(Into::into).collect(),
            constraints: value.constraints.into_iter().map(Into::into).collect(),
            sequences: value.sequences.into_iter().map(Into::into).collect(),
            table_type: value.table_type,
            table_access: value.table_access,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_primitives::col_list;

    fn table_def() -> TableDef {
        TableDef::new(
            "test".into(),
            vec![
                ColumnDef::sys("id", AlgebraicType::U64),
                ColumnDef::sys("name", AlgebraicType::String),
                ColumnDef::sys("age", AlgebraicType::I16),
                ColumnDef::sys("x", AlgebraicType::F32),
                ColumnDef::sys("y", AlgebraicType::F64),
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

        let mut s = t.into_schema(TableId(0)).validated().unwrap();
        s.indexes.sort_by_key(|x| x.columns.clone());

        #[rustfmt::skip]
        assert_eq!(
            s.indexes,
            vec![
                IndexSchema::from_def(TableId(0), IndexDef::btree("idx_test_id_unique".into(), ColId(0), true)),
                IndexSchema::from_def(TableId(0), IndexDef::btree("idx_test_id_name_unique".into(), col_list![0, 1], true)),
                IndexSchema::from_def(TableId(0), IndexDef::btree("idx_test_name_indexed_non_unique".into(), ColId(1), false)),
                IndexSchema::from_def(TableId(0), IndexDef::btree("idx_test_age_primary_key_unique".into(), ColId(2), true)),
            ]
        );
    }

    // Verify we generate sequences from constraints
    #[test]
    fn test_seq_generated() {
        let t = table_def()
            .with_column_constraint(Constraints::identity(), ColId(0))
            .with_column_constraint(Constraints::primary_key_identity(), ColId(1));

        let mut s = t.into_schema(TableId(0)).validated().unwrap();
        s.sequences.sort_by_key(|x| x.col_pos);

        #[rustfmt::skip]
        assert_eq!(
            s.sequences,
            vec![
                SequenceSchema::from_def(
                    TableId(0),
                    SequenceDef::for_column("test", "id_identity", ColId(0))
                ),
                SequenceSchema::from_def(
                    TableId(0),
                    SequenceDef::for_column("test", "name_primary_key_auto", ColId(1))
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

        let mut s = t.into_schema(TableId(0)).validated().unwrap();
        s.constraints.sort_by_key(|x| x.columns.clone());

        #[rustfmt::skip]
        assert_eq!(
            s.constraints,
            vec![
                ConstraintSchema::from_def(
                    TableId(0),
                    ConstraintDef::new("ct_test_id_unique".into(), Constraints::unique(), ColId(0))
                ),
                ConstraintSchema::from_def(
                    TableId(0),
                    ConstraintDef::new("ct_test_id_name_unique".into(), Constraints::unique(), col_list![0, 1])
                ),
                ConstraintSchema::from_def(
                    TableId(0),
                    ConstraintDef::new("ct_test_name_non_unique_indexed".into(), Constraints::indexed(), ColId(1))
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
                .map(|(pos, x)| ConstraintDef::for_column("test", &x.col_name, Constraints::unset(), ColId(pos as u32)))
                .collect(),
        );

        let s = t.into_schema(TableId(0)).validated();
        assert!(s.is_ok());

        let s = s.unwrap();

        assert_eq!(
            s.indexes,
            vec![IndexSchema::from_def(
                TableId(0),
                IndexDef::btree("idx_test_id_unique".into(), ColId(0), true)
            )]
        );
        assert_eq!(
            s.constraints,
            vec![ConstraintSchema::from_def(
                TableId(0),
                ConstraintDef::new("ct_test_id_unique".into(), Constraints::unique(), ColId(0))
            )]
        );

        // We got a duplication, both means 'UNIQUE'
        let t = table_def()
            .with_column_index(ColId(0), true)
            .with_column_constraint(Constraints::unique(), ColId(0));

        let s = t.into_schema(TableId(0)).validated();
        assert!(s.is_ok());

        let s = s.unwrap();

        assert_eq!(
            s.indexes,
            vec![IndexSchema::from_def(
                TableId(0),
                IndexDef::btree("idx_test_id_unique".into(), ColId(0), true)
            )]
        );
        assert_eq!(
            s.constraints,
            vec![ConstraintSchema::from_def(
                TableId(0),
                ConstraintDef::new("ct_test_id_unique".into(), Constraints::unique(), ColId(0))
            )]
        );
    }

    // Not empty names
    #[test]
    fn test_validate_empty() {
        let t = table_def();

        // Empty names
        let mut t_name = t.clone();
        t_name.table_name.clear();
        assert_eq!(
            t_name.into_schema(TableId(0)).validated(),
            Err(vec![SchemaError::EmptyTableName { table_id: TableId(0) }])
        );

        let mut t_col = t.clone();
        t_col.columns.push(ColumnDef::sys("", AlgebraicType::U64));
        assert_eq!(
            t_col.into_schema(TableId(0)).validated(),
            Err(vec![SchemaError::EmptyName {
                table: "test".to_string(),
                ty: DefType::Column,
                id: 5
            },])
        );

        let mut t_ct = t.clone();
        t_ct.constraints
            .push(ConstraintDef::new("".into(), Constraints::primary_key(), ColId(0)));
        assert_eq!(
            t_ct.into_schema(TableId(0)).validated(),
            Err(vec![SchemaError::EmptyName {
                table: "test".to_string(),
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
        // t_seq.sequences.push(SequenceDef::for_column("", "", ColId(0)));
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
            t.into_schema(TableId(0)).validated(),
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

        let mut errs = t.into_schema(TableId(0)).validated().err().unwrap();
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
                    name: "seq_test_".to_string(),
                    table: "test".into(),
                    columns: vec![ColId(1001)],
                    ty: DefType::Sequence,
                },
                SchemaError::ColumnsNotFound {
                    name: "ct_test__unique".to_string(),
                    table: "test".into(),
                    columns: vec![ColId(1002)],
                    ty: DefType::Constraint,
                },
                SchemaError::ColumnsNotFound {
                    name: "idx_test__unique".to_string(),
                    table: "test".into(),
                    columns: vec![ColId(1002)],
                    ty: DefType::Index,
                },
                SchemaError::ColumnsNotFound {
                    name: "ct_test__non_unique_indexed".to_string(),
                    table: "test".into(),
                    columns: vec![ColId(1003)],
                    ty: DefType::Constraint,
                },
                SchemaError::ColumnsNotFound {
                    name: "idx_test__non_unique".to_string(),
                    table: "test".into(),
                    columns: vec![ColId(1003)],
                    ty: DefType::Index,
                },
                SchemaError::ColumnsNotFound {
                    name: "seq_test_".to_string(),
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
            t.into_schema(TableId(0)).validated(),
            Err(vec![SchemaError::OneAutoInc {
                table: "test".into(),
                field: "id".into(),
            }])
        );
    }

    // Only BTree indexes
    #[test]
    fn test_validate_btree() {
        let t = table_def().with_indexes(vec![IndexDef {
            index_name: "bad".to_string(),
            is_unique: false,
            index_type: IndexType::Hash,
            columns: ColList::new(0.into()),
        }]);

        assert_eq!(
            t.into_schema(TableId(0)).validated(),
            Err(vec![SchemaError::OnlyBtree {
                table: "test".into(),
                index: "bad".to_string(),
                index_type: IndexType::Hash,
            }])
        );
    }
}
