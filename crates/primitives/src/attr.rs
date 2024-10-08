//! Constraints and Column attributes.
//!
//! ## For tables
//!
//! The database engine support a sub-set of constraints defined by the `SQL` standard:
//!
//! ### UNIQUE:
//!
//!  One or many columns with enforced uniqueness using an auto-generate index.
//!
//! ### IDENTITY:
//!
//! A column that is made up of values generated by the database using a sequence.
//!
//!  Differs from `PRIMARY_KEY` in that its values are managed by the database and usually cannot be modified.
//!
//! ### PRIMARY_KEY:
//!
//! One or many columns that uniquely identifies each record in a table.
//!
//! Enforce uniqueness using an auto-generate index.
//!
//! Can only be one per-table.
//!
//! ## For Columns
//!
//! Additionally, is possible to add markers to columns as:
//!
//! - AUTO_INC: Auto-generate a sequence
//! - INDEXED: Auto-generate a non-unique index
//! - PRIMARY_KEY_AUTO: Make it a `PRIMARY_KEY` + `AUTO_INC`
//! - PRIMARY_KEY_IDENTITY: Make it a `PRIMARY_KEY` + `IDENTITY`
//!
//! NOTE: We have [ConstraintKind] and [AttributeKind] intentionally semi-duplicated because
//! the first is for the [Constrains] that are per-table and the second is for markers of the column.

//TODO: This needs a proper refactor, and use types for `column attributes` and `table tributes`
/// The assigned constraint for a `Table`
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub enum ConstraintKind {
    UNSET,
    ///  Index no unique
    INDEXED,
    /// Index unique
    UNIQUE,
    /// Unique + AutoInc
    IDENTITY,
    /// Primary key column (implies Unique)
    PRIMARY_KEY,
    /// PrimaryKey + AutoInc
    PRIMARY_KEY_AUTO,
    /// PrimaryKey + Identity
    PRIMARY_KEY_IDENTITY,
}

/// The assigned constraint OR auto-inc marker for a `Column`
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Eq, PartialEq)]
pub enum AttributeKind {
    UNSET,
    ///  Index no unique
    INDEXED,
    ///  Auto Increment
    AUTO_INC,
    /// Index unique
    UNIQUE,
    /// Unique + AutoInc
    IDENTITY,
    /// Primary key column (implies Unique).
    PRIMARY_KEY,
    /// PrimaryKey + AutoInc
    PRIMARY_KEY_AUTO,
    /// PrimaryKey + Identity
    PRIMARY_KEY_IDENTITY,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
    pub struct ColumnAttribute: u8 {
        const UNSET = Self::empty().bits();
        ///  Index no unique
        const INDEXED = 0b0001;
        /// Generate the next [Sequence]
        const AUTO_INC = 0b0010;
        /// Index unique
        const UNIQUE = Self::INDEXED.bits() | 0b0100;
        /// Unique + AutoInc
        const IDENTITY = Self::UNIQUE.bits() | Self::AUTO_INC.bits();
        /// Primary key column (implies Unique)
        const PRIMARY_KEY = Self::UNIQUE.bits() | 0b1000;
        /// PrimaryKey + AutoInc
        const PRIMARY_KEY_AUTO = Self::PRIMARY_KEY.bits() | Self::AUTO_INC.bits();
         /// PrimaryKey + Identity
        const PRIMARY_KEY_IDENTITY = Self::PRIMARY_KEY.bits() | Self::IDENTITY.bits() ;
    }
}

impl ColumnAttribute {
    /// Checks if either 'IDENTITY' or 'PRIMARY_KEY_AUTO' constraints are set because the imply the use of
    /// auto increment sequence.
    pub const fn has_autoinc(&self) -> bool {
        self.contains(Self::IDENTITY) || self.contains(Self::PRIMARY_KEY_AUTO) || self.contains(Self::AUTO_INC)
    }

