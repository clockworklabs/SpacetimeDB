use std::fmt;

use derive_more::From;
use parking_lot::RwLock;

use crate::{
    blob_store::BlobHash,
    tiered::{
        error::ObjectStoreError,
        map::{ObjectKey, ObjectMap},
        BlobHandle, PageHandle,
    },
};

pub type ObjectBytes = Box<[u8]>;

#[derive(From)]
pub enum Object<'a> {
    Page(&'a PageHandle),
    Blob(&'a BlobHandle),
}

impl<'a> Object<'a> {
    pub fn size(&self) -> u64 {
        match self {
            Self::Page(handle) => handle.size(),
            Self::Blob(handle) => handle.size(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ObjectMetadata {
    pub size: u64,
}

impl From<&ObjectCatalogEntry> for ObjectMetadata {
    fn from(ObjectCatalogEntry { size, .. }: &ObjectCatalogEntry) -> Self {
        ObjectMetadata { size: *size }
    }
}

pub trait ObjectStore: fmt::Debug + Send + Sync + 'static {
    /// Obtain metadata about the object at `key`, or `None` if no such key
    /// exists.
    fn stat(&self, key: ObjectKey) -> Result<Option<ObjectMetadata>, ObjectStoreError>;
    /// Read the raw bytes of the object at `key`, or `None` if no such key
    /// exists.
    ///
    /// The implementation must verify that the object hashes to `key`, and
    /// return [ObjectStoreError::HashMismatch] if it doesn't.
    fn read(&self, key: ObjectKey) -> Result<Option<ObjectBytes>, ObjectStoreError>;
    /// Store `object` and return an [ObjectKey] by which the object can be
    /// retrieved later.
    fn write(&self, object: Object<'_>) -> Result<ObjectKey, ObjectStoreError>;
}

impl ObjectStore for () {
    fn stat(&self, _: ObjectKey) -> Result<Option<ObjectMetadata>, ObjectStoreError> {
        unimplemented!("null object store does not support `stat`")
    }

    fn read(&self, _: ObjectKey) -> Result<Option<ObjectBytes>, ObjectStoreError> {
        unimplemented!("null object store does not support `read`")
    }

    fn write(&self, _: Object<'_>) -> Result<ObjectKey, ObjectStoreError> {
        unimplemented!("null object store does not support `store`")
    }
}

#[derive(Debug, Default)]
pub struct MemoryObjectStore {
    objects: RwLock<ObjectMap<ObjectBytes>>,
}

impl ObjectStore for MemoryObjectStore {
    fn stat(&self, key: ObjectKey) -> Result<Option<ObjectMetadata>, ObjectStoreError> {
        Ok(self.objects.read().get(&key).map(|bytes| ObjectMetadata {
            size: bytes.len() as u64,
        }))
    }

    fn read(&self, key: ObjectKey) -> Result<Option<ObjectBytes>, ObjectStoreError> {
        Ok(self.objects.read().get(&key).cloned())
    }

    fn write(&self, object: Object<'_>) -> Result<ObjectKey, ObjectStoreError> {
        let (key, bytes) = match object {
            Object::Page(handle) => {
                let (hash, bytes) = handle.serialize_for_storage();
                (ObjectKey::from(hash), bytes.into())
            }
            Object::Blob(handle) => {
                let hash = BlobHash::hash_from_bytes(handle);
                let bytes = (&**handle).into();
                (ObjectKey::from(hash), bytes)
            }
        };
        self.objects.write().insert(key, bytes);
        Ok(key)
    }
}

#[derive(Debug)]
pub struct TieredStorage {
    local: Box<dyn ObjectStore>,
    remote: Option<Box<dyn ObjectStore>>,

    catalog: RwLock<ObjectMap<ObjectCatalogEntry>>,
    // TODO: Should this be here? [ObjectStore]s should be managing their
    // budgets.
    //local_budget: ByteBudget,
}

impl TieredStorage {
    pub fn new(local: Box<dyn ObjectStore>, remote: Option<Box<dyn ObjectStore>>) -> Self {
        Self {
            local,
            remote,
            catalog: <_>::default(),
        }
    }

    pub fn new_for_test() -> Self {
        Self::new(Box::new(MemoryObjectStore::default()), None)
    }

    pub fn stat(&self, key: ObjectKey) -> Result<ObjectMetadata, ObjectStoreError> {
        self.catalog
            .read()
            .get(&key)
            .map(Into::into)
            .ok_or(ObjectStoreError::UnknownObject(key))
    }

    pub fn contains(&self, key: ObjectKey) -> bool {
        self.catalog.read().contains_key(&key)
    }

    pub fn write(&self, object: Object<'_>) -> Result<ObjectKey, ObjectStoreError> {
        let size = object.size();
        let key = self.local.write(object)?;
        self.catalog.write().insert(key, ObjectCatalogEntry { size });
        Ok(key)
    }

    pub fn read(&self, key: ObjectKey) -> Result<ObjectBytes, ObjectStoreError> {
        if !self.catalog.read().contains_key(&key) {
            return Err(ObjectStoreError::UnknownObject(key));
        }

        if let Some(object) = self.local.read(key)? {
            return Ok(object);
        }

        if let Some(remote) = &self.remote
            && let Some(object) = remote.read(key)?
        {
            return Ok(object);
        }

        Err(ObjectStoreError::MissingObject(key))
    }
}

#[derive(Debug)]
struct ObjectCatalogEntry {
    size: u64,
}
