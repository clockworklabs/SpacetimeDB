use std::collections::BTreeSet;
use std::marker::PhantomData;

use crate::{de::Deserialize, ser::Serialize};
use crate::{AlgebraicType, AlgebraicValue};
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_primitives::ColId;

pub use crate::db::def::IndexType;

pub mod identifier;
pub use identifier::Identifier;

pub mod builder;
pub use builder::DatabaseDefBuilder;

/// A reference to an entity in a `DatabaseDef`.
#[derive(Debug, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct Ref<T> {
    /// Sanity check. These are assigned randomly to each new DatabaseDef, and shared across all the refs to that DatabaseDef.
    sanity: u32,
    /// Identifier. These are assigned starting from 0.
    id: u32,
    _phantom: PhantomData<T>,
}
impl<T> Clone for Ref<T> {
    fn clone(&self) -> Self {
        Self {
            sanity: self.sanity,
            id: self.id,
            _phantom: PhantomData,
        }
    }
}
impl<T> Copy for Ref<T> {}
unsafe impl<T> Send for Ref<T> {}
unsafe impl<T> Sync for Ref<T> {}
impl<T> Ref<T> {
    fn new(sanity: u32, id: u32) -> Self {
        Self {
            sanity,
            id,
            _phantom: PhantomData,
        }
    }
}

/// A table definition.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct TableDef {
    /// The name of the table.
    pub name: Identifier,
    /// The fields of the table.
    pub fields: Vec<Ref<ColumnDef>>,
    _rest: (),
}

/// A column definition.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct ColumnDef {
    /// The table this column is attached to.
    pub table: Ref<TableDef>,
    /// The name of this column.
    /// This is the only identifier for the column that can be used across migrations.
    pub name: Identifier,
    /// The position of the column in the table.
    /// Not preserved by manual migrations.
    /// Fixed by the `canonical_ordering` algorithm, see that module for more information.
    pub position: ColId,
    /// The type of the column.
    pub type_: AlgebraicType,
    _rest: (),
}

/// A sequence definition.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize)] //, Deserialize)] // this needs fancy logic
#[sats(crate = crate)]
pub struct SequenceDef {
    /// The column this sequence is attached to.
    pub column: Ref<ColumnDef>,
    /// The starting value for the sequence.
    /// Must be valid at `start_type`.
    /// See **TODO** for incrementing logic of different types.
    pub start: AlgebraicValue,
    /// The type of the starting value.
    /// Must be equal to the type of `column` in the `Typespace` of the containing `DatabaseDef`.
    pub start_type: AlgebraicType,
    _rest: (),
}

/// A schedule definition. When attached to a table, marks that table as the timer table for a scheduled reducer.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct ScheduleDef {
    pub table: Ref<TableDef>,
    pub reducer: Identifier,
    _rest: (),
}

/// A unique constraint definition.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct UniqueConstraintDef {
    /// The table this is a constraint on.
    pub table: Ref<TableDef>,
    /// The set-theoretic projection of this table onto these columns must be canonically isomorphic to the original table.
    pub columns: Vec<Ref<ColumnDef>>,
    _rest: (),
}

/// An index definition.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct IndexDef {
    pub table: Ref<TableDef>,
    pub columns: Vec<Ref<ColumnDef>>,
    pub type_: IndexType,
    _rest: (),
}

/// The definition of a database schema.
/// Immutable after construction.
#[derive(Clone)]
pub struct DatabaseDef {
    // ---- Serialized Data ----
    /// Assigned randomly on initial creation; preserved by serialization; not preserved by migrations.
    sanity: u32,

    tables: Vec<TableDef>,
    columns: Vec<ColumnDef>,
    sequences: Vec<SequenceDef>,
    schedules: Vec<ScheduleDef>,
    unique_constraints: Vec<UniqueConstraintDef>,
    indexes: Vec<IndexDef>,

    // ---- Unserialized Data ----
    // These lookups are mainly used during migrations. Avoid usage at other times, as these are slow paths.
    tables_by_key: KeyLookup<TableDef>,
    columns_by_key: KeyLookup<ColumnDef>,
    sequences_by_key: KeyLookup<SequenceDef>,
    schedules_by_key: KeyLookup<ScheduleDef>,
    unique_constraints_by_key: KeyLookup<UniqueConstraintDef>,
    indexes_by_key: KeyLookup<IndexDef>,
}

type KeyLookup<T> = HashMap<<T as SchemaEntity>::Key, Ref<T>>;

impl<T: SchemaEntity> std::ops::Index<Ref<T>> for DatabaseDef {
    type Output = T;

    fn index(&self, index: Ref<T>) -> &Self::Output {
        assert_eq!(
            self.sanity, index.sanity,
            "Using a reference from a different schema is forbidden: db.sanity {:?} != index.sanity {:?}",
            self.sanity, index.sanity
        );
        &T::storage(self)[index.id as usize]
    }
}

impl DatabaseDef {
    pub fn lookup_by_key<'a, 'b, T: SchemaEntity>(&'a self, key: &'b T::Key) -> Option<(Ref<T>, &'a T)> {
        let ref_ = T::lookup(self, key)?;
        Some((ref_, &self[ref_]))
    }
}

