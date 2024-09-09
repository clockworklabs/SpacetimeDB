//! Schema data structures.
//! These are used at runtime by the vm to store the schema of the database.
//! They are mirrored in the system tables -- see `spacetimedb_core::db::datastore::system_tables`.
//! Types in this file are not public ABI or API and may be changed at any time; it's the system tables that cannot.

// TODO(1.0): change all the `Box<str>`s in this file to `Identifier`.
// This doesn't affect the ABI so can wait until 1.0.

use itertools::Itertools;
use spacetimedb_lib::db::auth::{StAccess, StTableType};
use spacetimedb_lib::db::error::{DefType, SchemaError};
use spacetimedb_lib::db::raw_def::{generate_cols_name, RawConstraintDefV8};
use spacetimedb_lib::relation::{Column, DbTable, FieldName, Header};
use spacetimedb_lib::{AlgebraicType, ProductType, ProductTypeElement};
use spacetimedb_primitives::*;
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::WithTypespace;
use std::sync::Arc;

use crate::def::{
    ColumnDef, ConstraintData, ConstraintDef, IndexAlgorithm, IndexDef, ModuleDef, ModuleDefLookup, ScheduleDef,
    SequenceDef, TableDef, UniqueConstraintData,
};
use crate::identifier::Identifier;

/// Helper trait documenting allowing schema entities to be built from a validated `ModuleDef`.
pub trait Schema: Sized {
    /// The `Def` type corresponding to this schema type.
    type Def: ModuleDefLookup;
    /// The `Id` type corresponding to this schema type.
    type Id;
    /// The `Id` type corresponding to the parent of this schema type.
    /// Set to `()` if there is no parent.
    type ParentId;

    /// Construct a schema entity from a validated `ModuleDef`.
    /// Panics if `module_def` does not contain `def`.
    fn from_module_def(module_def: &ModuleDef, def: &Self::Def, parent_id: Self::ParentId, id: Self::Id) -> Self;

