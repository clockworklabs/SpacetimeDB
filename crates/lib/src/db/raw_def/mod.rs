//! Raw definitions of the database schema.
//!
//! Modules serialize these types and send them across the ABI boundary to describe to the database what tables they expect.
//! (Wrapped in the type `spacetimedb_lib::ModuleDef`.)
//!
//! There will eventually be multiple versions of these types wrapped in a top-level enum.
//! This is because the only backwards-compatible schema changes allowed by BSATN is adding variants to an existing enum.
//! The `spacetimedb_schema` crate will in the future perform validation and normalization of these `Raw` types to a canonical form,
//! which will be used everywhere.

use derive_more::Display;
use spacetimedb_sats::{de, ser};
pub mod v8;

/// Which type of index to create.
///
/// Currently only `IndexType::BTree` is allowed.
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Display, de::Deserialize, ser::Serialize)]
pub enum IndexType {
    /// A BTree index.
    BTree = 0,
    /// A Hash index.
    Hash = 1,
}

impl From<IndexType> for u8 {
    fn from(value: IndexType) -> Self {
        value as u8
    }
}

impl TryFrom<u8> for IndexType {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(IndexType::BTree),
            1 => Ok(IndexType::Hash),
            _ => Err(()),
        }
    }
}
