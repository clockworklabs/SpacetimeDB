//! This module contains the "raw definitions" returned by modules to the database.

use crate::db::auth::{StAccess, StTableType};
use derive_more::Display;
use itertools::Itertools;
use spacetimedb_primitives::*;
use spacetimedb_sats::{de, ser, AlgebraicTypeRef, Typespace};
use spacetimedb_sats::{AlgebraicType, ProductType, ProductTypeElement};

/// The default preallocation amount for sequences.
pub const SEQUENCE_PREALLOCATION_AMOUNT: i128 = 4_096;

/// A not-yet-validated identifier.
pub type RawIdentifier = Box<str>;

/// A possibly-invalid raw database definition, version 1.
///
/// These "raw definitions" may contain invalid data; the internal crate `spacetimedb_schema` allows validating them into an immutable `DatabaseDef`.
///
/// Many of the vectors in this type will be sorted during validation. This is documented with the `UnsortedVec` type.
/// Vectors that will not be sorted use the regular `Vec` type.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize, Default)]
pub struct RawDatabaseDefV1 {
    /// The tables of the database def.
    /// Need not be sorted.
    pub tables: Vec<RawTableDef>,

    /// The typespace of the database def.
    pub typespace: Typespace,
}

impl RawDatabaseDefV1 {
    /// Creates a new, empty [RawDatabaseDef] instance with no types in its typespace.
    pub fn new() -> Self {
        Default::default()
    }

    /// Adds a table to the database definition.
    /// The table's `product_type_ref` must have been initialized correctly.
    pub fn add_table(&mut self, table: RawTableDef) {
        // The product type will be checked during validation.
        self.tables.push(table);
    }

    /// Adds a table to the database definition.
    /// The table's `product_type_ref` must have been initialized correctly.
    pub fn with_table(mut self, table: RawTableDef) -> Self {
        // The product type will be checked during validation.
        self.add_table(table);
        self
    }

