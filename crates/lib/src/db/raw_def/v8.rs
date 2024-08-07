//! Database definitions v8, the last version before they were wrapped in an enum.
//!
//! Nothing to do with Chrome.

use crate::db::auth::{StAccess, StTableType};
use crate::relation::FieldName;
use crate::{AlgebraicType, ProductType};
use derive_more::Display;
use spacetimedb_data_structures::map::HashSet;
use spacetimedb_primitives::*;
use spacetimedb_sats::{de, ser};

/// The amount sequences allocate each time they over-run their allocation.
///
/// Note that we do not perform an initial allocation during `create_sequence` or at startup.
/// Newly-created sequences will allocate the first time they are advanced.
pub const SEQUENCE_ALLOCATION_STEP: i128 = 4096;

/// Represents a sequence definition for a database table column.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, ser::Serialize, de::Deserialize)]
pub struct RawSequenceDefV8 {
    /// The name of the sequence.
    pub sequence_name: Box<str>,
    /// The position of the column associated with this sequence.
    pub col_pos: ColId,
    /// The increment value for the sequence.
    pub increment: i128,
    /// The starting value for the sequence.
    pub start: Option<i128>,
    /// The minimum value for the sequence.
    pub min_value: Option<i128>,
    /// The maximum value for the sequence.
    pub max_value: Option<i128>,
    /// The number of values to preallocate for the sequence.
    /// Deprecated, in the future this concept will no longer exist.
    pub allocated: i128,
}

impl RawSequenceDefV8 {
    /// Creates a new [RawSequenceDefV8] instance for a specific table and column.
    ///
    /// # Parameters
    ///
    /// * `table` - The name of the table.
    /// * `seq_name` - The name of the sequence.
    /// * `col_pos` - The position of the column in the `table`.
    ///
    /// # Example
    ///
    /// ```
    /// use spacetimedb_lib::db::raw_def::*;
    ///
    /// let sequence_def = RawSequenceDefV8::for_column("my_table", "my_sequence", 1.into());
    /// assert_eq!(&*sequence_def.sequence_name, "seq_my_table_my_sequence");
    /// ```
    pub fn for_column(table: &str, column_or_name: &str, col_pos: ColId) -> Self {
        //removes the auto-generated suffix...
        let seq_name = column_or_name.trim_start_matches(&format!("ct_{}_", table));

        RawSequenceDefV8 {
            sequence_name: format!("seq_{}_{}", table, seq_name).into(),
            col_pos,
            increment: 1,
            start: None,
            min_value: None,
            max_value: None,
            // Start with no values allocated. The first time we advance the sequence,
            // we will allocate [`SEQUENCE_ALLOCATION_STEP`] values.
            allocated: 0,
        }
    }
}

