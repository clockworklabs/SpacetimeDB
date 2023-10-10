// This file is `included` also in `crates/bindings-macro/src/lib.rs` to avoid duplicating
// So can't import things from `sats` crate
#![allow(dead_code)]
use bitflags::bitflags;

#[allow(non_camel_case_types)]
#[allow(clippy::upper_case_acronyms)]
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
    /// Primary key column (implies Unique)
    PRIMARY_KEY,
    /// PrimaryKey + AutoInc
    PRIMARY_KEY_AUTO,
    /// PrimaryKey + Identity
    PRIMARY_KEY_IDENTITY,
}

// This indeed is only used for defining columns + constraints AND/OR auto_inc,
// and is distinct to `Constraints` in `sats/db/def.rs`
bitflags! {
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
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
        self.contains(ColumnAttribute::IDENTITY)
            || self.contains(ColumnAttribute::PRIMARY_KEY_AUTO)
            || self.contains(ColumnAttribute::AUTO_INC)
    }

    /// Returns the [ColumnAttribute] of constraints as an enum variant.
    ///
    /// NOTE: This represent the higher possible representation of a constraints, so for example
    /// `IDENTITY` imply that is `INDEXED, UNIQUE`
    pub fn kind(&self) -> AttributeKind {
        match self {
            x if x == &ColumnAttribute::UNSET => AttributeKind::UNSET,
            x if x == &ColumnAttribute::INDEXED => AttributeKind::INDEXED,
            x if x == &ColumnAttribute::UNIQUE => AttributeKind::UNIQUE,
            x if x == &ColumnAttribute::AUTO_INC => AttributeKind::AUTO_INC,
            x if x == &ColumnAttribute::IDENTITY => AttributeKind::IDENTITY,
            x if x == &ColumnAttribute::PRIMARY_KEY => AttributeKind::PRIMARY_KEY,
            x if x == &ColumnAttribute::PRIMARY_KEY_AUTO => AttributeKind::PRIMARY_KEY_AUTO,
            x if x == &ColumnAttribute::PRIMARY_KEY_IDENTITY => AttributeKind::PRIMARY_KEY_IDENTITY,
            x => unreachable!("Unexpected value {x:?}"),
        }
    }
}
