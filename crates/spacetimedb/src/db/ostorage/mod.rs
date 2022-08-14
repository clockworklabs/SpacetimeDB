use bytes;

use crate::hash::Hash;

pub mod hashmap_object_db;

#[cfg(feature = "rocksdb")]
pub mod rocks_object_db;

pub mod sled_object_db;

// Trait defined for any object store which maps keys ("Hash") to their in-memory or secondary
// storage format.
pub trait ObjectDB {
    fn add(&mut self, bytes: Vec<u8>) -> Hash;
    fn get(&self, hash: Hash) -> Option<bytes::Bytes>;
    fn flush(&mut self) -> Result<(), anyhow::Error>;
    fn sync_all(&mut self) -> Result<(), anyhow::Error>;
}
