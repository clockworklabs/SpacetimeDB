#![cfg_attr(not(test), no_std)]

mod attr;
mod col_list;
mod ids;

pub use attr::{AttributeKind, ColumnAttribute, ConstraintKind, Constraints};
pub use col_list::{ColList, ColListBuilder};
pub use ids::{ColId, ConstraintId, IndexId, SequenceId, TableId};