    /// Adds a table to the database definition.
    /// Will automatically construct a product type for the table.
    /// This is only used in tests, since the runtime bindings system automatically generates product types
    /// and adds them to the `DatabaseDef` while initializing the `RawTableDef`.
    #[cfg(feature = "test")]
    pub fn with_table_and_product_type(mut self, table: RawTableDef) -> Self {
        assert_eq!(
            table.product_type_ref,
            AlgebraicTypeRef(PROBABLY_UNALLOCATED_ID),
            "product_type_ref must be unallocated to use add_table_and_product_type"
        );
        let product_type = ProductType::new(
            table
                .columns
                .iter()
                .map(|x| ProductTypeElement {
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

/// A sequence definition for a database table column.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
pub struct RawSequenceDef {
    /// The name of the column associated with this sequence.
    /// Must have integral type.
    /// This uniquely identifies the sequence definition.
    pub column_name: RawIdentifier,
    /// The value to start assigning to this column.
    /// Will be incremented by 1 for each new row.
    pub start: Option<i128>,
    /// The minimum allowed value in this column.
    pub min_value: Option<i128>,
    /// The maximum allowed value in this column.
    pub max_value: Option<i128>,
}

impl RawSequenceDef {
    /// Creates a new [RawSequenceDef] instance for a specific table and column.
    ///
    /// # Parameters
    ///
    /// * `column` - The name of the column in the `table`.
    pub fn for_column(column: &str) -> Self {
        RawSequenceDef {
            column_name: column.into(),
            start: None,
            min_value: None,
            max_value: None,
        }
    }
}

/// An index used within a database table.
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Display, Hash, de::Deserialize, ser::Serialize)]
#[non_exhaustive]
pub enum IndexType {
    /// A BTree-based index, currently implemented with a Rust `std::collections::BTreeMap`.
    BTree = 0,
    /// A hypothetical hash-based index.
    /// Currently it is forbidden to use this.
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

/// The definition of a database index.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
pub struct RawIndexDef {
    /// The name of the index.
    ///
    /// This can be overridden by the user and should NOT be assumed to follow
    /// any particular format.
    ///
    /// Unique within a table.
    pub name: RawIdentifier,

    /// The algorithm parameters for the index.
    pub algorithm: RawIndexAlgorithm,
}

/// Data specifying an index algorithm.
#[non_exhaustive]
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
pub enum RawIndexAlgorithm {
    /// Implemented using a rust `std::collections::BTreeMap`.
    BTree {
        /// The columns to index on. These are ordered.
        columns: ColList,
    },
    /// Currently forbidden.
    Hash {
        /// The columns to index on. These are ordered.
        columns: ColList,
    },
}

/// The definition of a database column.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
pub struct RawColumnDef {
    /// The name of the column.
    pub col_name: RawIdentifier,

    /// The type of the column.
    pub col_type: AlgebraicType,
}

impl RawColumnDef {
    /// Creates a new [RawColumnDef] for a field with the specified data type.
    ///
    /// This method is typically used to define columns with predefined names and data types for tests.
    ///
    /// # Parameters
    ///
    /// * `col_name`: The name for which to create a column definition.
    /// * `col_type`: The [AlgebraicType] of the column.
    ///
    pub fn new(col_name: RawIdentifier, col_type: AlgebraicType) -> Self {
        Self { col_name, col_type }
    }
}

/// Requires that the projection of the table onto these columns is an bijection.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
pub struct RawUniqueConstraintDef {
    /// The columns that must be unique.
    pub columns: ColList,
}

/// The definition of a database table.
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
pub struct RawTableDef {
    /// The name of the table.
    /// Unique within a module, acts as the table's identifier.
    /// Must be a valid [crate::db::identifier::Identifier].
    pub table_name: RawIdentifier,
    /// The columns of the table.
    /// Must be ordered according to [crate::db::column_ordering::canonical_ordering].
    pub columns: Vec<RawColumnDef>,
    /// The indices of the table.
    pub indexes: Vec<RawIndexDef>,
    /// Any unique constraints on the table.
    pub unique_constraints: Vec<RawUniqueConstraintDef>,
    /// The sequences for the table.
    pub sequences: Vec<RawSequenceDef>,
    /// The schedule for the table.
    pub schedule: Option<RawScheduleDef>,
    /// Whether this is a system- or user-created table.
    pub table_type: StTableType,
    /// Whether this table is public or private.
    pub table_access: StAccess,
    /// The product type corresponding to a row of this table, stored in the DatabaseDef's typespace.
    /// May be set to a dummy value until the `RawTableDef` is actually added to a `RawDatabaseDef`.
    pub product_type_ref: AlgebraicTypeRef,
}

impl RawTableDef {
    /// Creates a new `RawTableDef` instance with the specified `table_name` and `columns`.
    ///
    /// # Parameters
    ///
    /// - `table_name`: The name of the table.
    /// - `columns`: A list of `RawColumnDef`s for the columns of the table.
    ///     These must be sorted according to [crate::db::column_ordering::canonical_ordering].
    ///     If they are not, an error will be thrown at validation time.
    pub fn new(table_name: RawIdentifier, columns: Vec<RawColumnDef>) -> Self {
        Self {
            table_name,
            columns,
            indexes: vec![],
            unique_constraints: vec![],
            sequences: vec![],
            schedule: None,
            table_type: StTableType::User,
            // TODO: make the default `Private` before 1.0.
            table_access: StAccess::Public,
            // This isn't ideal, but should catch failed fixups.
            product_type_ref: AlgebraicTypeRef(PROBABLY_UNALLOCATED_ID),
        }
    }

    /// Sets the type of the table and return it.
    pub fn with_type(mut self, table_type: StTableType) -> Self {
        self.table_type = table_type;
        self
    }

    /// Sets the access rights for the table and return it.
    pub fn with_access(mut self, table_access: StAccess) -> Self {
        self.table_access = table_access;
        self
    }

    /// Generates a [UniqueConstraintDef] using the supplied `columns`.
    pub fn with_unique_constraint(mut self, columns: ColList) -> Self {
        self.unique_constraints.push(RawUniqueConstraintDef { columns });
        self
    }

    /// Get the column ID for a column name.
    pub fn col_id(&self, column_name: &str) -> Option<ColId> {
        self.columns
            .iter()
            .position(|x| &*x.col_name == column_name)
            .map(|x| ColId(x as u32))
    }

    /// Get the column IDs for a list of column names.
    pub fn col_list(&self, column_names: &[&str]) -> Option<ColList> {
        column_names
            .iter()
            .map(|x| self.col_id(x))
            .collect::<Option<ColListBuilder>>()
            .and_then(|b| b.build().ok())
    }

    /// YOU CANNOT RELY ON INDEXES HAVING THIS NAME FORMAT.
    fn generate_index_name(&self, algorithm: &RawIndexAlgorithm) -> RawIdentifier {
        let (label, columns) = match algorithm {
            RawIndexAlgorithm::BTree { columns } => ("btree", columns),
            RawIndexAlgorithm::Hash { columns } => ("hash", columns),
        };
        let column_names = columns.iter().map(|x| &self.columns[x.0 as usize].col_name).join("_");
        let table_name = &self.table_name;
        format!("{table_name}_{label}_{column_names}").into()
    }

    /// Generates a [RawIndexDef] using the supplied `columns`.
    pub fn with_index(mut self, algorithm: RawIndexAlgorithm, name: Option<RawIdentifier>) -> Self {
        let name = name.unwrap_or_else(|| self.generate_index_name(&algorithm));

        self.indexes.push(RawIndexDef { name, algorithm });
        self
    }

    /// Sets the sequences for the table and return it.
    /// Drops any existing sequences.
    pub fn with_sequences(mut self, sequences: Vec<RawSequenceDef>) -> Self {
        self.sequences = sequences;
        self
    }

    /// Adds a [RawSequenceDef] on the supplied `column`.
    pub fn with_column_sequence(mut self, column: &str) -> Self {
        self.sequences.push(RawSequenceDef::for_column(column));
        self
    }

    /// Adds a schedule definition to the table.
    pub fn with_schedule_def(mut self, schedule: RawScheduleDef) -> Self {
        self.schedule = Some(schedule);
        self
    }

    /// Creates a `RawTableDef` from a product type and table name. Used in tests and benchmarks.
    ///
    /// `row` MUST be sorted according to [crate::db::column_ordering::canonical_ordering].
    /// This is to ensure that benchmarks & tests are passing data with the correct ordering to the database.
    ///
    /// NOTE: If the [ProductType.name] is `None` then it auto-generate a name like `col_{col_pos}`
    pub fn from_product_for_tests(table_name: &str, row: ProductType) -> Self {
        let columns = Vec::from(row.elements)
            .into_iter()
            .enumerate()
            .map(|(col_pos, e)| RawColumnDef {
                // If a field is unnamed, use e.g., `col_4`.
                col_name: e.name.unwrap_or_else(|| format!("col_{col_pos}").into()),
                col_type: e.algebraic_type,
            })
            .collect();

        Self::new(table_name.into(), columns)
    }

    /// Gets the column at the specified position, if present.
    pub fn get_column(&self, pos: usize) -> Option<&RawColumnDef> {
        self.columns.get(pos)
    }

    /// Checks if the `col_name` exist on this [TableSchema]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_name(&self, col_name: &str) -> Option<&RawColumnDef> {
        self.columns.iter().find(|x| &*x.col_name == col_name)
    }
}

/// Marks a table as a timer table for a scheduled reducer.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
pub struct RawScheduleDef {
    /// The name of the column that stores the desired invocation time.
    pub at_column: RawIdentifier,
    /// The name of the reducer to call.
    pub reducer_name: RawIdentifier,
}