    /// Checks if the 'UNIQUE' constraint is set.
    pub const fn has_unique(&self) -> bool {
        self.contains(Self::UNIQUE)
    }

    /// Checks if the 'INDEXED' constraint is set.
    pub const fn has_indexed(&self) -> bool {
        self.contains(ColumnAttribute::INDEXED)
    }

    /// Checks if the 'PRIMARY_KEY' constraint is set.
    pub const fn has_primary_key(&self) -> bool {
        self.contains(ColumnAttribute::PRIMARY_KEY)
            || self.contains(ColumnAttribute::PRIMARY_KEY_AUTO)
            || self.contains(ColumnAttribute::PRIMARY_KEY_IDENTITY)
    }

    /// Returns the [ColumnAttribute] of constraints as an enum variant.
    ///
    /// NOTE: This represent the higher possible representation of a constraints, so for example
    /// `IDENTITY` imply that is `INDEXED, UNIQUE`
    pub fn kind(&self) -> AttributeKind {
        match *self {
            x if x == Self::UNSET => AttributeKind::UNSET,
            x if x == Self::INDEXED => AttributeKind::INDEXED,
            x if x == Self::UNIQUE => AttributeKind::UNIQUE,
            x if x == Self::AUTO_INC => AttributeKind::AUTO_INC,
            x if x == Self::IDENTITY => AttributeKind::IDENTITY,
            x if x == Self::PRIMARY_KEY => AttributeKind::PRIMARY_KEY,
            x if x == Self::PRIMARY_KEY_AUTO => AttributeKind::PRIMARY_KEY_AUTO,
            x if x == Self::PRIMARY_KEY_IDENTITY => AttributeKind::PRIMARY_KEY_IDENTITY,
            x => unreachable!("Unexpected value {x:?}"),
        }
    }
}

/// Represents constraints for a database table. May apply to multiple columns.
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Constraints {
    attr: ColumnAttribute,
}

impl Constraints {
    /// Creates a new `Constraints` instance with the given `attr` flags.
    #[inline(always)]
    const fn new(attr: ColumnAttribute) -> Self {
        Self { attr }
    }

    /// Creates a new `Constraints` instance that is [`Self::unique`] if `is_unique`
    /// and [`Self::indexed`] otherwise.
    pub const fn from_is_unique(is_unique: bool) -> Self {
        if is_unique {
            Self::unique()
        } else {
            Self::indexed()
        }
    }

    /// Creates a new `Constraints` instance with no constraints set.
    pub const fn unset() -> Self {
        Self::new(ColumnAttribute::UNSET)
    }

    /// Creates a new `Constraints` instance with [ColumnAttribute::INDEXED] set.
    pub const fn indexed() -> Self {
        Self::new(ColumnAttribute::INDEXED)
    }

    /// Creates a new `Constraints` instance with [ColumnAttribute::UNIQUE] constraint set.
    pub const fn unique() -> Self {
        Self::new(ColumnAttribute::UNIQUE)
    }

    /// Creates a new `Constraints` instance with [ColumnAttribute::IDENTITY] set.
    pub const fn identity() -> Self {
        Self::new(ColumnAttribute::IDENTITY)
    }

    /// Creates a new `Constraints` instance with [ColumnAttribute::PRIMARY_KEY] set.
    pub const fn primary_key() -> Self {
        Self::new(ColumnAttribute::PRIMARY_KEY)
    }

    /// Creates a new `Constraints` instance with [ColumnAttribute::PRIMARY_KEY_AUTO] set.
    pub const fn primary_key_auto() -> Self {
        Self::new(ColumnAttribute::PRIMARY_KEY_AUTO)
    }

    /// Creates a new `Constraints` instance with [ColumnAttribute::PRIMARY_KEY_IDENTITY] set.
    pub const fn primary_key_identity() -> Self {
        Self::new(ColumnAttribute::PRIMARY_KEY_IDENTITY)
    }

