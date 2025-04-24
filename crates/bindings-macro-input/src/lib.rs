//! Defines the parsing logic for macros like `#[spacetimedb::table]`,
//! simplifying writing SpacetimeDB modules in Rust.

// DO NOT WRITE (public) DOCS IN THIS MODULE.
// Docs should be written in the `spacetimedb` crate (i.e. `bindings/`) at reexport sites
// using `#[doc(inline)]`.
// We do this so that links to library traits, structs, etc can resolve correctly.
//
// (private documentation for the macro authors is totally fine here and you SHOULD write that!)

pub mod table;
pub mod util;
pub mod sats;
pub mod sym;
