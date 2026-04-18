//! Rust replacement for SpacetimeDB C++ bindings type registration system.
//!
//! This crate rewrites the core type registration logic from
//! `module_type_registration.cpp` in idiomatic Rust, using the existing
//! `spacetimedb-sats` and `spacetimedb-lib` crates for type definitions.

mod ffi;
mod module_type_registration;

pub use ffi::{
    has_registration_error, register_procedure, register_reducer, register_type, register_view, register_view_anon,
    registration_error,
};
pub use module_type_registration::*;
