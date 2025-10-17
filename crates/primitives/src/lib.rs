#![cfg_attr(not(test), no_std)]

mod attr;
mod col_list;
pub mod errno;
mod ids;

pub use attr::{AttributeKind, ColumnAttribute, ConstraintKind, Constraints};
pub use col_list::{ColList, ColOrCols, ColSet};
pub use ids::{
    ColId, ConstraintId, FunctionId, IndexId, ProcedureId, ReducerId, ScheduleId, SequenceId, TableId, ViewId,
};

/// The minimum size of a chunk yielded by a wasm abi RowIter.
pub const ROW_ITER_CHUNK_SIZE: usize = 32 * 1024;
