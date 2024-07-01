//! This module contains the "raw definitions" returned by modules to the database.

use crate::db::auth::{StAccess, StTableType};
use crate::{de, impl_deserialize, impl_serialize, ser, AlgebraicTypeRef, Typespace};
use crate::{AlgebraicType, ProductType};
use derive_more::Display;
use spacetimedb_primitives::*;

/// The default preallocation amount for sequences.
pub const SEQUENCE_PREALLOCATION_AMOUNT: i128 = 4_096;

/// A ref to use when initializing RawTableDef.
/// Should usually result in an error if it isn't fixed up.
const PROBABLY_UNALLOCATED_REF: AlgebraicTypeRef = AlgebraicTypeRef(u32::MAX);

/// Documents that a vector will be sorted during validation.
pub type UnsortedVec<T> = Vec<T>;

// deprecated, here for backwards-compatibility
impl_deserialize!([] Constraints, de => Self::try_from(de.deserialize_u8()?)
    .map_err(|_| de::Error::custom("invalid bitflags for `Constraints`"))
);
impl_serialize!([] Constraints, (self, ser) => ser.serialize_u8(self.bits()));

/// A possibly-invalid raw database definition.
/// These "raw definitions" may contain invalid data; the internal crate `spacetimedb_schema` allows validating them into an immutable `DatabaseDef`.
///
/// Many of the vectors in this type will be sorted during validation. This is documented with the `UnsortedVec` type.
/// Vectors that will not be sorted use the regular `Vec` type.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize, Default)]
#[sats(crate = crate)]
pub struct RawDatabaseDef {
    /// The tables of the database def.
    /// Need not be sorted.
    pub tables: UnsortedVec<RawTableDef>,

    /// The typespace of the database def.
    pub typespace: Typespace,
}

impl RawDatabaseDef {
    /// Create a new, empty [RawDatabaseDef] instance with no types in its typespace.
    pub fn new() -> Self {
        Default::default()
    }

    /// Add a table to the database definition.
    /// The table's `product_type_ref` must have been initialized correctly.
    pub fn add_table(&mut self, table: RawTableDef) {
        self.tables.push(table);
    }

    /// Add a table to the database definition.
    /// The table's `product_type_ref` must have been initialized correctly.
    pub fn with_table(mut self, table: RawTableDef) -> Self {
        self.add_table(table);
        self
    }

    /// Add a table to the database definition.
    /// Will automatically construct a product type for the table.
    /// This is only used in tests, since the runtime bindings system automatically generates product types
    /// and adds them to the DatabaseDef while initializing the RawTableDef.
    pub fn with_table_and_product_type(mut self, table: RawTableDef) -> Self {
        assert_eq!(
            table.product_type_ref, PROBABLY_UNALLOCATED_REF,
            "product_type_ref must be unallocated to use add_table_and_product_type"
        );
        let product_type = ProductType::new(
            table
                .columns
                .iter()
                .map(|x| crate::ProductTypeElement {
                    // TODO(jgilles): use Identifiers here.
                    // Really, though, this will require rewriting codegen to use Identifiers as well.
                    name: Some((&*x.col_name).into()),
                    algebraic_type: x.col_type.clone(),
                })
                .collect(),
        );
        let product_type_ref = self.typespace.add(product_type.clone().into());
        let mut table = table;
        table.product_type_ref = product_type_ref;
        self.with_table(table)
    }
}

/// Represents a sequence definition for a database table column.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[sats(crate = crate)]
pub struct RawSequenceDef {
    /// The name of the column associated with this sequence.
    pub column_name: Box<str>,
    pub start: Option<i128>,
    pub min_value: Option<i128>,
    pub max_value: Option<i128>,
}

impl RawSequenceDef {
    /// Creates a new [RawSequenceDef] instance for a specific table and column.
    ///
    /// # Arguments
    ///
    /// * `column` - The name of the column in the `table`.
    pub fn for_column(column: &str) -> Self {
        //removes the auto-generated suffix...

        RawSequenceDef {
            column_name: column.into(),
            start: None,
            min_value: None,
            max_value: None,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Display, Hash, de::Deserialize, ser::Serialize)]
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
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[sats(crate = crate)]
pub struct RawIndexDef {
    /// The type of the index.
    pub index_type: IndexType,
    /// List of column names that compose the index.
    /// ORDER MATTERS HERE.
    pub column_names: Vec<Box<str>>,
}

/// A struct representing the definition of a database column.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[sats(crate = crate)]
pub struct RawColumnDef {
    /// The name of the column.
    pub col_name: Box<str>,

