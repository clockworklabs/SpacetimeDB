//! Provides identifiers such as `TableId`.

use core::fmt;

use enum_as_inner::EnumAsInner;

macro_rules! system_id {
    ($(#[$($doc_comment:tt)*])* pub struct $name:ident(pub $backing_ty:ty);) => {

        $(#[$($doc_comment)*])*
        #[derive(Debug, Default, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
        #[repr(transparent)]
        pub struct $name(pub $backing_ty);

        impl From<$backing_ty> for $name {
            fn from(value: $backing_ty) -> Self {
                Self(value)
            }
        }
        impl From<$name> for $backing_ty {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl $name {
            /// Convert `self` to a `usize` suitable for indexing into an array.
            pub fn idx(self) -> usize {
                self.0 as usize
            }

        }

        impl nohash_hasher::IsEnabled for $name {}

        impl From<usize> for $name {
            fn from(value: usize) -> Self {
                Self(value as _)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        // Defined so that e.g., `0.into()` is possible.
        impl From<i32> for $name {
            fn from(value: i32) -> Self {
                Self(value as _)
            }
        }

        #[cfg(feature = "memory-usage")]
        impl spacetimedb_memory_usage::MemoryUsage for $name {}
    };
}
// TODO(1.0): convert this into a proper trait.
macro_rules! auto_inc_system_id {
    ($name:ident) => {
        impl $name {
            /// The sentinel value for this type.
            /// This will be initialized to a valid ID upon insertion into a system table as a primary key.
            pub const SENTINEL: Self = Self(0);

            /// Check if this ID is the sentinel value.
            pub fn is_sentinel(self) -> bool {
                self == Self::SENTINEL
            }
        }
    };
}

system_id! {
    /// An identifier for a table, unique within a database.
    pub struct TableId(pub u32);
}
auto_inc_system_id!(TableId);

system_id! {
    /// An identifier for a view, unique within a database.
    pub struct ViewId(pub u32);
}
auto_inc_system_id!(ViewId);

system_id! {
    /// An identifier for a sequence, unique within a database.
    pub struct SequenceId(pub u32);
}
auto_inc_system_id!(SequenceId);

system_id! {
    /// An identifier for an index, unique within a database.
    pub struct IndexId(pub u32);
}
auto_inc_system_id!(IndexId);

system_id! {
    /// An identifier for a constraint, unique within a database.
    pub struct ConstraintId(pub u32);
}
auto_inc_system_id!(ConstraintId);

system_id! {
    /// An identifier for a schedule, unique within a database.
    pub struct ScheduleId(pub u32);
}
auto_inc_system_id!(ScheduleId);

system_id! {
    /// The position of a column within a table.
    ///
    /// A `ColId` does NOT uniquely identify a column within a database!
    /// A pair `(TableId, ColId)` is required for this.
    /// Each table will have columns with `ColId` values ranging from `0` to `n-1`, where `n` is the number of columns in the table.
    /// A table may have at most `u16::MAX` columns.
    pub struct ColId(pub u16);
}
// ColId works differently from other system IDs and is not auto-incremented.

system_id! {
    /// The index of a reducer as defined in a module's reducers list.
    // This is never stored in a system table, but is useful to have defined here.
    pub struct ReducerId(pub u32);
}

system_id! {
    /// The index of a procedure as defined in a module's procedure list.
    // This is never stored in a system table, but is useful to have defined here.
    pub struct ProcedureId(pub u32);
}

/// An id for a function exported from a module, which may be a reducer or a procedure.
// This is never stored in a system table,
// but is useful to have defined here to provide a shared language for downstream crates.
#[derive(Clone, Copy, Debug, EnumAsInner)]
pub enum FunctionId {
    Reducer(ReducerId),
    Procedure(ProcedureId),
}
