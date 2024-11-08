//! Raw definitions of the database schema.
//!
//! Modules serialize these types and send them across the ABI boundary to describe to the database what tables they expect.
//! (Wrapped in the type `spacetimedb_lib::ModuleDef`.)
//!
//! There will eventually be multiple versions of these types wrapped in a top-level enum.
//! This is because the only backwards-compatible schema changes allowed by BSATN is adding variants to an existing enum.
//! The `spacetimedb_schema` crate will in the future perform validation and normalization of these `Raw` types to a canonical form,
//! which will be used everywhere.

pub mod v8;

// for backwards-compatibility
pub use v8::*;

pub mod v9;
