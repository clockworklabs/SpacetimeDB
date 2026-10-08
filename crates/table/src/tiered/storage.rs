use std::fmt::{self};

use parking_lot::RwLock;

use crate::tiered::{
    error::ObjectStoreError,
    map::{ObjectKey, ObjectMap},
};

pub type ObjectBytes = Box<[u8]>;

#[derive(Clone, Copy, Debug)]
pub struct ObjectMetadata {
    pub size: u64,
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
    /// Store `bytes` and return an [ObjectKey] by which the object can be
    /// retrieved later.
    fn store(&self, bytes: ObjectBytes) -> Result<ObjectKey, ObjectStoreError>;
}

impl ObjectStore for () {
    fn stat(&self, _: ObjectKey) -> Result<Option<ObjectMetadata>, ObjectStoreError> {
        unimplemented!("null object store does not support `stat`")
    }

    fn read(&self, _: ObjectKey) -> Result<Option<ObjectBytes>, ObjectStoreError> {
        unimplemented!("null object store does not support `read`")
    }

    fn store(&self, _: ObjectBytes) -> Result<ObjectKey, ObjectStoreError> {
        unimplemented!("null object store does not support `store`")
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
    pub fn new_for_test() -> Self {
        Self {
            local: Box::new(()),
            remote: None,

            catalog: <_>::default(),
            //local_budget: ByteBudget::new(ByteBudgetConfig::unlimited()).unwrap(),
        }
    }

    pub fn stat(&self, key: ObjectKey) -> Result<ObjectMetadata, ObjectStoreError> {
        self.catalog
            .read()
            .get(&key)
            .map(|&ObjectCatalogEntry { size }| ObjectMetadata { size })
            .ok_or(ObjectStoreError::UnknownObject(key))
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