/// Which type of index to create.
///
/// Currently only `IndexType::BTree` is allowed.
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Display, de::Deserialize, ser::Serialize)]
pub enum IndexType {
    /// A BTree index.
    BTree = 0,
    /// A Hash index.
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
pub struct RawIndexDefV8 {
    /// The name of the index.
    /// This should not be assumed to follow any particular format.
    pub index_name: Box<str>,
    /// Whether the index is unique.
    pub is_unique: bool,
    /// The type of the index.
    pub index_type: IndexType,
    /// List of column positions that compose the index.
    pub columns: ColList,
}

impl RawIndexDefV8 {
    /// Creates a new [IndexDef] with the provided parameters.
    ///
    /// WARNING: Only [IndexType::Btree] is supported for now...
    ///
    /// # Parameters
    ///
    /// * `index_name`: The name of the index.
    /// * `columns`: List of column positions that compose the index.
    /// * `is_unique`: Indicates whether the index enforces uniqueness.
    pub fn btree(index_name: Box<str>, columns: impl Into<ColList>, is_unique: bool) -> Self {
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
    /// use spacetimedb_lib::db::raw_def::*;
    ///
    /// let index_def = RawIndexDefV8::for_column("my_table", "test", ColList::new(1u32.into()), true);
    /// assert_eq!(&*index_def.index_name, "idx_my_table_test_unique");
    /// ```
    pub fn for_column(table: &str, index_or_name: &str, columns: impl Into<ColList>, is_unique: bool) -> Self {
        let unique = if is_unique { "unique" } else { "non_unique" };

        // Removes the auto-generated suffix from the index name.
        let name = index_or_name.trim_start_matches(&format!("ct_{}_", table));

        // Constructs the index name using a predefined format.
        // No duplicate the `kind_name` that was added by an constraint
        let name = if name.ends_with(&unique) {
            format!("idx_{table}_{name}")
        } else {
            format!("idx_{table}_{name}_{unique}")
        };
        Self::btree(name.into(), columns, is_unique)
    }
}

/// A struct representing the definition of a database column.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, ser::Serialize, de::Deserialize)]
pub struct RawColumnDefV8 {
    /// The name of the column.
    pub col_name: Box<str>,
    /// The type of the column.
    ///
    /// Must be in "nominal normal form", as determined by [AlgebraicType::is_nominal_normal_form].
    pub col_type: AlgebraicType,
}

impl RawColumnDefV8 {
    /// Convert a product type to a list of column definitions.
    pub fn from_product_type(value: ProductType) -> Vec<RawColumnDefV8> {
        Vec::from(value.elements)
            .into_iter()
            .enumerate()
            .map(|(pos, col)| {
                let col_name = if let Some(name) = col.name {
                    name
                } else {
                    format!("col_{pos}").into()
                };

                RawColumnDefV8 {
                    col_name,
                    col_type: col.algebraic_type,
                }
            })
            .collect()
    }
}

impl RawColumnDefV8 {
    /// Creates a new [ColumnDef] for a system field with the specified data type.
    ///
    /// This method is typically used to define system columns with predefined names and data types.
    ///
    /// # Parameters
    ///
    /// * `field_name`: The name for which to create a column definition.
    /// * `col_type`: The [AlgebraicType] of the column.
    ///
    /// If `type_` is not `AlgebraicType::Builtin` or `AlgebraicType::Ref`, an error will result at validation time.
    pub fn sys(field_name: &str, col_type: AlgebraicType) -> Self {
        Self {
            col_name: field_name.into(),
            col_type,
        }
    }
}

/// A struct representing the definition of a database constraint.
/// Associated with a unique `TableDef`, the one that contains it.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, ser::Serialize, de::Deserialize)]
pub struct RawConstraintDefV8 {
    /// The name of the constraint.
    pub constraint_name: Box<str>,
    /// The constraints applied to the columns.
    pub constraints: Constraints,
    /// List of column positions associated with the constraint.
    pub columns: ColList,
}

impl RawConstraintDefV8 {
    /// Creates a new [ConstraintDef] with the specified parameters.
    ///
    /// # Parameters
    ///
    /// * `constraint_name`: The name of the constraint.
    /// * `constraints`: The constraints.
    /// * `columns`: List of column positions associated with the constraint.
    pub fn new(constraint_name: Box<str>, constraints: Constraints, columns: impl Into<ColList>) -> Self {
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
    /// # Parameters
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
    /// use spacetimedb_lib::db::raw_def::*;
    ///
    /// let constraint_def = RawConstraintDefV8::for_column("my_table", "test", Constraints::identity(), ColList::new(1u32.into()));
    /// assert_eq!(&*constraint_def.constraint_name, "ct_my_table_test_identity");
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
            Self::new(format!("ct_{table}_{name}").into(), constraints, columns)
        } else {
            Self::new(format!("ct_{table}_{name}_{kind_name}").into(), constraints, columns)
        }
    }
}

/// Concatenate the column names from the `columns`
///
/// WARNING: If the `ColId` not exist, is skipped.
/// TODO(Tyler): This should return an error and not allow this to be constructed
/// if there is an invalid `ColId`
pub fn generate_cols_name<'a>(columns: &ColList, col_name: impl Fn(ColId) -> Option<&'a str>) -> String {
    let mut column_name = Vec::with_capacity(columns.len() as usize);
    column_name.extend(columns.iter().filter_map(col_name));
    column_name.join("_")
}

