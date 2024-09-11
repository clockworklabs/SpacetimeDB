//! Internal SpacetimeDB schema handling.
//!
//! Handles validation and normalization of raw schema definitions from the `spacetimedb_lib` crate.

pub mod auto_migrate;
pub mod def;
pub mod error;
pub mod identifier;
pub mod schema;
pub mod type_for_generate;
