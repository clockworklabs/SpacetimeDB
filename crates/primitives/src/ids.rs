//! Provides identifiers such as `TableId`.
use core::fmt;

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct TableId(pub u32);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct ColId(pub u32);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct IndexId(pub u32);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct SequenceId(pub u32);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct ConstraintId(pub u32);

macro_rules! system_id {
    ($name:ident) => {
        impl $name {
            pub fn idx(self) -> usize {
                self.0 as usize
            }
        }

        impl From<i32> for $name {
            fn from(value: i32) -> Self {
                Self(value as u32)
            }
        }
        impl From<u32> for $name {
            fn from(value: u32) -> Self {
                Self(value)
            }
        }
        impl From<$name> for usize {
            fn from(value: $name) -> Self {
                value.0 as usize
            }
        }
        impl From<$name> for u32 {
            fn from(value: $name) -> Self {
                value.0
            }
        }
        impl From<usize> for $name {
            fn from(value: usize) -> Self {
                Self(value as u32)
            }
        }
        impl From<u8> for $name {
            fn from(value: u8) -> Self {
                Self(value as u32)
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}
system_id!(TableId);
system_id!(ColId);
system_id!(SequenceId);
system_id!(IndexId);
system_id!(ConstraintId);
