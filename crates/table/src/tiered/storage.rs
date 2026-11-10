use std::{
    fmt,
    ops::Deref,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use parking_lot::{Mutex, RwLock};

use crate::tiered::{
    error::ObjectStoreError,
    map::{BlobHashWrapper, ObjectKey, ObjectMap, PageHash},
    BlobHandle, BudgetPermit, ByteBudget, PageHandle,
};

pub type ObjectBytes = Arc<[u8]>; // TODO: Use `bytes::Bytes`?

pub struct Object {
    inner: ObjectInner,
}

/// The inner object contains the verified hash and raw bytes of the object.
///
/// It can only be created via the `From` impls of [Object], which makes the
/// hash trustworthy.
enum ObjectInner {
    Page { hash: PageHash, bytes: ObjectBytes },
    Blob { hash: BlobHashWrapper, bytes: ObjectBytes },
}

impl ObjectInner {
    #[inline]
    pub fn size(&self) -> u64 {
        match self {
            Self::Page { bytes, .. } => bytes.len() as _,
            Self::Blob { bytes, .. } => bytes.len() as _,
        }
    }

    #[inline]
    pub fn into_kv(self) -> (ObjectKey, ObjectBytes) {
        match self {
            Self::Page { hash, bytes } => (hash.into(), bytes),
            Self::Blob { hash, bytes } => (hash.into(), bytes),
        }
    }
}

impl Object {
    pub fn size(&self) -> u64 {
        self.inner.size()
    }

    pub fn into_kv(self) -> (ObjectKey, ObjectBytes) {
        self.inner.into_kv()
    }
}

impl From<&BlobHandle> for Object {
    fn from(handle: &BlobHandle) -> Self {
        let hash = handle.hash();
        let bytes = handle.deref().into();
        Self {
            inner: ObjectInner::Blob { hash, bytes },
        }
    }
}

impl From<&PageHandle> for Object {
    fn from(handle: &PageHandle) -> Self {
        let (hash, bytes) = handle.serialize_for_storage();
        Self {
            inner: ObjectInner::Page {
                hash,
                bytes: bytes.into(),
            },
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
    fn write(&self, object: Object) -> Result<ObjectKey, ObjectStoreError>;
}

impl<T: ObjectStore> ObjectStore for Arc<T> {
    #[inline]
    fn stat(&self, key: ObjectKey) -> Result<Option<ObjectMetadata>, ObjectStoreError> {
        (**self).stat(key)
    }

    #[inline]
    fn read(&self, key: ObjectKey) -> Result<Option<ObjectBytes>, ObjectStoreError> {
        (**self).read(key)
    }

    #[inline]
    fn write(&self, object: Object) -> Result<ObjectKey, ObjectStoreError> {
        (**self).write(object)
    }
}

impl ObjectStore for () {
    fn stat(&self, _: ObjectKey) -> Result<Option<ObjectMetadata>, ObjectStoreError> {
        unimplemented!("null object store does not support `stat`")
    }

    fn read(&self, _: ObjectKey) -> Result<Option<ObjectBytes>, ObjectStoreError> {
        unimplemented!("null object store does not support `read`")
    }

    fn write(&self, _: Object) -> Result<ObjectKey, ObjectStoreError> {
        unimplemented!("null object store does not support `store`")
    }
}

/// Extension trait for [ObjectStore] that adds the capability to remove
/// objects.
///
/// Currently, only local object stores can support object removals.
///
/// # Safety
///
/// Safety requirements as per method documentation.
///
/// This trait is unsafe only by convention: the caller is forced to add
/// "SAFETY" documentation.
pub unsafe trait RemoveObject {
    /// Remove the object at `key` from the object store.
    ///
    /// Removing a key twice is a no-op, not an error.
    ///
    /// # Safety
    ///
    /// The caller must uphold that the object is available in a further storage
    /// tier, e.g. remote or achival storage.
    unsafe fn remove(&self, key: ObjectKey) -> Result<(), ObjectStoreError>;
}

pub trait LocalStorage: ObjectStore + RemoveObject {}
impl<T> LocalStorage for T where T: ObjectStore + RemoveObject {}

pub trait RemoteStorage: ObjectStore {}
impl<T> RemoteStorage for T where T: ObjectStore {}

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

    fn write(&self, object: Object) -> Result<ObjectKey, ObjectStoreError> {
        let (key, bytes) = object.inner.into_kv();
        self.objects.write().insert(key, bytes);
        Ok(key)
    }
}

unsafe impl RemoveObject for MemoryObjectStore {
    unsafe fn remove(&self, key: ObjectKey) -> Result<(), ObjectStoreError> {
        self.objects.write().remove(&key);
        Ok(())
    }
}

#[derive(Debug)]
pub struct BackingStore<T> {
    pub object_store: T,
    pub budget: ByteBudget,
}

pub type LocalBackingStore = BackingStore<Box<dyn LocalStorage>>;
pub type RemoteBackingStore = BackingStore<Box<dyn RemoteStorage>>;

type SharedCatalogEntry = Arc<Mutex<Option<ObjectCatalogEntry>>>;

#[derive(Debug)]
pub struct TieredStorage {
    local: LocalBackingStore,
    remote: Option<RemoteBackingStore>,
    access_epoch: AtomicU64,

    catalog: RwLock<ObjectMap<SharedCatalogEntry>>,
}

impl TieredStorage {
    pub fn with_catalog(
        local: LocalBackingStore,
        remote: Option<RemoteBackingStore>,
        catalog: impl Iterator<Item = (ObjectKey, u64)>,
    ) -> Self {
        let access_epoch = AtomicU64::new(1);
        let catalog = RwLock::new(
            catalog
                .map(|(key, size)| {
                    let entry = ObjectCatalogEntry {
                        size,
                        last_access_epoch: 1,
                        permit: Some(local.budget.force_acquire(size)),
                    };
                    (key, Arc::new(Mutex::new(Some(entry))))
                })
                .collect::<ObjectMap<_>>(),
        );

        Self {
            local,
            remote,
            access_epoch,
            catalog,
        }
    }

    pub fn empty(local: LocalBackingStore, remote: Option<RemoteBackingStore>) -> Self {
        Self {
            local,
            remote,
            access_epoch: AtomicU64::new(1),
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
        let slot = self.catalog.read().get(&key).cloned().ok_or_else(|| unknown(key))?;
        let entry = slot.lock();
        entry.as_ref().map(Into::into).ok_or_else(|| unknown(key))
    }

    pub fn contains(&self, key: ObjectKey) -> bool {
        let slot = self.catalog.read().get(&key).cloned();
        slot.is_some_and(|entry| entry.lock().is_some())
    }

    pub fn write(&self, object: Object) -> Result<ObjectKey, ObjectStoreError> {
        // Acquiring a permit can evict this key, so it must precede locking the
        // slot.
        let permit = self.acquire_local_permit(object.size())?;
        let (key, bytes) = object.into_kv();
        // Lock the slot while writing the object, preventing concurrent
        // eviction.
        let slot = self.catalog.write().entry(key).or_default().clone();
        let mut entry = slot.lock();
        let key = self.local.object_store.write(object_from_trusted_bytes(key, &bytes))?;
        *entry = Some(ObjectCatalogEntry {
            size: bytes.len() as u64,
            last_access_epoch: self.access_epoch(),
            permit: Some(permit),
        });

        Ok(key)
    }

    pub fn read(&self, key: ObjectKey) -> Result<ObjectBytes, ObjectStoreError> {
        let slot = self.catalog.read().get(&key).cloned().ok_or_else(|| unknown(key))?;
        // Try to read from local storage first.
        {
            let mut entry = slot.lock();
            let entry = entry.as_mut().ok_or_else(|| unknown(key))?;
            if let Some(object) = self.local.object_store.read(key)? {
                entry.last_access_epoch = self.access_epoch();
                return Ok(object);
            }
        }

        // If remote storage is configured, promote the object into the local
        // tier.
        if let Some(remote) = &self.remote {
            let ObjectMetadata { size } = remote.object_store.stat(key)?.ok_or_else(|| not_found(key))?;
            // Acquire the permit before locking the slot, as eviction may lock
            // it, too.
            let permit = self.acquire_local_permit(size)?;

            let mut entry = slot.lock();
            let entry = entry.as_mut().ok_or_else(|| unknown(key))?;
            // Another reader or writer may have restored the object while we
            // reserved space.
            if let Some(object) = self.local.object_store.read(key)? {
                entry.last_access_epoch = self.access_epoch();
                return Ok(object);
            }

            // Fetch the remote object and store it locally.
            // Hold the entry lock to prevent eviction.
            let bytes = remote.object_store.read(key)?.ok_or_else(|| not_found(key))?;
            self.local.object_store.write(object_from_trusted_bytes(key, &bytes))?;
            entry.last_access_epoch = self.access_epoch();
            entry.permit = Some(permit);

            return Ok(bytes);
        }

        // Entry is in the catalog, but neither in local nor remote storage.
        // TODO: panic?
        Err(not_found(key))
    }

    /// Trim the local storage until the accounted byte usage is at or below
    /// `target_usage`.
    ///
    /// If no remote storage is configured, this method does nothing.
    ///
    /// Selects the least recently read objects first.
    /// For each candidate, the object is:
    ///
    ///   - uploaded to the remote storage
    ///   - then, removed from the local storage
    ///   - finally, its budget permit is dropped
    ///
    /// This means that, in case of failures, the object may be present in
    /// remote storage, local storage, or both. It is guaranteed that the object
    /// is not removed from local storage if remote storage fails.
    ///
    /// Returns the total number of bytes freed from local storage.
    pub fn evict(&self, target_usage: u64) -> Result<u64, ObjectStoreError> {
        let Some(remote) = &self.remote else {
            return Ok(0);
        };

        // Collect cloned slots, so we can release the catalog lock.
        let slots = self
            .catalog
            .read()
            .iter()
            .map(|(key, slot)| (*key, slot.clone()))
            .collect::<Vec<_>>();
        let mut candidates = slots
            .into_iter()
            .filter_map(|(key, slot)| {
                let last_access_epoch = {
                    let entry = slot.lock();

                    // Filter out empty or non-resident slots.
                    let entry = entry.as_ref()?;
                    entry.permit.as_ref()?;

                    entry.last_access_epoch
                };
                Some((last_access_epoch, key, slot))
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable_by_key(|(last_access_epoch, ..)| *last_access_epoch);

        let mut bytes_freed = 0;
        for (_, key, slot) in candidates {
            let mut entry = slot.lock();
            if self.local.budget.usage().accounted_bytes <= target_usage {
                break;
            }

            let Some(entry) = entry.as_mut().filter(|entry| entry.permit.is_some()) else {
                continue;
            };
            let bytes = self.local.object_store.read(key)?.ok_or_else(|| not_found(key))?;
            remote.object_store.write(object_from_trusted_bytes(key, &bytes))?;
            // SAFETY: The remote write succeeded. The slot lock excludes
            // concurrent local writes until removal and release of this
            // object's permit are complete.
            unsafe { self.local.object_store.remove(key)? };

            entry.permit.take();
            bytes_freed += entry.size;
        }

        Ok(bytes_freed)
    }

    fn access_epoch(&self) -> u64 {
        self.access_epoch.fetch_add(1, Ordering::Relaxed)
    }

    /// Acquire a [BudgetPermit] to store `size` bytes in the local tier.
    ///
    /// Tries once to evict objects when permit acquisition fails.
    fn acquire_local_permit(&self, size: u64) -> Result<BudgetPermit, ObjectStoreError> {
        self.local.budget.acquire(size).or_else(|_| {
            self.evict(self.local.budget.config().low_water_bytes)?;
            self.local
                .budget
                .acquire(size)
                .map_err(Box::new)
                .map_err(ObjectStoreError::from)
        })
    }
}

fn object_from_trusted_bytes(key: ObjectKey, bytes: &ObjectBytes) -> Object {
    let inner = match key {
        ObjectKey::Page(hash) => ObjectInner::Page {
            hash,
            bytes: bytes.clone(),
        },
        ObjectKey::Blob(hash) => ObjectInner::Blob {
            hash,
            bytes: bytes.clone(),
        },
    };
    Object { inner }
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

            fn write(&self, object: Object) -> Result<ObjectKey, ObjectStoreError> {
                self.inner.write(object)
            }
        }

        unsafe impl RemoveObject for FailingReads {
            unsafe fn remove(&self, key: ObjectKey) -> Result<(), ObjectStoreError> {
                // SAFETY: Forward caller requirements.
                unsafe { self.inner.remove(key) }
            }
        }

        let store = FailingReads::default();
        let fail_reads = store.fail_reads.clone();
        (
            Self::empty(
                LocalBackingStore {
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
    last_access_epoch: u64,
    /// The local tier budget permit.
    ///
    /// `Some` if the object is resident, otherwise `None`.
    permit: Option<BudgetPermit>,
}

fn not_found(key: ObjectKey) -> ObjectStoreError {
    ObjectStoreError::MissingObject(key)
}

fn unknown(key: ObjectKey) -> ObjectStoreError {
    ObjectStoreError::UnknownObject(key)
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        sync::{atomic::AtomicBool, mpsc},
        thread,
        time::Duration,
    };

    use super::*;
    use crate::tiered::ByteBudgetConfig;

    const TIMEOUT: Duration = Duration::from_secs(10);

    #[derive(Debug)]
    struct Pause {
        entered: mpsc::SyncSender<()>,
        resume: Mutex<mpsc::Receiver<()>>,
    }

    fn pause() -> (Arc<Pause>, mpsc::Receiver<()>, mpsc::SyncSender<()>) {
        let (entered, reached) = mpsc::sync_channel(1);
        let (resume, receiver) = mpsc::sync_channel(1);
        (
            Arc::new(Pause {
                entered,
                resume: Mutex::new(receiver),
            }),
            reached,
            resume,
        )
    }

    impl Pause {
        fn wait(&self) {
            self.entered.send(()).unwrap();
            self.resume.lock().recv_timeout(TIMEOUT).unwrap();
        }
    }

    #[derive(Debug, Default)]
    struct TestStore {
        inner: MemoryObjectStore,
        after_remove: Option<Arc<Pause>>,
        fail_writes: AtomicBool,
        fail_remove: AtomicBool,
    }

    impl ObjectStore for Arc<TestStore> {
        fn stat(&self, key: ObjectKey) -> Result<Option<ObjectMetadata>, ObjectStoreError> {
            self.inner.stat(key)
        }

        fn read(&self, key: ObjectKey) -> Result<Option<ObjectBytes>, ObjectStoreError> {
            self.inner.read(key)
        }

        fn write(&self, object: Object) -> Result<ObjectKey, ObjectStoreError> {
            if self.fail_writes.load(Ordering::Relaxed) {
                return Err(io::Error::other("injected write failure").into());
            }
            self.inner.write(object)
        }
    }

    unsafe impl RemoveObject for Arc<TestStore> {
        unsafe fn remove(&self, key: ObjectKey) -> Result<(), ObjectStoreError> {
            if self.fail_remove.load(Ordering::Relaxed) {
                return Err(io::Error::other("injected remove failure").into());
            }
            // SAFETY: Forward caller requirements.
            unsafe { self.inner.remove(key)? };
            if let Some(pause) = &self.after_remove {
                pause.wait();
            }
            Ok(())
        }
    }

    fn object(bytes: &[u8]) -> Object {
        let key = BlobHashWrapper::hash_from_bytes(bytes).into();
        object_from_trusted_bytes(key, &Arc::from(bytes))
    }

    fn storage(local: Arc<TestStore>, remote: Arc<TestStore>, budget: ByteBudget) -> Arc<TieredStorage> {
        Arc::new(TieredStorage::empty(
            BackingStore {
                object_store: Box::new(local),
                budget,
            },
            Some(BackingStore {
                object_store: Box::new(remote),
                budget: ByteBudget::unlimited(),
            }),
        ))
    }

    fn assert_restoration_after_removal_keeps_its_permit(overwrite: bool) {
        // Start with one local object, then stop eviction after its remote
        // upload and local removal, but before `TieredStorage` releases its
        // budget permit.
        let (pause, reached, resume) = pause();
        let local = Arc::new(TestStore {
            after_remove: Some(pause),
            ..<_>::default()
        });
        let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());
        let key = storage.write(object(b"aaaa")).unwrap();
        let slot = storage.catalog.read().get(&key).unwrap().clone();

        let evicting = storage.clone();
        let eviction = thread::spawn(move || evicting.evict(0));
        reached.recv_timeout(TIMEOUT).unwrap();

        let transition_is_locked = slot.try_lock().is_none();
        let reading = storage.clone();
        let reader = thread::spawn(move || {
            if overwrite {
                reading.write(object(b"aaaa"))?;
            }
            reading.read(key)
        });
        // Unrelated keys can still be written while the eviction's I/O is paused.
        let other_key = storage.write(object(b"bbbb")).unwrap();
        resume.send(()).unwrap();

        assert!(transition_is_locked);
        assert_eq!(eviction.join().unwrap().unwrap(), 4);
        assert_eq!(&*reader.join().unwrap().unwrap(), b"aaaa");
        assert!(local.stat(key).unwrap().is_some());
        assert!(local.stat(other_key).unwrap().is_some());
        assert_eq!(storage.local.budget.usage().accounted_bytes, 8);
        assert!(Arc::ptr_eq(&slot, storage.catalog.read().get(&key).unwrap()));
        assert!(slot.lock().as_ref().unwrap().permit.is_some());
    }

    #[test]
    fn promotion_after_removal_keeps_its_permit() {
        assert_restoration_after_removal_keeps_its_permit(false);
    }

    #[test]
    fn overwrite_after_removal_keeps_its_permit() {
        assert_restoration_after_removal_keeps_its_permit(true);
    }

    #[test]
    fn concurrent_evictions_only_release_one_permit() {
        let (pause, reached, resume) = pause();
        let local = Arc::new(TestStore {
            after_remove: Some(pause),
            ..<_>::default()
        });
        let storage = storage(local, <_>::default(), ByteBudget::unlimited());
        let key = storage.write(object(b"aaaa")).unwrap();

        let first_storage = storage.clone();
        let first = thread::spawn(move || first_storage.evict(0));
        reached.recv_timeout(TIMEOUT).unwrap();

        let second_storage = storage.clone();
        let second = thread::spawn(move || second_storage.evict(0));
        resume.send(()).unwrap();

        assert_eq!(first.join().unwrap().unwrap() + second.join().unwrap().unwrap(), 4);
        assert_eq!(storage.local.budget.usage().accounted_bytes, 0);
        assert!(storage.contains(key));
    }

    #[test]
    fn remote_read_can_evict_without_holding_its_slot() {
        // Budget to fit exactly one object.
        let budget = ByteBudget::new(ByteBudgetConfig {
            low_water_bytes: 0,
            soft_limit_bytes: 4,
            hard_limit_bytes: 4,
        })
        .unwrap();
        let storage = storage(Arc::default(), Arc::default(), budget);

        let key = storage.write(object(b"aaaa")).unwrap();
        storage.write(object(b"bbbb")).unwrap();

        let (sender, receiver) = mpsc::sync_channel(1);
        let reading = storage.clone();
        let reader = thread::spawn(move || sender.send(reading.read(key)).unwrap());
        assert_eq!(&*receiver.recv_timeout(TIMEOUT).unwrap().unwrap(), b"aaaa");

        reader.join().unwrap();
        assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
    }

    #[test]
    fn overwrites_preserve_the_slot_and_account_once() {
        let storage = storage(Arc::default(), Arc::default(), ByteBudget::unlimited());

        let key = storage.write(object(b"aaaa")).unwrap();
        let slot = storage.catalog.read().get(&key).unwrap().clone();
        assert_eq!(storage.write(object(b"aaaa")).unwrap(), key);

        assert!(Arc::ptr_eq(&slot, storage.catalog.read().get(&key).unwrap()));
        assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
        assert_eq!(&*storage.read(key).unwrap(), b"aaaa");
    }

    #[test]
    fn failed_initial_write_does_not_publish_the_object() {
        // A write reserves a slot before doing I/O.
        // If I/O fails, that slot must stay invisible to callers and hold no
        // budget permit.
        let local = Arc::new(TestStore::default());
        local.fail_writes.store(true, Ordering::Relaxed);
        let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());

        let key = BlobHashWrapper::hash_from_bytes(b"aaaa").into();
        assert!(storage.write(object(b"aaaa")).is_err());
        let slot = storage.catalog.read().get(&key).unwrap().clone();

        assert!(!storage.contains(key));
        assert!(matches!(storage.stat(key), Err(ObjectStoreError::UnknownObject(_))));
        assert!(matches!(storage.read(key), Err(ObjectStoreError::UnknownObject(_))));
        assert_eq!(storage.local.budget.usage().accounted_bytes, 0);
        assert_eq!(storage.evict(0).unwrap(), 0);

        // Retrying publishes the object through the existing mutex, so callers
        // that already obtained the slot cannot end up locking a different one.
        local.fail_writes.store(false, Ordering::Relaxed);
        assert_eq!(storage.write(object(b"aaaa")).unwrap(), key);
        assert!(Arc::ptr_eq(&slot, storage.catalog.read().get(&key).unwrap()));
        assert_eq!(storage.stat(key).unwrap().size, 4);
        assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
    }

    #[test]
    fn failed_remote_write_preserves_local_bytes_and_permit() {
        let local = Arc::new(TestStore::default());
        let remote = Arc::new(TestStore::default());
        remote.fail_writes.store(true, Ordering::Relaxed);

        let storage = storage(local.clone(), remote, ByteBudget::unlimited());

        let key = storage.write(object(b"aaaa")).unwrap();

        assert!(storage.evict(0).is_err());
        assert!(local.stat(key).unwrap().is_some());
        assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
        assert_eq!(&*storage.read(key).unwrap(), b"aaaa");
    }

    #[test]
    fn eviction_does_nothing_when_target_is_met() {
        let local = Arc::new(TestStore::default());
        let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());

        let key = storage.write(object(b"aaaa")).unwrap();

        assert_eq!(storage.evict(4).unwrap(), 0);
        assert!(local.stat(key).unwrap().is_some());
        assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
    }

    #[test]
    fn failed_local_remove_preserves_permit() {
        let local = Arc::new(TestStore::default());
        let remote = Arc::new(TestStore::default());
        local.fail_remove.store(true, Ordering::Relaxed);

        let storage = storage(local.clone(), remote, ByteBudget::unlimited());
        let key = storage.write(object(b"aaaa")).unwrap();

        assert!(storage.evict(0).is_err());
        assert!(local.stat(key).unwrap().is_some());
        assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
        assert_eq!(&*storage.read(key).unwrap(), b"aaaa");
    }
}