/// An entity in the database schema.
pub trait SchemaEntity: Sized {
    /// The logical key for the entity, preserved across migrations.
    /// This type typically is expensive to allocate and should ONLY be used during migrations.
    type Key: Ord + Clone + 'static;

    /// Construct the key for an instance of the type.
    /// Should only be used during migrations.
    fn key(&self, schema: &DatabaseDef) -> Self::Key;

    /// Look up the key in the schema. This can be understood as a form of interning.
    fn lookup(schema: &DatabaseDef, key: &Self::Key) -> Option<Ref<Self>>;

    /// The underlying storage for this type.
    fn storage(schema: &DatabaseDef) -> &Vec<Self>;

    /// The underlying storage for this type, mutable.
    fn storage_mut(schema: &mut DatabaseDef) -> &mut Vec<Self>;
}

impl SchemaEntity for TableDef {
    // Table name.
    type Key = Identifier;

    fn key(&self, _schema: &DatabaseDef) -> Self::Key {
        self.name.clone()
    }

    fn lookup(schema: &DatabaseDef, key: &Self::Key) -> Option<Ref<Self>> {
        schema.tables_by_key.get(key).cloned()
    }

    fn storage(schema: &DatabaseDef) -> &Vec<Self> {
        &schema.tables
    }

    fn storage_mut(schema: &mut DatabaseDef) -> &mut Vec<Self> {
        &mut schema.tables
    }
}

impl SchemaEntity for ColumnDef {
    // Table name, column name.
    // TODO: should this be promoted to its own struct?
    type Key = (Identifier, Identifier);

    fn key(&self, schema: &DatabaseDef) -> Self::Key {
        (schema[self.table].name.clone(), self.name.clone())
    }

    fn lookup(schema: &DatabaseDef, key: &Self::Key) -> Option<Ref<Self>> {
        schema.columns_by_key.get(key).cloned()
    }

    fn storage(schema: &DatabaseDef) -> &Vec<Self> {
        &schema.columns
    }

    fn storage_mut(schema: &mut DatabaseDef) -> &mut Vec<Self> {
        &mut schema.columns
    }
}

impl SchemaEntity for SequenceDef {
    // Table name, column name.
    type Key = (Identifier, Identifier);

    fn key<'a>(&'a self, schema: &'a DatabaseDef) -> Self::Key {
        let column = &schema[self.column];
        (schema[column.table].name.clone(), column.name.clone())
    }

    fn lookup<'a, 'b>(schema: &'a DatabaseDef, key: &Self::Key) -> Option<Ref<Self>> {
        schema.sequences_by_key.get(key).cloned()
    }

    fn storage(schema: &DatabaseDef) -> &Vec<Self> {
        &schema.sequences
    }

    fn storage_mut(schema: &mut DatabaseDef) -> &mut Vec<Self> {
        &mut schema.sequences
    }
}

impl SchemaEntity for ScheduleDef {
    // Table name.
    type Key = Identifier;

    fn key(&self, schema: &DatabaseDef) -> Self::Key {
        schema[self.table].name.clone()
    }

    fn lookup(schema: &DatabaseDef, key: &Self::Key) -> Option<Ref<Self>> {
        schema.schedules_by_key.get(key).cloned()
    }

    fn storage(schema: &DatabaseDef) -> &Vec<Self> {
        &schema.schedules
    }

    fn storage_mut(schema: &mut DatabaseDef) -> &mut Vec<Self> {
        &mut schema.schedules
    }
}

impl SchemaEntity for UniqueConstraintDef {
    // Table name, column names.
    type Key = (Identifier, BTreeSet<Identifier>);

    fn key(&self, schema: &DatabaseDef) -> Self::Key {
        let table = &schema[self.table];
        (
            table.name.clone(),
            self.columns.iter().map(|&col| schema[col].name.clone()).collect(),
        )
    }

    fn lookup(schema: &DatabaseDef, key: &Self::Key) -> Option<Ref<Self>> {
        schema.unique_constraints_by_key.get(key).cloned()
    }

    fn storage(schema: &DatabaseDef) -> &Vec<Self> {
        &schema.unique_constraints
    }

    fn storage_mut(schema: &mut DatabaseDef) -> &mut Vec<Self> {
        &mut schema.unique_constraints
    }
}

impl SchemaEntity for IndexDef {
    // Table name, column names, type.
    type Key = (Identifier, BTreeSet<Identifier>, IndexType);

    fn key(&self, schema: &DatabaseDef) -> Self::Key {
        let table = &schema[self.table];
        (
            table.name.clone(),
            self.columns.iter().map(|&col| schema[col].name.clone()).collect(),
            self.type_,
        )
    }

    fn lookup(schema: &DatabaseDef, key: &Self::Key) -> Option<Ref<Self>> {
        schema.indexes_by_key.get(key).cloned()
    }

    fn storage(schema: &DatabaseDef) -> &Vec<Self> {
        &schema.indexes
    }

    fn storage_mut(schema: &mut DatabaseDef) -> &mut Vec<Self> {
        &mut schema.indexes
    }
}