    /// Creates a new `Constraints` instance with [ColumnAttribute::AUTO_INC] set.
    pub const fn auto_inc() -> Self {
        Self::new(ColumnAttribute::AUTO_INC)
    }

    /// Adds a constraint to the existing constraints.
    ///
    /// # Example
    ///
    /// ```
    /// use spacetimedb_primitives::Constraints;
    ///
    /// let constraints = Constraints::unset().push(Constraints::indexed());
    /// assert!(constraints.has_indexed());
    /// ```
    pub fn push(self, other: Constraints) -> Self {
        Self::new(self.attr | other.attr)
    }

    /// Add auto-increment constraint to the existing constraints.
    /// Returns Err if the result would not be valid.
    #[allow(clippy::result_unit_err)]
    pub fn push_auto_inc(self) -> Result<Self, ()> {
        Self::try_from(self.attr | ColumnAttribute::AUTO_INC)
    }

    /// Returns the bits representing the constraints.
    pub const fn bits(&self) -> u8 {
        self.attr.bits()
    }

    /// Returns the [ConstraintKind] of constraints as an enum variant.
    ///
    /// NOTE: This represent the higher possible representation of a constraints, so for example
    /// `IDENTITY` imply that is `INDEXED, UNIQUE`
    pub fn kind(&self) -> ConstraintKind {
        match self {
            x if x.attr == ColumnAttribute::UNSET => ConstraintKind::UNSET,
            x if x.attr == ColumnAttribute::INDEXED => ConstraintKind::INDEXED,
            x if x.attr == ColumnAttribute::UNIQUE => ConstraintKind::UNIQUE,
            x if x.attr == ColumnAttribute::IDENTITY => ConstraintKind::IDENTITY,
            x if x.attr == ColumnAttribute::PRIMARY_KEY => ConstraintKind::PRIMARY_KEY,
            x if x.attr == ColumnAttribute::PRIMARY_KEY_AUTO => ConstraintKind::PRIMARY_KEY_AUTO,
            x if x.attr == ColumnAttribute::PRIMARY_KEY_IDENTITY => ConstraintKind::PRIMARY_KEY_IDENTITY,
            x => unreachable!("Unexpected value {x:?}"),
        }
    }

    pub fn contains(&self, other: &Self) -> bool {
        self.attr.contains(other.attr)
    }

    /// Checks if the 'UNIQUE' constraint is set.
    pub const fn has_unique(&self) -> bool {
        self.attr.has_unique()
    }

    /// Checks if the 'INDEXED' constraint is set.
    pub const fn has_indexed(&self) -> bool {
        self.attr.has_indexed()
    }

    /// Checks if either 'IDENTITY' or 'PRIMARY_KEY_AUTO' constraints are set because the imply the use of
    /// auto increment sequence.
    pub const fn has_autoinc(&self) -> bool {
        self.attr.has_autoinc()
    }

    /// Checks if the 'PRIMARY_KEY' constraint is set.
    pub const fn has_primary_key(&self) -> bool {
        self.attr.has_primary_key()
    }
}

impl TryFrom<u8> for Constraints {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        ColumnAttribute::from_bits(v).ok_or(()).map(Self::new)
    }
}

impl TryFrom<ColumnAttribute> for Constraints {
    type Error = ();

    fn try_from(value: ColumnAttribute) -> Result<Self, Self::Error> {
        Ok(match value.kind() {
            AttributeKind::UNSET => Self::unset(),
            AttributeKind::INDEXED => Self::indexed(),
            AttributeKind::UNIQUE => Self::unique(),
            AttributeKind::IDENTITY => Self::identity(),
            AttributeKind::PRIMARY_KEY => Self::primary_key(),
            AttributeKind::PRIMARY_KEY_AUTO => Self::primary_key_auto(),
            AttributeKind::PRIMARY_KEY_IDENTITY => Self::primary_key_identity(),
            AttributeKind::AUTO_INC => return Err(()),
        })
    }
}
