use std::collections::HashMap;

use bytes::Bytes;
use spacetimedb_sats::hash::{hash_bytes, Hash};

use crate::db::ostorage::ObjectDB;

/// A simple in-memory object store, mapping hashes to their contents.
#[derive(Default)]
pub struct MemoryObjectDB {
    objects: HashMap<Hash, Bytes>,
}

impl ObjectDB for MemoryObjectDB {
    fn add(&mut self, bytes: Vec<u8>) -> Hash {
        let hash = hash_bytes(&bytes);
        self.objects.entry(hash).or_insert_with(|| bytes.into());
        hash
    }

    fn get(&self, hash: Hash) -> Option<Bytes> {
        self.objects.get(&hash).cloned()
    }

    /// Flushing an in-memory object store is a no-op.
    fn flush(&mut self) -> Result<(), crate::error::DBError> {
        Ok(())
    }

    /// Syncing an in-memory object store is a no-op.
    fn sync_all(&mut self) -> Result<(), crate::error::DBError> {
        Ok(())
    }

    fn size_on_disk(&self) -> Result<u64, crate::error::DBError> {
        Ok(0)
    }
}

impl From<HashMap<Hash, Bytes>> for MemoryObjectDB {
    fn from(objects: HashMap<Hash, Bytes>) -> Self {
        Self { objects }
    }
}
