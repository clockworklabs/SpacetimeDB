//! Raw, un-validated database schemas.
//!
//! The data structures in this module are exported from compiled wasm modules to describe their database schemas.
//! They are serialized in the bsatn format.
//! Validation of these data structures is performed by the `spacetimedb-schema` crate. That is not a dependency of this crate
//! because modules don't need to validate their own schemas.

pub mod auth;
pub mod column_ordering;
pub mod error;
pub mod raw_def;
