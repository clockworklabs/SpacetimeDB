//! Schema data structures.
//! These are used at runtime by the vm to store the schema of the database.
//! They are mirrored in the system tables -- see `spacetimedb_core::db::datastore::system_tables`.
//! Types in this file are not public ABI or API and may be changed at any time; it's the system tables that cannot.

// TODO(1.0): change all the `Box<str>`s in this file to `Identifier`.
// This doesn't affect the ABI so can wait until 1.0.

use core::mem;
use itertools::Itertools;
use spacetimedb_lib::db::auth::{StAccess, StTableType};
use spacetimedb_lib::db::error::{DefType, SchemaError};
use spacetimedb_lib::db::raw_def::v9::RawSql;
use spacetimedb_lib::db::raw_def::{generate_cols_name, RawConstraintDefV8};
use spacetimedb_lib::relation::{combine_constraints, Column, DbTable, FieldName, Header};
use spacetimedb_lib::{AlgebraicType, ProductType, ProductTypeElement};
use spacetimedb_primitives::*;
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::WithTypespace;
use std::collections::BTreeMap;
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
    ///
    /// If this schema entity contains children (e.g. if it is a table schema), they should be constructed with
    /// IDs set to `ChildId::SENTINEL`.
    ///
    /// If this schema entity contains `AlgebraicType`s, they should be fully resolved by this function (via
    /// `WithTypespace::resolve_refs`). This means they will no longer contain references to any typespace (and be non-recursive).
    /// This is necessary because the database does not currently attempt to handle typespaces / recursive types.
    fn from_module_def(module_def: &ModuleDef, def: &Self::Def, parent_id: Self::ParentId, id: Self::Id) -> Self;

    /// Check that a schema entity is compatible with a definition.
    fn check_compatible(&self, module_def: &ModuleDef, def: &Self::Def) -> Result<(), anyhow::Error>;
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
                table_id: TableId::SENTINEL,
                col_pos: ColId(col_pos as _),
                col_name: element.name.clone().unwrap_or_else(|| format!("col{}", col_pos).into()),
                col_type: element.algebraic_type.clone(),
            })
            .collect();

        TableSchema::new(
            TableId::SENTINEL,
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

    /// Update the table id of this schema.
    /// For use by the core database engine after assigning a table id.
    pub fn update_table_id(&mut self, id: TableId) {
        self.table_id = id;
        self.columns.iter_mut().for_each(|c| c.table_id = id);
        self.indexes.iter_mut().for_each(|i| i.table_id = id);
        self.constraints.iter_mut().for_each(|c| c.table_id = id);
        self.sequences.iter_mut().for_each(|s| s.table_id = id);
        if let Some(s) = self.schedule.as_mut() {
            s.table_id = id;
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

    /// Extracts all the [Self::indexes], [Self::sequences], and [Self::constraints].
    pub fn take_adjacent_schemas(&mut self) -> (Vec<IndexSchema>, Vec<SequenceSchema>, Vec<ConstraintSchema>) {
        (
            mem::take(&mut self.indexes),
            mem::take(&mut self.sequences),
            mem::take(&mut self.constraints),
        )
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
    pub fn remove_sequence(&mut self, sequence_id: SequenceId) -> Option<SequenceSchema> {
        find_remove(&mut self.sequences, |x| x.sequence_id == sequence_id)
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
    pub fn remove_index(&mut self, index_id: IndexId) -> Option<IndexSchema> {
        find_remove(&mut self.indexes, |x| x.index_id == index_id)
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
    pub fn remove_constraint(&mut self, constraint_id: ConstraintId) -> Option<ConstraintSchema> {
        find_remove(&mut self.constraints, |x| x.constraint_id == constraint_id)
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

    /// Is there a unique constraint for this set of columns?
    pub fn is_unique(&self, cols: &ColList) -> bool {
        self.constraints
            .iter()
            .filter_map(|cs| cs.data.unique_columns())
            .any(|unique_cols| **unique_cols == *cols)
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

    /// Iterate over the constraints on sets of columns on this table.
    fn backcompat_constraints_iter(&self) -> impl Iterator<Item = (ColList, Constraints)> + '_ {
        self.constraints
            .iter()
            .map(|x| -> (ColList, Constraints) {
                match &x.data {
                    ConstraintData::Unique(unique) => (unique.columns.clone().into(), Constraints::unique()),
                }
            })
            .chain(self.indexes.iter().map(|x| match &x.index_algorithm {
                IndexAlgorithm::BTree(btree) => (btree.columns.clone(), Constraints::indexed()),
                IndexAlgorithm::Direct(direct) => (direct.column.into(), Constraints::indexed()),
            }))
            .chain(
                self.sequences
                    .iter()
                    .map(|x| (col_list![x.col_pos], Constraints::auto_inc())),
            )
            .chain(
                self.primary_key
                    .iter()
                    .map(|x| (col_list![*x], Constraints::primary_key())),
            )
    }

    /// Get backwards-compatible constraints for this table.
    ///
    /// This is closer to how `TableSchema` used to work.
    pub fn backcompat_constraints(&self) -> BTreeMap<ColList, Constraints> {
        combine_constraints(self.backcompat_constraints_iter())
    }

    /// Get backwards-compatible constraints for this table.
    ///
    /// Resolves the constraints per each column. If the column don't have one, auto-generate [Constraints::unset()].
    /// This guarantee all columns can be queried for it constraints.
    pub fn backcompat_column_constraints(&self) -> BTreeMap<ColList, Constraints> {
        let mut result = self.backcompat_constraints();
        for col in &self.columns {
            result.entry(col_list![col.col_pos]).or_insert(Constraints::unset());
        }
        result
    }

    /// Get the column corresponding to the primary key, if any.
    pub fn pk(&self) -> Option<&ColumnSchema> {
        self.primary_key.and_then(|pk| self.get_column(pk.0 as usize))
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
            .chain(self.indexes.iter().map(|x| {
                let cols = match &x.index_algorithm {
                    IndexAlgorithm::BTree(btree) => btree.columns.clone(),
                    IndexAlgorithm::Direct(direct) => direct.column.into(),
                };
                (DefType::Index, x.index_name.clone(), cols)
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

/// Removes and returns the first element satisfying `predicate` in `vec`.
fn find_remove<T>(vec: &mut Vec<T>, predicate: impl Fn(&T) -> bool) -> Option<T> {
    let pos = vec.iter().position(predicate)?;
    Some(vec.remove(pos))
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
            .map(|def| IndexSchema::from_module_def(module_def, def, table_id, IndexId::SENTINEL))
            .collect();

        let sequences = sequences
            .values()
            .map(|def| SequenceSchema::from_module_def(module_def, def, table_id, SequenceId::SENTINEL))
            .collect();

        let constraints = constraints
            .values()
            .map(|def| ConstraintSchema::from_module_def(module_def, def, table_id, ConstraintId::SENTINEL))
            .collect();

        let schedule = schedule
            .as_ref()
            .map(|schedule| ScheduleSchema::from_module_def(module_def, schedule, table_id, ScheduleId::SENTINEL));

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
            *primary_key,
        )
    }

    fn check_compatible(&self, module_def: &ModuleDef, def: &Self::Def) -> Result<(), anyhow::Error> {
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
            col.check_compatible(module_def, col_def)?;
        }
        ensure_eq!(self.columns.len(), def.columns.len(), "Column count mismatch");

        for index in &self.indexes {
            let index_def = def
                .indexes
                .get(&index.index_name[..])
                .ok_or_else(|| anyhow::anyhow!("Index {} not found in definition", index.index_id.0))?;
            index.check_compatible(module_def, index_def)?;
        }
        ensure_eq!(self.indexes.len(), def.indexes.len(), "Index count mismatch");

        for constraint in &self.constraints {
            let constraint_def = def
                .constraints
                .get(&constraint.constraint_name[..])
                .ok_or_else(|| anyhow::anyhow!("Constraint {} not found in definition", constraint.constraint_id.0))?;
            constraint.check_compatible(module_def, constraint_def)?;
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
            sequence.check_compatible(module_def, sequence_def)?;
        }
        ensure_eq!(self.sequences.len(), def.sequences.len(), "Sequence count mismatch");

        if let Some(schedule) = &self.schedule {
            let schedule_def = def
                .schedule
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Schedule not found in definition"))?;
            schedule.check_compatible(module_def, schedule_def)?;
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

        Header::new(
            value.table_id,
            value.table_name.clone(),
            fields,
            value.backcompat_constraints(),
        )
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

impl ColumnSchema {
    pub fn for_test(pos: impl Into<ColId>, name: impl Into<Box<str>>, ty: AlgebraicType) -> Self {
        Self {
            table_id: TableId::SENTINEL,
            col_pos: pos.into(),
            col_name: name.into(),
            col_type: ty,
        }
    }
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

    fn check_compatible(&self, module_def: &ModuleDef, def: &Self::Def) -> Result<(), anyhow::Error> {
        ensure_eq!(&self.col_name[..], &def.name[..], "Column name mismatch");
        let resolved_def_ty = WithTypespace::new(module_def.typespace(), &def.ty).resolve_refs()?;
        ensure_eq!(self.col_type, resolved_def_ty, "Column type mismatch");
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

    fn check_compatible(&self, _module_def: &ModuleDef, def: &Self::Def) -> Result<(), anyhow::Error> {
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

    /// The name of the reducer to call.
    pub reducer_name: Box<str>,

    /// The column containing the `ScheduleAt` enum.
    pub at_column: ColId,
}

impl ScheduleSchema {
    pub fn for_test(name: impl Into<Box<str>>, reducer: impl Into<Box<str>>, at: impl Into<ColId>) -> Self {
        Self {
            table_id: TableId::SENTINEL,
            schedule_id: ScheduleId::SENTINEL,
            schedule_name: name.into(),
            reducer_name: reducer.into(),
            at_column: at.into(),
        }
    }
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
            at_column: def.at_column,
            // Ignore def.at_column and id_column. Those are recovered at runtime.
        }
    }

    fn check_compatible(&self, _module_def: &ModuleDef, def: &Self::Def) -> Result<(), anyhow::Error> {
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

impl IndexSchema {
    pub fn for_test(name: impl Into<Box<str>>, algo: impl Into<IndexAlgorithm>) -> Self {
        Self {
            index_id: IndexId::SENTINEL,
            table_id: TableId::SENTINEL,
            index_name: name.into(),
            index_algorithm: algo.into(),
        }
    }
}

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

    fn check_compatible(&self, _module_def: &ModuleDef, def: &Self::Def) -> Result<(), anyhow::Error> {
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
    pub fn unique_for_test(name: impl Into<Box<str>>, cols: impl Into<ColSet>) -> Self {
        Self {
            table_id: TableId::SENTINEL,
            constraint_id: ConstraintId::SENTINEL,
            constraint_name: name.into(),
            data: ConstraintData::Unique(UniqueConstraintData { columns: cols.into() }),
        }
    }

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
                constraint_id: ConstraintId::SENTINEL, // Set to 0 as it may be assigned later.
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

    fn check_compatible(&self, _module_def: &ModuleDef, def: &Self::Def) -> Result<(), anyhow::Error> {
        ensure_eq!(&self.constraint_name[..], &def.name[..], "Constraint name mismatch");
        ensure_eq!(&self.data, &def.data, "Constraint data mismatch");
        Ok(())
    }
}

/// A struct representing the schema of a row-level security policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowLevelSecuritySchema {
    pub table_id: TableId,
    pub sql: RawSql,
}