    /// Check that a schema entity is compatible with a definition.
    fn check_compatible(&self, def: &Self::Def) -> Result<(), anyhow::Error>;
}

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

    /// The primary key of the table, if present. Must refer to a valid column.
    ///
    /// Currently, there must be a unique constraint and an index corresponding to the primary key.
    /// Eventually, we may remove the requirement for an index.
    ///
    /// The database engine does not actually care about this, but client code generation does.
    pub primary_key: Option<ColId>,

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

    /// The schedule for the table, if present.
    pub schedule: Option<ScheduleSchema>,

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
        schedule: Option<ScheduleSchema>,
        primary_key: Option<ColId>,
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
            schedule,
            primary_key,
        }
    }

    /// Create a `TableSchema` corresponding to a product type.
    /// For use in tests.
    #[cfg(feature = "test")]
    pub fn from_product_type(ty: ProductType) -> TableSchema {
        let columns = ty
            .elements
            .iter()
            .enumerate()
            .map(|(col_pos, element)| ColumnSchema {
                table_id: TableId(0),
                col_pos: ColId(col_pos as _),
                col_name: element.name.clone().unwrap_or_else(|| format!("col{}", col_pos).into()),
                col_type: element.algebraic_type.clone(),
            })
            .collect();

        TableSchema::new(
            TableId(0),
            "TestTable".into(),
            columns,
            vec![],
            vec![],
            vec![],
            StTableType::User,
            StAccess::Public,
            None,
            None,
        )
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
    pub fn get_constraints(&self) -> impl Iterator<Item = &ConstraintData> {
        self.constraints.iter().map(|x| &x.data)
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

        if self.table_name.is_empty() {
            errors.push(SchemaError::EmptyTableName {
                table_id: self.table_id,
            });
        }

        let columns_not_found = self
            .sequences
            .iter()
            .map(|x| (DefType::Sequence, x.sequence_name.clone(), ColList::new(x.col_pos)))
            .chain(self.indexes.iter().map(|x| match &x.index_algorithm {
                IndexAlgorithm::BTree(btree) => (DefType::Index, x.index_name.clone(), btree.columns.clone()),
            }))
            .chain(self.constraints.iter().map(|x| {
                (
                    DefType::Constraint,
                    x.constraint_name.clone(),
                    match &x.data {
                        ConstraintData::Unique(unique) => unique.columns.clone().into(),
                    },
                )
            }))
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
                    id: x.col_pos.0 as _,
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

    /// The C# and Rust SDKs are inconsistent about whether v8 column defs store resolved or unresolved algebraic types.
    /// This method works around this problem by copying the column types from the module def into the table schema.
    /// It can be removed once v8 is removed, since v9 will reject modules with an inconsistency like this.
    pub fn janky_fix_column_defs(&mut self, module_def: &ModuleDef) {
        let table_name = Identifier::new(self.table_name.clone()).unwrap();
        for col in &mut self.columns {
            let def: &ColumnDef = module_def
                .lookup((&table_name, &Identifier::new(col.col_name.clone()).unwrap()))
                .unwrap();
            col.col_type = def.ty.clone();
        }
        let table_def: &TableDef = module_def.expect_lookup(&table_name);
        self.row_type = module_def.typespace()[table_def.product_type_ref]
            .as_product()
            .unwrap()
            .clone();
    }

    /// Normalize a `TableSchema`.
    /// The result is semantically equivalent, but may have reordered indexes, constraints, or sequences.
    /// Columns will not be reordered.
    pub fn normalize(&mut self) {
        self.indexes.sort_by(|a, b| a.index_name.cmp(&b.index_name));
        self.constraints
            .sort_by(|a, b| a.constraint_name.cmp(&b.constraint_name));
        self.sequences.sort_by(|a, b| a.sequence_name.cmp(&b.sequence_name));
    }
}

/// Like `assert_eq!` for `anyhow`, but `$msg` is just a string, not a format string.
macro_rules! ensure_eq {
    ($a:expr, $b:expr, $msg:expr) => {
        if $a != $b {
            anyhow::bail!(
                "{0}: expected {1} == {2}:\n   {1}: {3:?}\n   {2}: {4:?}",
                $msg,
                stringify!($a),
                stringify!($b),
                $a,
                $b
            );
        }
    };
}

impl Schema for TableSchema {
    type Def = TableDef;
    type Id = TableId;
    type ParentId = ();

    // N.B. This implementation gives all children ID 0 (the auto-inc sentinel value.)
    fn from_module_def(
        module_def: &ModuleDef,
        def: &Self::Def,
        _parent_id: Self::ParentId,
        table_id: Self::Id,
    ) -> Self {
        module_def.expect_contains(def);

        let TableDef {
            name,
            product_type_ref: _,
            primary_key,
            columns,
            indexes,
            constraints,
            sequences,
            schedule,
            table_type,
            table_access,
        } = def;

        let columns: Vec<ColumnSchema> = columns
            .iter()
            .enumerate()
            .map(|(col_pos, def)| ColumnSchema::from_module_def(module_def, def, (), (table_id, col_pos.into())))
            .collect();

        // note: these Ids are fixed up somewhere else, so we can just use 0 here...
        // but it would be nice to pass the correct values into this method.
        let indexes = indexes
            .values()
            .map(|def| IndexSchema::from_module_def(module_def, def, table_id, IndexId(0)))
            .collect();

        let sequences = sequences
            .values()
            .map(|def| SequenceSchema::from_module_def(module_def, def, table_id, SequenceId(0)))
            .collect();

        let constraints = constraints
            .values()
            .map(|def| ConstraintSchema::from_module_def(module_def, def, table_id, ConstraintId(0)))
            .collect();

        let schedule = schedule
            .as_ref()
            .map(|schedule| ScheduleSchema::from_module_def(module_def, schedule, table_id, ScheduleId(0)));

        TableSchema::new(
            table_id,
            (*name).clone().into(),
            columns,
            indexes,
            constraints,
            sequences,
            (*table_type).into(),
            (*table_access).into(),
            schedule,
            primary_key.clone(),
        )
    }

    fn check_compatible(&self, def: &Self::Def) -> Result<(), anyhow::Error> {
        ensure_eq!(&self.table_name[..], &def.name[..], "Table name mismatch");
        ensure_eq!(self.primary_key, def.primary_key, "Primary key mismatch");
        let def_table_access: StAccess = (def.table_access).into();
        ensure_eq!(self.table_access, def_table_access, "Table access mismatch");
        let def_table_type: StTableType = (def.table_type).into();
        ensure_eq!(self.table_type, def_table_type, "Table type mismatch");

        for col in &self.columns {
            let col_def = def
                .columns
                .get(col.col_pos.0 as usize)
                .ok_or_else(|| anyhow::anyhow!("Column {} not found in definition", col.col_pos.0))?;
            col.check_compatible(col_def)?;
        }
        ensure_eq!(self.columns.len(), def.columns.len(), "Column count mismatch");

        for index in &self.indexes {
            let index_def = def
                .indexes
                .get(&index.index_name[..])
                .ok_or_else(|| anyhow::anyhow!("Index {} not found in definition", index.index_id.0))?;
            index.check_compatible(index_def)?;
        }
        ensure_eq!(self.indexes.len(), def.indexes.len(), "Index count mismatch");

        for constraint in &self.constraints {
            let constraint_def = def
                .constraints
                .get(&constraint.constraint_name[..])
                .ok_or_else(|| anyhow::anyhow!("Constraint {} not found in definition", constraint.constraint_id.0))?;
            constraint.check_compatible(constraint_def)?;
        }
        ensure_eq!(
            self.constraints.len(),
            def.constraints.len(),
            "Constraint count mismatch"
        );

        for sequence in &self.sequences {
            let sequence_def = def
                .sequences
                .get(&sequence.sequence_name[..])
                .ok_or_else(|| anyhow::anyhow!("Sequence {} not found in definition", sequence.sequence_id.0))?;
            sequence.check_compatible(sequence_def)?;
        }
        ensure_eq!(self.sequences.len(), def.sequences.len(), "Sequence count mismatch");

        if let Some(schedule) = &self.schedule {
            let schedule_def = def
                .schedule
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Schedule not found in definition"))?;
            schedule.check_compatible(schedule_def)?;
        }
        ensure_eq!(
            self.schedule.is_some(),
            def.schedule.is_some(),
            "Schedule presence mismatch"
        );
        Ok(())
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

        // Note that repeated col lists here will be flattened by the `Header` constructor.
        let constraints = value
            .get_constraints()
            .map(|x| -> (ColList, Constraints) {
                match x {
                    ConstraintData::Unique(unique) => (unique.columns.clone().into(), Constraints::unique()),
                }
            })
            .chain(value.indexes.iter().map(|x| match &x.index_algorithm {
                IndexAlgorithm::BTree(btree) => (btree.columns.clone(), Constraints::indexed()),
            }))
            .chain(
                value
                    .sequences
                    .iter()
                    .map(|x| (ColList::new(x.col_pos), Constraints::auto_inc())),
            )
            .chain(
                value
                    .primary_key
                    .iter()
                    .map(|x| (col_list![*x], Constraints::primary_key())),
            );

        Header::new(value.table_id, value.table_name.clone(), fields, constraints)
    }
}

impl From<TableSchema> for Header {
    fn from(schema: TableSchema) -> Self {
        // TODO: optimize.
        Header::from(&schema)
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
    /// The type of the column. This will never contain any `AlgebraicTypeRef`s,
    /// that is, it will be resolved.
    pub col_type: AlgebraicType,
}

impl Schema for ColumnSchema {
    type Def = ColumnDef;
    type ParentId = ();
    // This is not like the other ID types: it's a tuple of the table ID and the column position.
    // A `ColId` alone does NOT suffice to identify a column!
    type Id = (TableId, ColId);

    fn from_module_def(
        module_def: &ModuleDef,
        def: &ColumnDef,
        _parent_id: (),
        (table_id, col_pos): (TableId, ColId),
    ) -> Self {
        let col_type = WithTypespace::new(module_def.typespace(), &def.ty)
            .resolve_refs()
            .expect("validated module should have all types resolve");
        ColumnSchema {
            table_id,
            col_pos,
            col_name: (*def.name).into(),
            col_type,
        }
    }

    fn check_compatible(&self, def: &Self::Def) -> Result<(), anyhow::Error> {
        ensure_eq!(&self.col_name[..], &def.name[..], "Column name mismatch");
        ensure_eq!(self.col_type, def.ty, "Column type mismatch");
        ensure_eq!(self.col_pos, def.col_id, "Columnh ID mismatch");
        Ok(())
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

impl Schema for SequenceSchema {
    type Def = SequenceDef;
    type Id = SequenceId;
    type ParentId = TableId;

    fn from_module_def(module_def: &ModuleDef, def: &Self::Def, parent_id: Self::ParentId, id: Self::Id) -> Self {
        module_def.expect_contains(def);

        SequenceSchema {
            sequence_id: id,
            sequence_name: (*def.name).into(),
            table_id: parent_id,
            col_pos: def.column,
            increment: def.increment,
            start: def.start.unwrap_or(1),
            min_value: def.min_value.unwrap_or(1),
            max_value: def.max_value.unwrap_or(i128::MAX),
            allocated: 0, // TODO: information not available in the `Def`s anymore, which is correct, but this may need to be overridden later.
        }
    }

    fn check_compatible(&self, def: &Self::Def) -> Result<(), anyhow::Error> {
        ensure_eq!(&self.sequence_name[..], &def.name[..], "Sequence name mismatch");
        ensure_eq!(self.col_pos, def.column, "Sequence column mismatch");
        ensure_eq!(self.increment, def.increment, "Sequence increment mismatch");
        if let Some(start) = &def.start {
            ensure_eq!(self.start, *start, "Sequence start mismatch");
        }
        if let Some(min_value) = &def.min_value {
            ensure_eq!(self.min_value, *min_value, "Sequence min_value mismatch");
        }
        if let Some(max_value) = &def.max_value {
            ensure_eq!(self.max_value, *max_value, "Sequence max_value mismatch");
        }
        Ok(())
    }
}

/// Marks a table as a timer table for a scheduled reducer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleSchema {
    /// The identifier of the table.
    pub table_id: TableId,

    /// The identifier of the schedule.
    pub schedule_id: ScheduleId,

    /// The name of the schedule.
    pub schedule_name: Box<str>,

    // The name of the reducer to call.
    pub reducer_name: Box<str>,
}

impl Schema for ScheduleSchema {
    type Def = ScheduleDef;

    type Id = ScheduleId;

    type ParentId = TableId;

    fn from_module_def(module_def: &ModuleDef, def: &Self::Def, parent_id: Self::ParentId, id: Self::Id) -> Self {
        module_def.expect_contains(def);

        ScheduleSchema {
            table_id: parent_id,
            schedule_id: id,
            schedule_name: (*def.name).into(),
            reducer_name: (*def.reducer_name).into(),
            // Ignore def.at_column and id_column. Those are recovered at runtime.
        }
    }

    fn check_compatible(&self, def: &Self::Def) -> Result<(), anyhow::Error> {
        ensure_eq!(&self.schedule_name[..], &def.name[..], "Schedule name mismatch");
        ensure_eq!(
            &self.reducer_name[..],
            &def.reducer_name[..],
            "Schedule reducer name mismatch"
        );
        Ok(())
    }
}

/// A struct representing the schema of a database index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSchema {
    /// The unique ID of the index within the schema.
    pub index_id: IndexId,
    /// The ID of the table associated with the index.
    pub table_id: TableId,
    /// The name of the index. This should not be assumed to follow any particular format.
    /// Unique within the database.
    pub index_name: Box<str>,
    /// The data for the schema.
    pub index_algorithm: IndexAlgorithm,
}

impl IndexSchema {}

impl Schema for IndexSchema {
    type Def = IndexDef;
    type Id = IndexId;
    type ParentId = TableId;

    fn from_module_def(module_def: &ModuleDef, def: &Self::Def, parent_id: Self::ParentId, id: Self::Id) -> Self {
        module_def.expect_contains(def);

        let index_algorithm = def.algorithm.clone();
        IndexSchema {
            index_id: id,
            table_id: parent_id,
            index_name: (*def.name).into(),
            index_algorithm,
        }
    }

    fn check_compatible(&self, def: &Self::Def) -> Result<(), anyhow::Error> {
        ensure_eq!(&self.index_name[..], &def.name[..], "Index name mismatch");
        ensure_eq!(&self.index_algorithm, &def.algorithm, "Index algorithm mismatch");
        Ok(())
    }
}

/// A struct representing the schema of a database constraint.
///
/// This struct holds information about a database constraint, including its unique identifier,
/// name, the table it belongs to, and the columns it is associated with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintSchema {
    /// The ID of the table the constraint applies to.
    pub table_id: TableId,
    /// The unique ID of the constraint within the database.
    pub constraint_id: ConstraintId,
    /// The name of the constraint.
    pub constraint_name: Box<str>,
    /// The data for the constraint.
    pub data: ConstraintData, // this reuses the type from Def, which is fine, neither of `schema` nor `def` are ABI modules.
}

impl ConstraintSchema {
    /// Constructs a `ConstraintSchema` from a given `ConstraintDef` and table identifier.
    ///
    /// # Parameters
    ///
    /// * `table_id`: Identifier of the table to which the constraint belongs.
    /// * `constraint`: The `ConstraintDef` containing constraint information.
    #[deprecated(note = "Use TableSchema::from_module_def instead")]
    pub fn from_def(table_id: TableId, constraint: RawConstraintDefV8) -> Option<Self> {
        if constraint.constraints.has_unique() {
            Some(ConstraintSchema {
                constraint_id: ConstraintId(0), // Set to 0 as it may be assigned later.
                constraint_name: constraint.constraint_name.trim().into(),
                table_id,
                data: ConstraintData::Unique(UniqueConstraintData {
                    columns: constraint.columns.into(),
                }),
            })
        } else {
            None
        }
    }
}

impl Schema for ConstraintSchema {
    type Def = ConstraintDef;
    type Id = ConstraintId;
    type ParentId = TableId;

    fn from_module_def(module_def: &ModuleDef, def: &Self::Def, parent_id: Self::ParentId, id: Self::Id) -> Self {
        module_def.expect_contains(def);

        ConstraintSchema {
            constraint_id: id,
            constraint_name: (*def.name).into(),
            table_id: parent_id,
            data: def.data.clone(),
        }
    }

    fn check_compatible(&self, def: &Self::Def) -> Result<(), anyhow::Error> {
        ensure_eq!(&self.constraint_name[..], &def.name[..], "Constraint name mismatch");
        ensure_eq!(&self.data, &def.data, "Constraint data mismatch");
        Ok(())
    }
}

#[cfg(test)]
#[allow(deprecated)]
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
                .map(|(pos, x)| RawConstraintDefV8::for_column("test", &x.col_name, Constraints::unset(), pos))
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
