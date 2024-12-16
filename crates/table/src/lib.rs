#![forbid(unsafe_op_in_unsafe_fn)]

//! The `spacetimedb_table` crate provides a `Table` implementation
//! and various ways to interact with a table.

// For now, all of these are public.
// We'll make as much as possible private when mem-arch has merged fully.

pub mod bflatn_from;
pub mod bflatn_to;
pub mod blob_store;
pub mod btree_index;
pub mod eq;
mod eq_to_pv;
mod fixed_bit_set;
pub mod indexes;
pub mod layout;
pub mod page;
pub mod pages;
pub mod pointer_map;
pub mod read_column;
pub mod row_hash;
pub mod row_type_visitor;
pub mod static_bsatn_validator;
pub mod static_layout;
pub mod table;
pub mod var_len;

mod memory_usage;
pub use memory_usage::MemoryUsage;

#[doc(hidden)] // Used in tests and benchmarks.
pub mod util;