    /// The type of the column.
    pub col_type: AlgebraicType,
}

impl RawColumnDef {
    /// Creates a new [RawColumnDef] for a field with the specified data type.
    ///
    /// This method is typically used to define columns with predefined names and data types for tests.
    ///
    /// # Arguments
    ///
    /// * `field_name`: The name for which to create a column definition.
    /// * `col_type`: The [AlgebraicType] of the column.
    ///
    pub fn new(field_name: &str, col_type: AlgebraicType) -> Self {
        Self {
            col_name: field_name.into(),
            col_type,
        }
    }
}

/// Requires that the projection of the table onto these columns is an bijection.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[sats(crate = crate)]
pub struct RawUniqueConstraintDef {
    pub column_names: UnsortedVec<Box<str>>,
}

/// A data structure representing the definition of a database table.
///
/// This struct holds information about the table, including its name, columns, indexes,
/// constraints, sequences, type, and access rights.
///
/// Validation rules:
/// - The table name must be a valid [crate::db::identifier::Identifier].
/// - The table's columns MUST be sorted according to [crate::db::ordering::canonical_ordering].
///   This is a sanity check to ensure that modules know the correct ordering to use for their tables.
/// - The table's indexes, constraints, and sequences need not be sorted; they will be sorted according to their respective ordering rules.
/// - The table's column types may refer only to types in the containing RawDatabaseDef's typespace.
/// - The table's column names must be unique.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[sats(crate = crate)]
pub struct RawTableDef {
    pub table_name: Box<str>,
    pub columns: Vec<RawColumnDef>,
    pub indexes: UnsortedVec<RawIndexDef>,
    pub unique_constraints: UnsortedVec<RawUniqueConstraintDef>,
    pub sequences: UnsortedVec<RawSequenceDef>,
    pub schedule: Option<RawScheduleDef>,
    pub table_type: StTableType,
    pub table_access: StAccess,
    /// The product type corresponding to a row of this table, stored in the DatabaseDef's typespace.
    /// May be set to a dummy value until the `RawTableDef` is actually added to a `RawDatabaseDef`.
    pub product_type_ref: AlgebraicTypeRef,
}

impl RawTableDef {
    /// Create a new `RawTableDef` instance with the specified `table_name` and `columns`.
    ///
    /// # Parameters
    ///
    /// - `table_name`: The name of the table.
    /// - `columns`: An Vec of `RawColumnDef` instances representing the columns of the table.
    ///     These must be sorted according to [crate::db::column_ordering::canonical_ordering].
    ///     If they are not, an error will be thrown at validation time.
    pub fn new(table_name: Box<str>, columns: Vec<RawColumnDef>) -> Self {
        Self {
            table_name,
            columns,
            indexes: vec![],
            unique_constraints: vec![],
            sequences: vec![],
            schedule: None,
            table_type: StTableType::User,
            table_access: StAccess::Public,
            // This isn't ideal, but should catch failed fixups.
            product_type_ref: PROBABLY_UNALLOCATED_REF,
        }
    }

    /// Set the type of the table and return a new `RawTableDef` instance with the updated type.
    pub fn with_type(self, table_type: StTableType) -> Self {
        let mut x = self;
        x.table_type = table_type;
        x
    }

    /// Set the access rights for the table and return a new `RawTableDef` instance with the updated access rights.
    pub fn with_access(self, table_access: StAccess) -> Self {
        let mut x = self;
        x.table_access = table_access;
        x
    }

    /// Generate a [UniqueConstraintDef] using the supplied `columns`.
    pub fn with_unique_constraint(self, column_names: &[&str]) -> Self {
        let mut x = self;
        let column_names = column_names.iter().map(|x| (*x).into()).collect();

        x.unique_constraints.push(RawUniqueConstraintDef { column_names });
        x
    }

    /// Generate a [RawIndexDef] using the supplied `columns`.
    pub fn with_index(self, column_names: &[&str], index_type: IndexType) -> Self {
        let mut x = self;
        let column_names = column_names.iter().map(|x| (*x).into()).collect();

        x.indexes.push(RawIndexDef {
            column_names,
            index_type,
        });
        x
    }

    /// Set the sequences for the table and return a new `RawTableDef` instance with the updated sequences.
    pub fn with_sequences(self, sequences: Vec<RawSequenceDef>) -> Self {
        let mut x = self;
        x.sequences = sequences;
        x
    }

    /// Generate a [RawSequenceDef] using the supplied `column`.
    pub fn with_column_sequence(self, column: &str) -> Self {
        let mut x = self;

        x.sequences.push(RawSequenceDef::for_column(column));
        x
    }

    /// Add a schedule definition to the table.
    pub fn with_schedule_def(self, schedule: RawScheduleDef) -> Self {
        let mut x = self;
        x.schedule = Some(schedule);
        x
    }

    /// Create a `RawTableDef` from a product type and table name. Used in tests and benchmarks.
    ///
    /// `row` MUST be sorted according to [crate::db::column_ordering::canonical_ordering].
    /// This is to ensure that benchmarks & tests are passing data with the correct ordering to the database.
    ///
    /// NOTE: If the [ProductType.name] is `None` then it auto-generate a name like `col_{col_pos}`
    pub fn from_product(table_name: &str, row: ProductType) -> Self {
        let columns = Vec::from(row.elements)
            .into_iter()
            .enumerate()
            .map(|(col_pos, e)| RawColumnDef {
                col_name: e.name.unwrap_or_else(|| format!("col_{col_pos}").into()),
                col_type: e.algebraic_type,
            })
            .collect::<Vec<_>>();

        Self::new(table_name.into(), columns)
    }

    pub fn get_column(&self, pos: usize) -> Option<&RawColumnDef> {
        self.columns.get(pos)
    }

    /// Check if the `col_name` exist on this [TableSchema]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_name(&self, col_name: &str) -> Option<&RawColumnDef> {
        self.columns.iter().find(|x| &*x.col_name == col_name)
    }
}

/// Marks a table as a timer table for a scheduled reducer.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[sats(crate = crate)]
pub struct RawScheduleDef {
    /// The name of the column that stores the desired invocation time.
    pub at_column: Box<str>,
    /// The name of the reducer to call.
    pub reducer_name: Box<str>,
}