/// A data structure representing the definition of a database table.
///
/// This struct holds information about the table, including its name, columns, indexes,
/// constraints, sequences, type, and access rights.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, ser::Serialize, de::Deserialize)]
pub struct RawTableDefV8 {
    /// The name of the table.
    pub table_name: Box<str>,
    /// The columns of the table.
    /// The ordering of the columns is significant. Columns are frequently identified by `ColId`, that is, position in this list.
    pub columns: Vec<RawColumnDefV8>,
    /// The indexes on the table.
    pub indexes: Vec<RawIndexDefV8>,
    /// The constraints on the table.
    pub constraints: Vec<RawConstraintDefV8>,
    /// The sequences attached to the table.
    pub sequences: Vec<RawSequenceDefV8>,
    /// Whether the table was created by a user or by the system.
    pub table_type: StTableType,
    /// The visibility of the table.
    pub table_access: StAccess,
    /// If this is a schedule table, the reducer it is scheduled for.
    pub scheduled: Option<Box<str>>,
}

impl RawTableDefV8 {
    /// Create a new `TableDef` instance with the specified `table_name` and `columns`.
    ///
    /// # Parameters
    ///
    /// - `table_name`: The name of the table.
    /// - `columns`: A `vec` of `ColumnDef` instances representing the columns of the table.
    ///
    pub fn new(table_name: Box<str>, columns: Vec<RawColumnDefV8>) -> Self {
        Self {
            table_name,
            columns,
            indexes: vec![],
            constraints: vec![],
            sequences: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
            scheduled: None,
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
    pub fn with_constraints(self, constraints: Vec<RawConstraintDefV8>) -> Self {
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
        generate_cols_name(columns, |p| self.get_column(p.idx()).map(|c| &*c.col_name))
    }

    /// Generate a [ConstraintDef] using the supplied `columns`.
    pub fn with_column_constraint(mut self, kind: Constraints, columns: impl Into<ColList>) -> Self {
        self.constraints.push(self.gen_constraint_def(kind, columns));
        self
    }

    fn gen_constraint_def(&self, kind: Constraints, columns: impl Into<ColList>) -> RawConstraintDefV8 {
        let columns = columns.into();
        RawConstraintDefV8::for_column(&self.table_name, &self.generate_cols_name(&columns), kind, columns)
    }

    /// Set the indexes for the table and return a new `TableDef` instance with the updated indexes.
    pub fn with_indexes(self, indexes: Vec<RawIndexDefV8>) -> Self {
        let mut x = self;
        x.indexes = indexes;
        x
    }

    /// Generate a [IndexDef] using the supplied `columns`.
    pub fn with_column_index(self, columns: impl Into<ColList>, is_unique: bool) -> Self {
        let mut x = self;
        let columns = columns.into();
        x.indexes.push(RawIndexDefV8::for_column(
            &x.table_name,
            &x.generate_cols_name(&columns),
            columns,
            is_unique,
        ));
        x
    }

    /// Set the sequences for the table and return a new `TableDef` instance with the updated sequences.
    pub fn with_sequences(self, sequences: Vec<RawSequenceDefV8>) -> Self {
        let mut x = self;
        x.sequences = sequences;
        x
    }

    /// Generate a [RawSequenceDefV8] using the supplied `columns`.
    pub fn with_column_sequence(self, columns: ColId) -> Self {
        let mut x = self;

        x.sequences.push(RawSequenceDefV8::for_column(
            &x.table_name,
            &x.generate_cols_name(&ColList::new(columns)),
            columns,
        ));
        x
    }

    /// Set the reducer name for scheduled tables and return updated `TableDef`.
    pub fn with_scheduled(mut self, scheduled: Option<Box<str>>) -> Self {
        self.scheduled = scheduled;
        self
    }

    /// Create a `TableDef` from a product type and table name.
    ///
    /// NOTE: If the [ProductType.name] is `None` then it auto-generate a name like `col_{col_pos}`
    pub fn from_product(table_name: &str, row: ProductType) -> Self {
        Self::new(
            table_name.into(),
            Vec::from(row.elements)
                .into_iter()
                .enumerate()
                .map(|(col_pos, e)| RawColumnDefV8 {
                    col_name: e.name.unwrap_or_else(|| format!("col_{col_pos}").into()),
                    col_type: e.algebraic_type,
                })
                .collect::<Vec<_>>(),
        )
    }

    /// Get an iterator deriving [IndexDef] from the constraints that require them like `UNIQUE`.
    ///
    /// It looks into [Self::constraints] for possible duplicates and remove them from the result
    pub fn generated_indexes(&self) -> impl Iterator<Item = RawIndexDefV8> + '_ {
        self.constraints
            .iter()
            // We are only interested in constraints implying an index.
            .filter(|x| x.constraints.has_indexed())
            // Create the `IndexDef`.
            .map(|x| {
                let is_unique = x.constraints.has_unique();
                RawIndexDefV8::for_column(&self.table_name, &x.constraint_name, x.columns.clone(), is_unique)
            })
            // Only keep those we don't yet have in the list of indices (checked by name).
            .filter(|idx| self.indexes.iter().all(|x| x.index_name != idx.index_name))
    }

    /// Get an iterator deriving [RawSequenceDefV8] from the constraints that require them like `IDENTITY`.
    ///
    /// It looks into [Self::constraints] for possible duplicates and remove them from the result
    pub fn generated_sequences(&self) -> impl Iterator<Item = RawSequenceDefV8> + '_ {
        self.constraints
            .iter()
            // We are only interested in constraints implying a sequence.
            .filter(|x| x.constraints.has_autoinc())
            // Create the `SequenceDef`.
            .map(|x| RawSequenceDefV8::for_column(&self.table_name, &x.constraint_name, x.columns.head()))
            // Only keep those we don't yet have in the list of sequences (checked by name).
            .filter(|seq| self.sequences.iter().all(|x| x.sequence_name != seq.sequence_name))
    }

