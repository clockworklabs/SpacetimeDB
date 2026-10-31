use std::{fmt, sync::Arc};

use derive_more::From;
use parking_lot::RwLock;

use crate::{
    blob_store::BlobHash,
    tiered::{
        error::ObjectStoreError,
        map::{ObjectKey, ObjectMap},
        BlobHandle, BudgetPermit, ByteBudget, PageHandle,
    },
};

pub type ObjectBytes = Arc<[u8]>; // TODO: Use `bytes::Bytes`?

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
pub struct BackingStore {
    pub object_store: Box<dyn ObjectStore>,
    pub budget: ByteBudget,
}

#[derive(Debug)]
pub struct TieredStorage {
    local: BackingStore,
    remote: Option<BackingStore>,

    catalog: RwLock<ObjectMap<ObjectCatalogEntry>>,
}

impl TieredStorage {
    pub fn with_catalog(
        local: BackingStore,
        remote: Option<BackingStore>,
        catalog: impl Iterator<Item = (ObjectKey, u64)>,
    ) -> Self {
        let catalog = RwLock::new(
            catalog
                .map(|(key, size)| {
                    (
                        key,
                        ObjectCatalogEntry {
                            size,
                            permit: Some(local.budget.force_acquire(size)),
                        },
                    )
                })
                .collect::<ObjectMap<_>>(),
        );

        Self { local, remote, catalog }
    }

    pub fn empty(local: BackingStore, remote: Option<BackingStore>) -> Self {
        Self {
            local,
            remote,
            catalog: <_>::default(),
        }
    }

    pub fn new_for_test() -> Self {
        Self::empty(
            BackingStore {
                object_store: Box::new(MemoryObjectStore::default()),
                budget: ByteBudget::unlimited(),
            },
            None,
        )
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
        // TODO: Observe the budget limit here. We don't know yet how to evict
        // entries, though.
        let permit = self.local.budget.force_acquire(size);
        let key = self.local.object_store.write(object)?;
        self.catalog.write().insert(
            key,
            ObjectCatalogEntry {
                size,
                permit: Some(permit),
            },
        );
        Ok(key)
    }

    pub fn read(&self, key: ObjectKey) -> Result<ObjectBytes, ObjectStoreError> {
        if !self.catalog.read().contains_key(&key) {
            return Err(ObjectStoreError::UnknownObject(key));
        }

        if let Some(object) = self.local.object_store.read(key)? {
            return Ok(object);
        }

        if let Some(remote) = &self.remote
            && let Some(object) = remote.object_store.read(key)?
        {
            return Ok(object);
        }

        Err(ObjectStoreError::MissingObject(key))
    }
}

#[cfg(test)]
impl TieredStorage {
    /// Make a [TieredStorage] that can be made to fail read access when the
    /// returned atomic bool is set to `true`.
    pub(crate) fn with_failing_reads_for_test() -> (Self, Arc<std::sync::atomic::AtomicBool>) {
        use std::{
            io,
            sync::atomic::{AtomicBool, Ordering},
        };

        #[derive(Debug, Default)]
        struct FailingReads {
            inner: MemoryObjectStore,
            fail_reads: Arc<AtomicBool>,
        }

        impl ObjectStore for FailingReads {
            fn stat(&self, key: ObjectKey) -> Result<Option<ObjectMetadata>, ObjectStoreError> {
                self.inner.stat(key)
            }

            fn read(&self, key: ObjectKey) -> Result<Option<ObjectBytes>, ObjectStoreError> {
                if self.fail_reads.load(Ordering::Relaxed) {
                    return Err(io::Error::other("injected object-store read failure").into());
                }
                self.inner.read(key)
            }

            fn write(&self, object: Object<'_>) -> Result<ObjectKey, ObjectStoreError> {
                self.inner.write(object)
            }
        }

        let store = FailingReads::default();
        let fail_reads = store.fail_reads.clone();
        (
            Self::empty(
                BackingStore {
                    object_store: Box::new(store),
                    budget: ByteBudget::unlimited(),
                },
                None,
            ),
            fail_reads,
        )
    }
}

#[derive(Debug)]
struct ObjectCatalogEntry {
    size: u64,
    #[allow(unused)]
    permit: Option<BudgetPermit>,
}
