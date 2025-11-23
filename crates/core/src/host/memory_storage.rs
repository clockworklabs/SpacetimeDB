use async_trait::async_trait;
use spacetimedb_lib::{hash_bytes, Hash};
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::ExternalStorage;

/// A simple [`ExternalStorage`] that stores programs in memory.
#[derive(Clone, Debug, Default)]
pub struct MemoryStorage {
    inner: Arc<RwLock<HashMap<Hash, Box<[u8]>>>>,
}

impl MemoryStorage {
    /// Create a new empty `MemoryStorage`.
    pub async fn new() -> io::Result<Self> {
        Ok(Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn get(&self, key: &Hash) -> io::Result<Option<Box<[u8]>>> {
        let guard = self.inner.read().await;
        Ok(guard.get(key).cloned())
    }

    #[tracing::instrument(level = "trace", skip(self, value))]
    pub async fn put(&self, value: &[u8]) -> io::Result<Hash> {
        let h = hash_bytes(value);
        let mut guard = self.inner.write().await;
        guard.insert(h, Box::from(value));
        Ok(h)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn prune(&self, key: &Hash) -> anyhow::Result<()> {
        let mut guard = self.inner.write().await;
        guard.remove(key);
        Ok(())
    }
}

#[async_trait]
impl ExternalStorage for MemoryStorage {
    async fn lookup(&self, program_hash: Hash) -> anyhow::Result<Option<Box<[u8]>>> {
        self.get(&program_hash).await.map_err(Into::into)
    }

    async fn put(&self, program_bytes: &[u8]) -> anyhow::Result<Hash> {
        self.put(program_bytes).await.map_err(Into::into)
    }
}
