use std::collections::HashMap;

use bytes::Bytes;
use parking_lot::RwLock;
use spacetimedb_lib::{hash::hash_bytes, Hash};

use crate::db::ostorage::ObjectDB;

/// A simple in-memory object store, mapping hashes to their contents.
#[derive(Default)]
pub struct MemoryObjectDB {
    objects: RwLock<HashMap<Hash, Bytes>>,
}

impl ObjectDB for MemoryObjectDB {
    fn add(&self, bytes: &[u8]) -> Hash {
        let hash = hash_bytes(bytes);
        self.objects
            .write()
            .entry(hash)
            .or_insert_with(|| Bytes::from(bytes.to_vec()));
        hash
    }

    fn get(&self, hash: Hash) -> Option<Bytes> {
        self.objects.read().get(&hash).cloned()
    }

    /// Flushing an in-memory object store is a no-op.
    fn flush(&self) -> Result<(), crate::error::DBError> {
        Ok(())
    }

    /// Syncing an in-memory object store is a no-op.
    fn sync_all(&self) -> Result<(), crate::error::DBError> {
        Ok(())
    }

    fn size_on_disk(&self) -> Result<u64, crate::error::DBError> {
        Ok(0)
    }
}