    /// Get an iterator deriving [ConstraintDef] from the indexes that require them like `UNIQUE`.
    ///
    /// It looks into [Self::constraints] for possible duplicates and remove them from the result
    pub fn generated_constraints(&self) -> impl Iterator<Item = RawConstraintDefV8> + '_ {
        // Collect the set of all col-lists with a constraint.
        let cols: HashSet<_> = self
            .constraints
            .iter()
            .filter(|x| x.constraints.kind() != ConstraintKind::UNSET)
            .map(|x| &x.columns)
            .collect();

        // Those indices that are not present in the constraints above
        // have constraints generated for them.
        // When `idx.is_unique`, a unique constraint is generated rather than an indexed one.
        self.indexes
            .iter()
            .filter(move |idx| !cols.contains(&idx.columns))
            .map(|idx| self.gen_constraint_def(Constraints::from_is_unique(idx.is_unique), idx.columns.clone()))
    }

    /// Check if the `name` of the [FieldName] exist on this [TableDef]
    ///
    /// Warning: It ignores the `table_id`
    pub fn get_column_by_field(&self, field: FieldName) -> Option<&RawColumnDefV8> {
        self.get_column(field.col.idx())
    }

    /// Get a column by its position in the table.
    pub fn get_column(&self, pos: usize) -> Option<&RawColumnDefV8> {
        self.columns.get(pos)
    }

    /// Check if the `col_name` exist on this [TableSchema]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_name(&self, col_name: &str) -> Option<&RawColumnDefV8> {
        self.columns.iter().find(|x| &*x.col_name == col_name)
    }
}
}
