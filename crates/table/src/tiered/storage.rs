use super::metrics::{CacheTier, EvictionDirection, FaultSource, Metrics};
use std::{
    fmt,
    num::NonZeroU64,
    ops::Deref,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

use parking_lot::{Mutex, RwLock};
use spacetimedb_lib::bsatn;

use crate::{
    page::Page,
    tiered::{
        error::ObjectStoreError,
        map::{BlobHashWrapper, ObjectKey, ObjectMap, PageHash},
        BlobHandle, BudgetPermit, ByteBudget, EvictionPolicy, PageHandle,
    },
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

    #[inline]
    pub fn to_kv(&self) -> (ObjectKey, ObjectBytes) {
        match self {
            ObjectInner::Page { hash, bytes } => ((*hash).into(), bytes.clone()),
            ObjectInner::Blob { hash, bytes } => ((*hash).into(), bytes.clone()),
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

impl From<&Page> for Object {
    fn from(page: &Page) -> Self {
        let hash = PageHash::from(page);
        let bytes = bsatn::to_vec(page).unwrap().into();
        Self {
            inner: ObjectInner::Page { hash, bytes },
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
    fn set_metrics(&mut self, _metrics: Metrics) {}
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
        let (key, bytes) = object.inner.to_kv();
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

type SharedCatalogEntry = Arc<ObjectSlot>;

/// Whether an owner requires the object to survive eviction to the remote tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskRetention {
    Evictable,
    Required,
}

impl DiskRetention {
    pub fn is_required(policy: EvictionPolicy) -> bool {
        Self::from(policy) == Self::Required
    }
}

impl From<EvictionPolicy> for DiskRetention {
    fn from(policy: EvictionPolicy) -> Self {
        match policy {
            // Objects may have been stored on disk before changing the cache
            // policy of a table. It is ok to evict those to the remote tier.
            EvictionPolicy::Never => Self::Evictable,
            // If main memory eviction is allowed, the disk copy must stay.
            EvictionPolicy::Memory => Self::Required,
            // Object is explicitly evictable.
            EvictionPolicy::Disk => Self::Evictable,
        }
    }
}

#[derive(Debug, Default)]
enum ObjectOwnership {
    #[default]
    Unreferenced,
    Referenced {
        /// The number of owners of this object.
        ///
        /// Owners may not request pinning, so this number can be greater than
        /// `total_pins`. The owner count is tracked so that we can detect the
        /// last owner in [Self::release].
        owners: NonZeroU64,
        /// Sum of all live [DiskObject]s' pin counts.
        total_pins: u64,
    },
}

impl ObjectOwnership {
    fn is_pinned(&self) -> bool {
        matches!(self, Self::Referenced { total_pins: 1.., .. })
    }

    fn retain(&mut self, pins: u64) {
        *self = match *self {
            Self::Unreferenced => Self::Referenced {
                owners: NonZeroU64::MIN,
                total_pins: pins,
            },
            Self::Referenced { owners, total_pins } => Self::Referenced {
                owners: owners.checked_add(1).expect("owner count overflow"),
                total_pins: total_pins.checked_add(pins).expect("pin count overflow"),
            },
        };
    }

    fn release(&mut self, pins: u64) -> bool {
        let Self::Referenced { owners, total_pins } = *self else {
            unreachable!("only a registered handle can release pin counts")
        };
        assert!(total_pins >= pins);

        match owners.get() {
            1 => {
                // The last owner has released its pins,
                // so the object becomes unreferenced.
                assert_eq!(total_pins, pins);
                *self = Self::Unreferenced;

                true
            }
            x => {
                *self = Self::Referenced {
                    owners: NonZeroU64::new(x - 1).unwrap(),
                    total_pins: total_pins.checked_sub(pins).expect("pin count underflow"),
                };

                false
            }
        }
    }
}

#[derive(Debug, Default)]
struct ObjectSlot {
    registering: AtomicU64,
    state: Mutex<ObjectState>,
}

#[derive(Debug, Default)]
struct ObjectState {
    object: Option<ObjectCatalogEntry>,
    ownership: ObjectOwnership,
}

/// Ownership of an object stored in the [LocalBackingStore].
///
/// Tracks pins of multiple owners. When pins go to zero, the object can be
/// evicted to the [RemoteBackingStore].
#[derive(Debug)]
pub struct DiskObject {
    key: ObjectKey,
    slot: SharedCatalogEntry,
    pins: u64,
    // Shared with [TieredStorage], so the object can signal pending
    // reclamation.
    should_reclaim: Arc<AtomicBool>,
}

impl DiskObject {
    pub fn key(&self) -> ObjectKey {
        self.key
    }

    pub fn pins(&self) -> u64 {
        self.pins
    }

    /// Prevent eviction of this object's local copy.
    ///
    /// This only modifies the catalog entry, it is the caller's responsibility
    /// to actually write the object.
    pub(crate) fn pin(&mut self) {
        let pins = self.pins.checked_add(1).expect("pin count overflow");
        self.set_pins(pins);
    }

    /// Decrement the pin count for this object.
    pub(crate) fn unpin(&mut self) {
        let pins = self.pins.checked_sub(1).expect("pin count underflow");
        self.set_pins(pins);
    }

    /// Replace the pin count contribution of this object.
    ///
    /// The caller must ensure that the object is live.
    pub(crate) fn set_pins(&mut self, pins: u64) {
        let mut state = self.slot.state.lock();
        state.ownership.release(self.pins);
        state.ownership.retain(pins);
        self.pins = pins;
    }
}

impl Clone for DiskObject {
    fn clone(&self) -> Self {
        let mut state = self.slot.state.lock();
        state.ownership.retain(self.pins);

        Self {
            key: self.key,
            slot: self.slot.clone(),
            pins: self.pins,
            should_reclaim: self.should_reclaim.clone(),
        }
    }
}

impl Drop for DiskObject {
    fn drop(&mut self) {
        let mut state = self.slot.state.lock();
        if state.ownership.release(self.pins) {
            self.should_reclaim.store(true, Ordering::Release);
        }
    }
}

#[derive(Debug)]
pub struct TieredStorage {
    local: LocalBackingStore,
    remote: Option<RemoteBackingStore>,
    access_epoch: AtomicU64,
    catalog: RwLock<ObjectMap<SharedCatalogEntry>>,
    should_reclaim: Arc<AtomicBool>,
}

impl TieredStorage {
    pub fn empty(local: LocalBackingStore, remote: Option<RemoteBackingStore>) -> Self {
        Self {
            local,
            remote,
            access_epoch: 1.into(),
            catalog: <_>::default(),
            should_reclaim: Arc::new(false.into()),
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

    pub(super) fn local_budget(&self) -> &ByteBudget {
        &self.local.budget
    }

    pub fn metrics(&self) -> &Metrics {
        self.local.budget.metrics()
    }

    pub fn stat(&self, key: ObjectKey) -> Result<ObjectMetadata, ObjectStoreError> {
        let slot = self.catalog.read().get(&key).cloned().ok_or_else(|| unknown(key))?;
        let state = slot.state.lock();
        state.object.as_ref().map(Into::into).ok_or_else(|| unknown(key))
    }

    pub fn contains(&self, key: ObjectKey) -> bool {
        let maybe_slot = self.catalog.read().get(&key).cloned();
        maybe_slot.is_some_and(|slot| slot.state.lock().object.is_some())
    }

    pub fn write(&self, object: Object, retention: impl Into<DiskRetention>) -> Result<DiskObject, ObjectStoreError> {
        let size = object.size();
        let (key, bytes) = object.into_kv();
        assert_eq!(size, bytes.len() as u64);

        let pins = match retention.into() {
            DiskRetention::Evictable => 0,
            DiskRetention::Required => 1,
        };

        // Establish a [DiskObject] owning the object.
        //
        // The following locking protocol applies:
        //
        // 1. Lock the catalog and:
        //    - get or create the entry slot
        //    - increment the slot's `registering` counter (lock-free)
        //    - release the catalog lock
        // 2. Lock the entry slot and:
        //    - establish local pinning requirements
        //    - decrement the `registering` counter
        //    - release the entry slot
        // 3. Move the slot into the `DiskObject` owner
        //
        // Concurrent reclamation can't proceed during (1), and will skip the
        // slot before its lock can be acquired in (2).
        // Waiting for the slot lock doesn't require holding the catalog lock.
        // Concurrent eviction could remove the object if it gets the slot lock
        // first, but will skip it if we installed local pins in (2).
        let owner = {
            let slot = {
                let mut catalog = self.catalog.write();
                let slot = catalog.entry(key).or_default().clone();
                // Relaxed ordering is sufficient as we hold the catalog lock.
                let prev = slot.registering.fetch_add(1, Ordering::Relaxed);
                assert_ne!(prev, u64::MAX, "registration count overflow");
                slot
            };
            {
                let mut state = slot.state.lock();
                state.ownership.retain(pins);

                // Relaxed ordering is sufficient as we hold the slot lock.
                let prev = slot.registering.fetch_sub(1, Ordering::Relaxed);
                assert!(prev > 0);
            }

            DiskObject {
                key,
                slot,
                pins,
                should_reclaim: self.should_reclaim.clone(),
            }
        };

        let slot = &owner.slot;
        let mut state = slot.state.lock();
        let exists = {
            // If a permit exists, the object was written through this storage
            // instance. Therefore it definitely exists. Storage faults are
            // deferred to reads and evictions.
            match state.object.as_mut().filter(|entry| entry.permit.is_some()) {
                Some(entry) => {
                    entry.last_access_epoch = self.access_epoch();
                    true
                }
                None => false,
            }
        };
        if !exists {
            let permit = self.acquire_local_permit(size)?;
            let written_key = self.local.object_store.write(object_from_trusted_bytes(key, &bytes))?;
            assert_eq!(written_key, key);

            state.object = Some(ObjectCatalogEntry {
                size,
                last_access_epoch: self.access_epoch(),
                permit: Some(permit),
            });
        }
        drop(state);

        Ok(owner)
    }

    pub fn read(&self, key: ObjectKey) -> Result<ObjectBytes, ObjectStoreError> {
        let mut source = None;
        self.read_with_source(key, &mut source)
    }

    pub(super) fn read_with_source(
        &self,
        key: ObjectKey,
        source: &mut Option<FaultSource>,
    ) -> Result<ObjectBytes, ObjectStoreError> {
        let slot = self.catalog.read().get(&key).cloned().ok_or_else(|| unknown(key))?;
        // Try to read from local storage first.
        {
            let mut state = slot.state.lock();
            let entry = state.object.as_mut().ok_or_else(|| unknown(key))?;
            let local = self.local.object_store.read(key)?;
            self.metrics()
                .lookup(key.metric_type(), CacheTier::Disk, local.is_some());
            if let Some(object) = local {
                *source = Some(FaultSource::Disk);
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

            let mut state = slot.state.lock();
            let entry = state.object.as_mut().ok_or_else(|| unknown(key))?;
            // Another reader or writer may have restored the object while we
            // reserved space.
            let local = self.local.object_store.read(key)?;
            self.metrics()
                .lookup(key.metric_type(), CacheTier::Disk, local.is_some());
            if let Some(object) = local {
                *source = Some(FaultSource::Disk);
                entry.last_access_epoch = self.access_epoch();
                return Ok(object);
            }

            // Fetch the remote object and store it locally.
            // Hold the entry lock to prevent eviction.
            let bytes = remote.object_store.read(key)?.ok_or_else(|| not_found(key))?;
            *source = Some(FaultSource::Remote);
            self.local.object_store.write(object_from_trusted_bytes(key, &bytes))?;
            entry.last_access_epoch = self.access_epoch();
            entry.permit = Some(permit);

            return Ok(bytes);
        }

        // Entry is in the catalog, but neither in local nor remote storage.
        // TODO: panic?
        Err(not_found(key))
    }

    /// Trim the local storage to the configured low watermark.
    ///
    /// See also: [Self::evict].
    pub fn trim(&self) -> Result<u64, ObjectStoreError> {
        self.evict(self.local.budget.config().low_water_bytes)
    }

    /// Trim the local storage until the accounted byte usage is at or below
    /// `target_usage`.
    ///
    /// Tries to reclaim orphaned objects, then the least recently used live
    /// objects. If no remote storage is configured, no live objects are
    /// evicted.
    ///
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
        let timer = self.metrics().timer();
        let result = self.evict_inner(target_usage);
        self.metrics().eviction(EvictionDirection::DiskToRemote, timer.nanos());
        result
    }

    fn evict_inner(&self, target_usage: u64) -> Result<u64, ObjectStoreError> {
        // Reclaim orphaned objects.
        let mut bytes_freed = self.reclaim()?;
        // If we don't have remote storage, we can't evict under budget
        // pressure.
        let Some(remote) = &self.remote else {
            return Ok(bytes_freed);
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
                    // If the slot is locked, don't evict it.
                    let state = slot.state.try_lock()?;
                    // Nor if the object is pinned.
                    if state.ownership.is_pinned() {
                        return None;
                    }
                    // Filter out empty or non-resident slots.
                    let entry = state.object.as_ref()?;
                    entry.permit.as_ref()?;

                    entry.last_access_epoch
                };
                Some((last_access_epoch, key, slot))
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable_by_key(|(last_access_epoch, ..)| *last_access_epoch);

        for (_, key, slot) in candidates {
            // Skip if the slot was locked meanwhile.
            let Some(mut state) = slot.state.try_lock() else {
                continue;
            };
            // Skip if the object got pinned meanwhile.
            if state.ownership.is_pinned() {
                continue;
            }
            // Stop if we already reached our target.
            if self.local.budget.usage().accounted_bytes <= target_usage {
                break;
            }
            // Skip if the object is no longer present.
            let Some(entry) = state.object.as_mut().filter(|entry| entry.permit.is_some()) else {
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
            self.metrics()
                .evicted(key.metric_type(), EvictionDirection::DiskToRemote, entry.size);
        }

        Ok(bytes_freed)
    }

    /// Reclaim storage of orphaned objects.
    ///
    /// Cheap to call when there are no objects pending reclamation.
    /// This does **not** [evict][Self::evict] live objects.
    ///
    /// Returns the total number of bytes reclaimed.
    pub fn reclaim(&self) -> Result<u64, ObjectStoreError> {
        if !self.should_reclaim.swap(false, Ordering::AcqRel) {
            return Ok(0);
        }

        self.reclaim_inner()
            .inspect_err(|_| self.should_reclaim.store(true, Ordering::Release))
    }

    fn reclaim_inner(&self) -> Result<u64, ObjectStoreError> {
        let mut catalog = self.catalog.write();
        let mut slots = catalog
            .iter()
            .filter_map(|(key, slot)| {
                if slot.registering.load(Ordering::Acquire) == 0
                    && let Some(state) = slot.state.try_lock()
                {
                    matches!(state.ownership, ObjectOwnership::Unreferenced).then_some((*key, slot.clone()))
                } else {
                    // If we skipped an object, register a retry.
                    self.should_reclaim.store(true, Ordering::Release);
                    None
                }
            })
            .collect::<Vec<_>>();
        // Impose a stable removal order for fault injection.
        slots.sort_unstable_by_key(|(key, _)| (matches!(key, ObjectKey::Blob(_)), *key.as_bytes()));

        let mut freed = 0;
        for (key, slot) in slots {
            let mut entry = slot.state.lock();
            assert_eq!(slot.registering.load(Ordering::Acquire), 0);
            assert!(matches!(entry.ownership, ObjectOwnership::Unreferenced));

            // SAFETY: No owner needs the bytes. The catalog lock prevents a new
            // owner from registering until removal completes.
            unsafe { self.local.object_store.remove(key)? };

            // Release accounted capacity only after the file was removed successfully.
            if let Some(object) = entry.object.take()
                && object.permit.is_some()
            {
                freed += object.size;
            }

            drop(entry);
            catalog.remove(&key);
        }

        Ok(freed)
    }

    fn access_epoch(&self) -> u64 {
        self.access_epoch.fetch_add(1, Ordering::Relaxed)
    }

    /// Acquire a [BudgetPermit] to store `size` bytes in the local tier.
    ///
    /// Tries once to evict objects when permit acquisition fails.
    fn acquire_local_permit(&self, size: u64) -> Result<BudgetPermit, ObjectStoreError> {
        self.local.budget.acquire(size).or_else(|_| {
            self.trim()?;
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
        let key = storage.write(object(b"aaaa"), DiskRetention::Evictable).unwrap().key();
        let slot = storage.catalog.read().get(&key).unwrap().clone();

        let evicting = storage.clone();
        let eviction = thread::spawn(move || evicting.evict(0));
        reached.recv_timeout(TIMEOUT).unwrap();

        let transition_is_locked = slot.state.try_lock().is_none();
        let reading = storage.clone();
        let reader = thread::spawn(move || {
            if overwrite {
                reading.write(object(b"aaaa"), DiskRetention::Evictable)?;
            }
            reading.read(key)
        });
        // Unrelated keys can still be written while the eviction's I/O is paused.
        let other_object_ref = storage.write(object(b"bbbb"), DiskRetention::Evictable).unwrap();
        resume.send(()).unwrap();

        assert!(transition_is_locked);
        assert_eq!(eviction.join().unwrap().unwrap(), 4);
        assert_eq!(&*reader.join().unwrap().unwrap(), b"aaaa");
        assert!(local.stat(key).unwrap().is_some());
        assert!(local.stat(other_object_ref.key()).unwrap().is_some());
        assert_eq!(storage.local.budget.usage().accounted_bytes, 8);
        assert!(Arc::ptr_eq(&slot, storage.catalog.read().get(&key).unwrap()));
        assert!(slot.state.lock().object.as_ref().unwrap().permit.is_some());
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
        let object_ref = storage.write(object(b"aaaa"), DiskRetention::Evictable).unwrap();

        let first_storage = storage.clone();
        let first = thread::spawn(move || first_storage.evict(0));
        reached.recv_timeout(TIMEOUT).unwrap();

        let second_storage = storage.clone();
        let second = thread::spawn(move || second_storage.evict(0));
        resume.send(()).unwrap();

        assert_eq!(first.join().unwrap().unwrap() + second.join().unwrap().unwrap(), 4);
        assert_eq!(storage.local.budget.usage().accounted_bytes, 0);
        assert!(storage.contains(object_ref.key()));
    }

    #[test]
    fn remote_read_can_evict_without_holding_its_slot() {
        // Budget to fit exactly one object.
        let budget = ByteBudget::new(ByteBudgetConfig {
            tier: CacheTier::Disk,
            low_water_bytes: 0,
            soft_limit_bytes: 4,
            hard_limit_bytes: 4,
        })
        .unwrap();
        let storage = storage(Arc::default(), Arc::default(), budget);

        let object_ref = storage.write(object(b"aaaa"), DiskRetention::Evictable).unwrap();
        storage.write(object(b"bbbb"), DiskRetention::Evictable).unwrap();

        let (sender, receiver) = mpsc::sync_channel(1);
        let reading = storage.clone();
        let reader = thread::spawn(move || sender.send(reading.read(object_ref.key())).unwrap());
        assert_eq!(&*receiver.recv_timeout(TIMEOUT).unwrap().unwrap(), b"aaaa");

        reader.join().unwrap();
        assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
    }

    #[test]
    fn overwrites_preserve_the_slot_and_account_once() {
        let storage = storage(Arc::default(), Arc::default(), ByteBudget::unlimited());

        let key = storage.write(object(b"aaaa"), DiskRetention::Evictable).unwrap().key();
        let slot = storage.catalog.read().get(&key).unwrap().clone();
        assert_eq!(
            storage.write(object(b"aaaa"), DiskRetention::Evictable).unwrap().key(),
            key
        );

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
        assert!(storage.write(object(b"aaaa"), DiskRetention::Evictable).is_err());
        let slot = storage.catalog.read().get(&key).unwrap().clone();

        assert!(!storage.contains(key));
        assert!(matches!(storage.stat(key), Err(ObjectStoreError::UnknownObject(_))));
        assert!(matches!(storage.read(key), Err(ObjectStoreError::UnknownObject(_))));
        assert_eq!(storage.local.budget.usage().accounted_bytes, 0);
        assert_eq!(storage.evict(0).unwrap(), 0);

        // Retrying publishes the object through the existing mutex, so callers
        // that already obtained the slot cannot end up locking a different one.
        local.fail_writes.store(false, Ordering::Relaxed);
        assert_eq!(
            storage.write(object(b"aaaa"), DiskRetention::Evictable).unwrap().key(),
            key
        );
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

        let key = storage.write(object(b"aaaa"), DiskRetention::Evictable).unwrap().key();

        assert!(storage.evict(0).is_err());
        assert!(local.stat(key).unwrap().is_some());
        assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
        assert_eq!(&*storage.read(key).unwrap(), b"aaaa");
    }

    #[test]
    fn eviction_does_nothing_when_target_is_met() {
        let local = Arc::new(TestStore::default());
        let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());

        let key = storage.write(object(b"aaaa"), DiskRetention::Evictable).unwrap().key();

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
        let key = storage.write(object(b"aaaa"), DiskRetention::Evictable).unwrap().key();

        assert!(storage.evict(0).is_err());
        assert!(local.stat(key).unwrap().is_some());
        assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
        assert_eq!(&*storage.read(key).unwrap(), b"aaaa");
    }
}
