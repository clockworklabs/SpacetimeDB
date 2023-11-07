use std::path::Path;

use crate::error::DBError;
use bytes;

use crate::hash::Hash;

pub mod memory_object_db;

pub mod hashmap_object_db;

#[cfg(feature = "odb_rocksdb")]
pub mod rocks_object_db;

#[cfg(feature = "odb_sled")]
pub mod sled_object_db;

// Trait defined for any object store which maps keys ("Hash") to their in-memory or secondary
// storage format.
pub trait ObjectDB {
    fn add(&mut self, bytes: Vec<u8>) -> Hash;
    fn get(&self, hash: Hash) -> Option<bytes::Bytes>;
    fn flush(&mut self) -> Result<(), DBError>;
    fn sync_all(&mut self) -> Result<(), DBError>;
    fn size_on_disk(&self) -> Result<u64, DBError>;
}

/// Create an instance of an on-disk object store using the default implementation.
pub fn persistent(path: impl AsRef<Path>) -> Result<Box<dyn ObjectDB + Send>, DBError> {
    #[cfg(feature = "odb_sled")]
    let odb = sled_object_db::SledObjectDB::open(path)?;
    #[cfg(not(feature = "odb_sled"))]
    let odb = hashmap_object_db::HashMapObjectDB::open(path)?;

    Ok(Box::new(odb))
}

/// Create an ephemeral (in-memory) object store using the default implementation.
pub fn ephemeral() -> Result<Box<dyn ObjectDB + Send>, DBError> {
    Ok(Box::<memory_object_db::MemoryObjectDB>::default())
}
