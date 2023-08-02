use std::collections::HashMap;

use bytes::Bytes;
use spacetimedb_lib::{hash::hash_bytes, Hash};

use crate::db::ostorage::ObjectDB;

/// A simple in-memory object store, mapping hashes to their contents.
pub struct MemoryObjectDB {
    objects: HashMap<Hash, Bytes>,
}

impl MemoryObjectDB {
    pub fn new() -> Self {
        eprintln!("GGKK in memory");
        let objects = HashMap::new();
        Self { objects }
    }
}

impl Default for MemoryObjectDB {
    fn default() -> Self {
        Self::new()
    }
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
}
