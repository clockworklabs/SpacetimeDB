//! Index support based on BTrees
//!
//! It provides:
//!
//! - Point-query
//! - Duplicate check of keys
//! - ORDER BY iteration
pub mod btree;
pub mod manager;

pub use btree::BTreeIndex;
pub use btree::*;
pub use manager::IndexCatalog;
