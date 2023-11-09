use std::{path::Path, sync::Arc};

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
pub trait ObjectDB: Send + Sync {
    fn add(&self, bytes: &[u8]) -> Hash;
    fn get(&self, hash: Hash) -> Option<bytes::Bytes>;
    fn flush(&self) -> Result<(), DBError>;
    fn sync_all(&self) -> Result<(), DBError>;
    fn size_on_disk(&self) -> Result<u64, DBError>;
}

impl<T: ObjectDB> ObjectDB for Arc<T> {
    fn add(&self, bytes: &[u8]) -> Hash {
        (**self).add(bytes)
    }

    fn get(&self, hash: Hash) -> Option<bytes::Bytes> {
        (**self).get(hash)
    }

    fn flush(&self) -> Result<(), DBError> {
        (**self).flush()
    }

    fn sync_all(&self) -> Result<(), DBError> {
        (**self).sync_all()
    }

    fn size_on_disk(&self) -> Result<u64, DBError> {
        (**self).size_on_disk()
    }
}

impl<T: ObjectDB + ?Sized> ObjectDB for Box<T> {
    fn add(&self, bytes: &[u8]) -> Hash {
        self.as_ref().add(bytes)
    }

    fn get(&self, hash: Hash) -> Option<bytes::Bytes> {
        self.as_ref().get(hash)
    }

    fn flush(&self) -> Result<(), DBError> {
        self.as_ref().flush()
    }

    fn sync_all(&self) -> Result<(), DBError> {
        self.as_ref().sync_all()
    }

    fn size_on_disk(&self) -> Result<u64, DBError> {
        self.as_ref().size_on_disk()
    }
}

/// Create an instance of an on-disk object store using the default implementation.
pub fn persistent(path: impl AsRef<Path>) -> Result<Box<dyn ObjectDB>, DBError> {
    #[cfg(feature = "odb_sled")]
    let odb = sled_object_db::SledObjectDB::open(path)?;
    #[cfg(not(feature = "odb_sled"))]
    let odb = hashmap_object_db::HashMapObjectDB::open(path)?;

    Ok(Box::new(odb))
}

/// Create an ephemeral (in-memory) object store using the default implementation.
pub fn ephemeral() -> Result<Box<dyn ObjectDB>, DBError> {
    Ok(Box::<memory_object_db::MemoryObjectDB>::default())
}
