//! Provides identifiers such as `TableId`.

use core::fmt;

macro_rules! system_id {
    ($name:ident, $backing_ty:ty) => {
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
    };
}

system_id!(TableId, u32);
system_id!(SequenceId, u32);
system_id!(IndexId, u32);
system_id!(ConstraintId, u32);
system_id!(ColId, u16);
