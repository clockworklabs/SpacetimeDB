use super::metrics::{CacheTier, EvictionDirection, ObjectType};
use std::{
    num::NonZeroUsize,
    ops::Deref,
    sync::{Arc, Mutex},
};

use parking_lot::RwLock;
use spacetimedb_data_structures::{
    map::{hash_map, IntMap},
    small_map::SmallHashMap,
};
use spacetimedb_memory_usage::MemoryUsage;
use spacetimedb_sats::layout::RowTypeLayout;

use crate::{
    blob_store::BlobHash,
    indexes::PageOffset,
    page::Page,
    row_type_visitor::VarLenVisitorProgram,
    tiered::{
        error::{BlobError, ObjectStoreError},
        map::{BlobHashWrapper, ObjectKey},
        mem::{self, BlobEvictionCandidate, BlobFrame, ReservedBlob},
        storage::{DiskObject, DiskRetention, Object},
        ByteBudget, ByteBudgetUsage, EvictionPolicy, TieredStorage,
    },
    var_len::VarLenMembers,
};

/// A blob set contains the content-addressed blobs, shared across all tables in
/// a database.
///
/// Residency of blobs is controlled by the shared [mem::Cache]'s budget. Blobs
/// that get evicted from memory are handed off to the shared [TieredStorage].
#[derive(Debug)]
pub struct BlobSet {
    resident: Arc<mem::ResidentObjects>,
    storage: Arc<TieredStorage>,
    entries: RwLock<IntMap<BlobHashWrapper, BlobEntry>>,
    /// Scratch allocation for collecting eviction candidates.
    evictable: Mutex<Vec<BlobEvictionCandidate>>,
}

impl BlobSet {
    /// Create a new, empty [BlobSet].
    pub fn new(resident: Arc<mem::ResidentObjects>, storage: Arc<TieredStorage>) -> Self {
        Self {
            resident,
            storage,
            entries: <_>::default(),
            evictable: <_>::default(),
        }
    }

    /// Create a new [BlobSet] from externally loaded blobs.
    ///
    /// This is used when restoring from a snapshot.
    ///
    /// All blobs in the map will initially be resident, bypassing the memory budget.
    pub fn from_blob_map(resident: Arc<mem::ResidentObjects>, storage: Arc<TieredStorage>, map: BlobMap) -> Self {
        let mut entries = IntMap::default();
        for (key, BlobMapEntry { uses, blob }) in map.inner {
            let frame = resident.register_blob_unchecked(blob);
            entries.insert(
                key,
                BlobEntry::Resident {
                    uses: uses.get(),
                    pins: 0,
                    blob: BlobHandle { frame },
                    backing: None,
                    disk_pins: 0,
                },
            );
        }

        Self {
            resident,
            storage,
            entries: RwLock::new(entries),
            evictable: <_>::default(),
        }
    }

    pub fn new_for_test() -> Self {
        Self::new(
            mem::ResidentObjects::new(ByteBudget::unlimited()).into(),
            TieredStorage::new_for_test().into(),
        )
    }

    /// Attempt to evict blobs from this set until the budget's low watermark is
    /// met.
    ///
    /// See also: [Self::evict].
    pub fn trim(&self) -> Result<u64, BlobError> {
        self.evict(self.resident.budget().config().low_water_bytes)
    }

    /// Attempt to evict resident blobs in this set that have no external
    /// references, until `target_usage` is met.
    ///
    /// Eviction will stop once the usage is at or below `target_usage`. No
    /// blobs may, however, be eligible for eviction. As the budget is shared,
    /// resident pages may also prevent the usage to meet the target.
    ///
    /// Returns the number of bytes freed.
    pub fn evict(&self, target_usage: u64) -> Result<u64, BlobError> {
        let timer = self.resident.metrics().timer();
        let result = self.evict_inner(target_usage);
        self.resident
            .metrics()
            .eviction(EvictionDirection::MemoryToDisk, timer.nanos());
        result
    }

    fn evict_inner(&self, target_usage: u64) -> Result<u64, BlobError> {
        let mut bytes_freed = 0;

        if self.resident.usage().accounted_bytes() <= target_usage {
            return Ok(bytes_freed);
        }

        let mut candidates = self.evictable.lock().unwrap();
        self.resident.evictable_blobs(&mut candidates);
        for candidate in candidates.drain(..) {
            let maybe_resident = self.entries.read().get(&candidate.key).and_then(|entry| match entry {
                BlobEntry::Resident { blob, pins, .. } => (pins == &0).then(|| blob.clone()),
                BlobEntry::NonResident { .. } => None,
            });

            if let Some(blob) = maybe_resident {
                let mut backing = self.storage.write(Object::from(&blob), DiskRetention::Required)?;
                assert_eq!(
                    backing.key().as_blob_hash().expect("storage key to be a blob hash"),
                    &candidate.key
                );

                let mut entries = self.entries.write();
                let Some(entry) = entries.get_mut(&candidate.key) else {
                    continue;
                };
                let BlobEntry::Resident {
                    uses,
                    blob: current,
                    pins,
                    disk_pins,
                    ..
                } = entry
                else {
                    continue;
                };
                // Skip pinned entries.
                if *pins > 0 {
                    continue;
                }
                // Eviction may proceed concurrent to blob set access, in
                // particular we're not holding an entries lock while writing to
                // the tiered storage. Therefore, ensure that the entry is still
                // the same allocation.
                //
                // An evictable frame must have exactly two references: one in
                // `entries` + the clone of `blob` that was done above.
                if !Arc::ptr_eq(&blob.frame, &current.frame) || Arc::strong_count(&blob.frame) != 2 {
                    continue;
                };

                let size = blob.size();
                backing.set_pins(*disk_pins);
                *entry = BlobEntry::NonResident {
                    uses: *uses,
                    size,
                    backing,
                };
                self.resident
                    .metrics()
                    .evicted(ObjectType::Blob, EvictionDirection::MemoryToDisk, size);
                drop(blob);
                self.resident.evict_blob(candidate);
                drop(entries);
                bytes_freed += size;

                if self.resident.usage().accounted_bytes() <= target_usage {
                    break;
                }
            }
        }

        Ok(bytes_freed)
    }

    #[inline]
    pub fn resident_usage(&self) -> ByteBudgetUsage {
        self.resident.usage()
    }

    fn reserve(&self, hash: BlobHashWrapper, bytes: &[u8]) -> Result<ReservedBlob, BlobError> {
        debug_assert_eq!(hash, BlobHashWrapper::hash_from_bytes(bytes));
        let size = bytes.len() as u64;
        self.resident.reserve_blob(size, hash, || bytes.into()).or_else(|_| {
            self.trim()?;
            self.resident
                .reserve_blob(size, hash, || bytes.into())
                .map_err(BlobError::from)
        })
    }

    /// Insert `bytes` into this set, making it a resident blob.
    ///
    /// Subject to the memory budget.
    pub fn insert(&self, eviction_policy: EvictionPolicy, bytes: &[u8]) -> Result<BlobHandle, BlobError> {
        self.insert_with_uses(NonZeroUsize::MIN, eviction_policy, bytes)
    }

    fn insert_with_uses(
        &self,
        uses: NonZeroUsize,
        eviction_policy: EvictionPolicy,
        bytes: &[u8],
    ) -> Result<BlobHandle, BlobError> {
        let hash = BlobHashWrapper::hash_from_bytes(bytes);
        // Memory pins -- disk pins are handled below.
        let pins = match eviction_policy {
            EvictionPolicy::Never => 1,
            EvictionPolicy::Memory | EvictionPolicy::Disk => 0,
        };
        {
            let mut entries = self.entries.write();
            if let Some(BlobEntry::Resident {
                uses: current_uses,
                pins: current_pins,
                blob,
                backing,
                disk_pins,
            }) = entries.get_mut(&hash)
            {
                *current_uses += uses.get();
                *current_pins += pins;

                if DiskRetention::is_required(eviction_policy) {
                    *disk_pins = disk_pins.checked_add(1).expect("disk pin count overflow");
                    if let Some(backing) = backing {
                        backing.pin();
                    }
                }

                return Ok(blob.clone());
            }
        }
        // Entry needs to be faulted or created. Reserve before modifying the
        // catalog. Note that this may trigger eviction, so we must release the
        // lock.
        let reservation = self.reserve(hash, bytes)?;
        let mut entries = self.entries.write();
        match entries.entry(hash) {
            hash_map::Entry::Vacant(entry) => {
                let frame = self.resident.redeem_blob(reservation);
                let handle = BlobHandle { frame };
                entry.insert(BlobEntry::Resident {
                    uses: uses.get(),
                    pins,
                    blob: handle.clone(),
                    backing: None,
                    disk_pins: DiskRetention::is_required(eviction_policy).into(),
                });
                Ok(handle)
            }
            hash_map::Entry::Occupied(mut entry) => match entry.get_mut() {
                // Entry became resident while we didn't hold the entries lock.
                // Drop the reservation and update the entry's refcount.
                BlobEntry::Resident {
                    uses: current_uses,
                    pins: current_pins,
                    blob,
                    backing,
                    disk_pins,
                } => {
                    *current_uses += uses.get();
                    *current_pins += pins;

                    if DiskRetention::is_required(eviction_policy) {
                        *disk_pins = disk_pins.checked_add(1).expect("disk pin count overflow");
                        if let Some(backing) = backing {
                            backing.pin();
                        }
                    }

                    Ok(blob.clone())
                }
                BlobEntry::NonResident {
                    uses: current_uses,
                    backing,
                    ..
                } => {
                    let new_uses = *current_uses + uses.get();
                    let frame = self.resident.redeem_blob(reservation);
                    let handle = BlobHandle { frame };

                    let mut backing = backing.clone();
                    let mut disk_pins = backing.pins();
                    if DiskRetention::is_required(eviction_policy) {
                        disk_pins = disk_pins.checked_add(1).expect("disk pin count overflow");
                        backing.pin();
                    }

                    entry.insert(BlobEntry::Resident {
                        uses: new_uses,
                        blob: handle.clone(),
                        pins,
                        backing: Some(backing),
                        disk_pins,
                    });

                    Ok(handle)
                }
            },
        }
    }

    /// Increment the reference count of the blob with content address `hash`.
    ///
    /// This does not fault the blob if it is non-resident.
    ///
    /// Returns [BlobError::MissingBlob] if no such blob is in this blob set's
    /// entry catalog.
    pub fn clone_ref(&mut self, hash: BlobHashWrapper) -> Result<(), BlobError> {
        match self.entries.write().entry(hash) {
            hash_map::Entry::Occupied(mut entry) => {
                entry.get_mut().inc_uses();
                Ok(())
            }
            hash_map::Entry::Vacant(_) => Err(BlobError::MissingBlob(Box::new(hash.into()))),
        }
    }

    /// Decrement the reference count of the blob with content address `hash`.
    ///
    /// If the refcount is zero after decrementing, the entry is removed from
    /// the entry catalog.
    ///
    /// If `eviction_policy` is [EvictionPolicy::NeverEvict], the pin count for
    /// the blob is decremented, too. The caller must ensure that the same
    /// policy is used for creating and freeing the blob -- i.e. the eviction
    /// policy of the table containing the blob's row.
    ///
    /// Returns [BlobError::MissingBlob] if no such blob is in this blob set's
    /// entry catalog.
    ///
    /// On success returns the size in bytes of the blob.
    pub fn free_ref(&self, hash: BlobHashWrapper, eviction_policy: EvictionPolicy) -> Result<u64, BlobError> {
        let mut entries = self.entries.write();
        match entries.entry(hash) {
            hash_map::Entry::Vacant(_) => Err(BlobError::MissingBlob(Box::new(hash.into()))),
            hash_map::Entry::Occupied(mut entry) => {
                {
                    let blob_entry = entry.get_mut();
                    blob_entry.dec_uses();
                    match eviction_policy {
                        EvictionPolicy::Never => blob_entry.unpin_mem(),
                        EvictionPolicy::Memory => blob_entry.unpin_disk(),
                        EvictionPolicy::Disk => {}
                    }
                }
                let blob_entry = entry.get();
                let size = blob_entry.size();
                if blob_entry.uses() == 0 {
                    entry.remove();
                }

                Ok(size)
            }
        }
    }

    /// Get a handle to the blob with content address `hash`, faulting it in if
    /// it is not resident.
    ///
    /// Subject to the memory budget. Note that the blob will remain resident as
    /// long as the returned [BlobHandle] is live.
    pub fn get(&self, hash: BlobHashWrapper) -> Result<BlobHandle, BlobError> {
        let mut fault = None;
        let blob_or_bytes = {
            let entries = self.entries.read();
            let entry = entries
                .get(&hash)
                .ok_or_else(|| BlobError::MissingBlob(Box::new(hash.into())))?;
            self.resident.metrics().lookup(
                ObjectType::Blob,
                CacheTier::Memory,
                matches!(entry, BlobEntry::Resident { .. }),
            );
            match entry {
                BlobEntry::Resident { blob, .. } => Ok::<_, BlobError>(Ok(blob.clone())),
                BlobEntry::NonResident { .. } => {
                    let fault_guard = fault.insert(self.resident.metrics().fault(ObjectType::Blob));
                    let bytes = self.storage.read_with_source(hash.into(), &mut fault_guard.source)?;
                    Ok(Err(bytes))
                }
            }
        }?;
        let handle = match blob_or_bytes {
            Ok(blob) => Ok::<_, BlobError>(blob),
            Err(bytes) => {
                let reservation = self.reserve(hash, &bytes)?;
                let mut entries = self.entries.write();
                let entry = entries
                    .get_mut(&hash)
                    .ok_or_else(|| BlobError::MissingBlob(Box::new(hash.into())))?;
                match entry {
                    BlobEntry::Resident { blob, .. } => Ok(blob.clone()),
                    BlobEntry::NonResident { uses, backing, .. } => {
                        let frame = self.resident.redeem_blob(reservation);
                        let handle = BlobHandle { frame };
                        *entry = BlobEntry::Resident {
                            uses: *uses,
                            pins: 0,
                            blob: handle.clone(),
                            disk_pins: backing.pins(),
                            backing: Some(backing.clone()),
                        };
                        Ok(handle)
                    }
                }
            }
        }?;
        assert_eq!(hash, handle.hash());
        self.resident.touch_blob(&handle.frame);
        if let Some(fault) = &mut fault {
            fault.success = true;
        }

        Ok(handle)
    }

    /// Fault all blobs in this set and return their pin handles.
    ///
    /// This is intended to be used on a transaction's blob set during
    /// preparation, not the entire database's blob storage.
    ///
    /// Subject to the resident memory budget.
    pub fn fault_all(&self) -> Result<Box<[BlobHandle]>, BlobError> {
        let hashes: Box<[_]> = self
            .entries
            .read()
            .iter()
            .filter_map(|(&hash, entry)| (entry.uses() > 0).then_some(hash))
            .collect();
        hashes.into_iter().map(|hash| self.get(hash)).collect()
    }

    /// Fault the blob at `hash` and install a [EvictionPolicy::Never]
    /// requirement.
    ///
    /// This protects resident bytes; if the object is present in local backing
    /// storage, it remains evictable there.
    ///
    /// Must be called for every reference to the blob, without deduplication.
    ///
    /// Faulting is subject to the resident memory budget.
    ///
    /// The blob must be in this blob set, otherwise a [BlobError::MissingBlob]
    /// is returned.
    pub fn pin(&self, hash: BlobHashWrapper) -> Result<(), BlobError> {
        self.get(hash).and_then(|handle| self.pin_handle(&handle))
    }

    /// Install a [EvictionPolicy::Never] for the supplied blob.
    ///
    /// This protects resident bytes; if the object is present in local backing
    /// storage, it remains evictable there.
    ///
    /// Must be called for every reference to the blob, without deduplication.
    ///
    /// The blob must be in this blob set, otherwise a [BlobError::MissingBlob]
    /// is returned.
    pub fn pin_handle(&self, handle: &BlobHandle) -> Result<(), BlobError> {
        let mut entries = self.entries.write();
        let entry = entries
            .get_mut(&handle.hash())
            .ok_or_else(|| BlobError::MissingBlob(Box::new(handle.hash().into())))?;
        let BlobEntry::Resident { pins, .. } = entry else {
            unreachable!("blob must be resident because its handle is in scope")
        };
        *pins += 1;

        Ok(())
    }

    /// Release one in-memory row reference's protection against eviction.
    /// The blob's logical reference count is unchanged.
    pub(crate) fn unpin(&self, hash: BlobHashWrapper) {
        let mut entries = self.entries.write();
        let Some(BlobEntry::Resident { pins, .. }) = entries.get_mut(&hash) else {
            unreachable!("a policy pin keeps the blob resident")
        };
        assert!(*pins > 0, "each in-memory reference owns a policy pin");
        *pins -= 1;
    }

    /// Prevent the object at `hash` from being evicted from local backing
    /// storage.
    ///
    /// Must be called for every reference to the blob, without deduplication.
    pub(crate) fn pin_disk(&self, hash: BlobHashWrapper) {
        self.entries
            .write()
            .get_mut(&hash)
            .expect("blob to disk-pin must exist")
            .pin_disk()
    }

    /// Release one object's local backing storage pin.
    pub(crate) fn unpin_disk(&self, hash: BlobHashWrapper) {
        self.entries
            .write()
            .get_mut(&hash)
            .expect("blob to disk-pin must exist")
            .unpin_disk();
    }

    /// Apply a fallible operation to each blob in this set.
    ///
    /// Note that non-resident blobs will not be faulted in, but read directly
    /// from tiered storage and passed to the closure.
    //
    // TODO(disk-tables): This is a temporary measure to satisfy the legacy bulk
    // snapshotting. In particular, it will read pages from tiered storage,
    // without budget accounting, nor an opportunity for optimizations such as
    // hardlinking.
    pub fn try_for_each<E: From<ObjectStoreError>>(
        &self,
        mut f: impl FnMut(BlobHashWrapper, NonZeroUsize, &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        let entries = self.entries.read();
        for (&hash, entry) in entries.iter().filter(|(_, entry)| entry.uses() > 0) {
            match entry {
                BlobEntry::Resident { uses, blob, .. } => {
                    f(hash, NonZeroUsize::new(*uses).unwrap(), blob)?;
                }
                BlobEntry::NonResident { uses, .. } => {
                    let bytes = self.storage.read(hash.into())?;
                    f(hash, NonZeroUsize::new(*uses).unwrap(), &bytes)?;
                }
            }
        }

        Ok(())
    }

    /// Sums size + hash bytes of each resident blob in the set.
    pub fn resident_bytes_used_by_blobs(&self) -> u64 {
        self.entries
            .read()
            .iter()
            .filter_map(|(_, entry)| {
                if let BlobEntry::Resident { blob, .. } = entry {
                    Some(blob.size() + (ObjectKey::SIZE as u64))
                } else {
                    None
                }
            })
            .sum()
    }

    /// Sums size * uses of each blob in the set.
    pub fn bytes_used_by_blobs(&self) -> u64 {
        self.entries
            .read()
            .iter()
            .map(|(_, entry)| entry.size() * entry.uses() as u64)
            .sum()
    }

    /// The number of blobs in the set's entry catalog.
    pub fn num_blobs(&self) -> usize {
        self.entries.read().len()
    }

    /// The number of references a blob has,
    /// or `None` if the blob is not in the set.
    pub fn uses(&self, hash: BlobHashWrapper) -> Option<usize> {
        Some(self.entries.read().get(&hash)?.uses())
    }
}

#[cfg(test)]
impl BlobSet {
    pub fn usage_counter(&self) -> spacetimedb_data_structures::map::HashMap<BlobHashWrapper, usize> {
        self.entries
            .read()
            .iter()
            .map(|(hash, entry)| (*hash, entry.uses()))
            .collect()
    }

    pub(crate) fn keys(&self) -> Vec<BlobHashWrapper> {
        self.entries.read().keys().copied().collect()
    }
}

impl MemoryUsage for BlobSet {
    fn heap_usage(&self) -> usize {
        let arc = std::mem::size_of::<usize>() * 2;
        (2 * arc)
            + std::mem::size_of::<mem::ResidentObjects>()
            + std::mem::size_of::<TieredStorage>()
            + std::mem::size_of::<Mutex<Vec<BlobEvictionCandidate>>>()
            + (self.resident_bytes_used_by_blobs() as usize)
    }
}

#[derive(Debug)]
enum BlobEntry {
    Resident {
        uses: usize,
        pins: usize,
        blob: BlobHandle,
        /// `Some` if the entry has been written to local disk.
        backing: Option<DiskObject>,
        /// Disk retention requirements exist before any backing object has been
        /// written. When `backing` is `Some`, its local pin contribution
        /// matches this value.
        disk_pins: u64,
    },
    #[allow(unused)]
    NonResident {
        uses: usize,
        size: u64,
        backing: DiskObject,
    },
}

impl BlobEntry {
    fn inc_uses(&mut self) {
        self.mod_uses(|uses| *uses += 1);
    }

    fn dec_uses(&mut self) {
        self.mod_uses(|uses| *uses = uses.saturating_sub(1));
    }

    fn mod_uses(&mut self, f: impl FnOnce(&mut usize)) {
        match self {
            Self::Resident { uses, .. } | Self::NonResident { uses, .. } => f(uses),
        }
    }

    fn uses(&self) -> usize {
        match self {
            Self::Resident { uses, .. } | Self::NonResident { uses, .. } => *uses,
        }
    }

    fn size(&self) -> u64 {
        match self {
            Self::NonResident { size, .. } => *size,
            Self::Resident { blob, .. } => blob.size(),
        }
    }

    fn unpin_mem(&mut self) {
        match self {
            Self::NonResident { .. } => {
                unreachable!("attempt to decrement pin count on non-resident blob")
            }
            Self::Resident { pins, .. } if *pins == 0 => {
                panic!("attempt to decrement pin count already at zero")
            }
            Self::Resident { pins, .. } => {
                *pins -= 1;
            }
        }
    }

    fn pin_disk(&mut self) {
        match self {
            Self::Resident { backing, disk_pins, .. } => {
                *disk_pins = disk_pins.checked_add(1).expect("disk pin count overflow");
                if let Some(backing) = backing {
                    backing.pin();
                }
            }
            Self::NonResident { backing, .. } => backing.pin(),
        }
    }

    fn unpin_disk(&mut self) {
        match self {
            Self::Resident { backing, disk_pins, .. } => {
                *disk_pins = disk_pins
                    .checked_sub(1)
                    .expect("attempt to decrement disk pin count already at zero");
                if let Some(backing) = backing {
                    backing.unpin();
                }
            }
            Self::NonResident { backing, .. } => backing.unpin(),
        }
    }
}

/// A handle to a blob.
///
/// As long as a handle to a blob is live, the blob is guaranteed to be
/// resident in main memory.
#[derive(Clone, Debug)]
pub struct BlobHandle {
    frame: Arc<BlobFrame>,
}

impl BlobHandle {
    pub fn hash(&self) -> BlobHashWrapper {
        self.frame.hash()
    }

    pub fn size(&self) -> u64 {
        self.frame.size()
    }
}

impl Deref for BlobHandle {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

/// A set of resident blobs for a specific row.
///
/// Essentially just a set of [BlobHandle]s that guarantees that those blobs are
/// resident. Used to perform operations such as equality checking that cannot
/// be fallible due to faults.
#[derive(Clone, Default)]
pub struct BlobReadSet {
    // Rows without external blobs need no Arc allocation or reference counting.
    // Non-empty sets share their pins when a RowRef is cloned.
    inner: Option<Arc<SmallHashMap<BlobHashWrapper, BlobHandle, 4, 16>>>,
}

impl BlobReadSet {
    /// Create a read set that contains all blobs referenced by `page`.
    pub fn new(
        visitor: &VarLenVisitorProgram,
        page: &Page,
        blobs: &BlobSet,
        layout: &RowTypeLayout,
        offset: PageOffset,
    ) -> Result<Self, BlobError> {
        let row_data = page.get_row_data(offset, layout.size());
        let mut inner = SmallHashMap::default();
        for vlr in unsafe { visitor.visit_var_len(row_data) }.filter(|vlr| vlr.is_large_blob()) {
            let granule = unsafe { page.iter_var_len_object(vlr.first_granule) }.next().unwrap();
            let blob_hash = granule.blob_hash().into();
            let blob = blobs.get(blob_hash)?;
            inner.insert(blob_hash, blob);
        }

        Ok(Self {
            inner: (!inner.is_empty()).then(|| Arc::new(inner)),
        })
    }

    /// Create an empty read set.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Get the blob with content address `hash`, or `None` if it is not in the
    /// read set.
    pub fn get(&self, hash: &BlobHashWrapper) -> Option<&BlobHandle> {
        self.inner.as_ref()?.get(hash)
    }

    /// Iterator over all entries in the read set.
    pub fn iter(&self) -> impl Iterator<Item = (&BlobHashWrapper, &BlobHandle)> {
        self.inner.iter().flat_map(|inner| inner.iter())
    }
}

/// A map of resident blobs.
///
/// Used when restoring from a snapshot (see: [BlobSet::from_blob_map]).
#[derive(Default)]
pub struct BlobMap {
    inner: IntMap<BlobHashWrapper, BlobMapEntry>,
}

impl BlobMap {
    /// Insert a blob into the map. The caller must guarantee that the `hash` is
    /// equal to `BlobHash::hash_from_bytes(blob)`.
    pub fn insert_with_uses(&mut self, hash: &BlobHash, uses: NonZeroUsize, blob: Box<[u8]>) {
        debug_assert_eq!(hash, &BlobHash::hash_from_bytes(&blob));
        self.inner.insert((*hash).into(), BlobMapEntry { uses, blob });
    }

    pub fn iter(&self) -> impl Iterator<Item = (&BlobHashWrapper, &BlobMapEntry)> {
        self.into_iter()
    }
}

pub struct BlobMapEntry {
    pub uses: NonZeroUsize,
    pub blob: Box<[u8]>,
}

impl<'a> IntoIterator for &'a BlobMap {
    type Item = (&'a BlobHashWrapper, &'a BlobMapEntry);
    type IntoIter = <&'a IntMap<BlobHashWrapper, BlobMapEntry> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&self.inner).into_iter()
    }
}

impl IntoIterator for BlobMap {
    type Item = (BlobHashWrapper, BlobMapEntry);
    type IntoIter = <IntMap<BlobHashWrapper, BlobMapEntry> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use proptest::{collection::vec, prelude::*};

    use crate::{
        blob_store::BlobHash,
        tiered::{
            map::BlobHashWrapper, metrics::CacheTier, BlobError, BlobSet, ByteBudget, ByteBudgetConfig, EvictionPolicy,
            ResidentObjects, TieredStorage,
        },
    };

    proptest! {
        #[test]
        fn insertion_deduplicates_content_and_tracks_reference_counts(
            blobs in vec(vec(any::<u8>(), 0..1024), 0..100),
        ) {
            let set = BlobSet::new_for_test();
            let mut model = HashMap::<BlobHash, (Vec<u8>, usize)>::new();

            for bytes in &blobs {
                let handle = set.insert(EvictionPolicy::Memory, bytes).unwrap();
                let hash = BlobHash::hash_from_bytes(bytes);
                prop_assert_eq!(handle.hash(), hash.into());
                prop_assert_eq!(&*handle, bytes.as_slice());
                let entry = model.entry(hash).or_insert_with(|| (bytes.clone(), 0));
                entry.1 += 1;
            }

            prop_assert_eq!(set.num_blobs(), model.len());
            prop_assert_eq!(
                set.bytes_used_by_blobs(),
                model.values().map(|(bytes, uses)| bytes.len() as u64 * *uses as u64).sum::<u64>(),
            );
            prop_assert_eq!(
                set.resident_bytes_used_by_blobs(),
                model.values().map(|(bytes, _)| (bytes.len() + BlobHash::SIZE) as u64).sum::<u64>(),
            );
            let usage_counter = set.usage_counter();
            for (hash, (bytes, uses)) in &model {
                let hash = BlobHashWrapper::from(*hash);
                prop_assert_eq!(usage_counter.get(&hash), Some(uses));
                prop_assert_eq!(&*set.get(hash).unwrap(), bytes.as_slice());
            }

            for bytes in blobs {
                let hash = BlobHash::hash_from_bytes(&bytes);
                prop_assert_eq!(set.free_ref(hash.into(), EvictionPolicy::Memory).unwrap(), bytes.len() as u64);
            }
            prop_assert_eq!(set.num_blobs(), 0);
            prop_assert_eq!(set.bytes_used_by_blobs(), 0);
            prop_assert_eq!(set.resident_bytes_used_by_blobs(), 0);
        }

        #[test]
        fn eviction_preserves_catalog_and_faults_bytes_back_in(
            first in vec(any::<u8>(), 1..1024),
            second in vec(any::<u8>(), 1..1024),
        ) {
            let first_hash = BlobHashWrapper::from(BlobHash::hash_from_bytes(&first));
            let second_hash = BlobHashWrapper::from(BlobHash::hash_from_bytes(&second));
            prop_assume!(first_hash != second_hash);

            // Make the hardlimit fit only one blob.
            let hard_limit_bytes = first.len().max(second.len()) as u64;
            let budget = ByteBudget::new(ByteBudgetConfig {
                tier: CacheTier::Memory,
                low_water_bytes: 0,
                soft_limit_bytes: hard_limit_bytes,
                hard_limit_bytes,
            }).unwrap();
            let resident = Arc::new(ResidentObjects::new(budget));
            let storage = Arc::new(TieredStorage::new_for_test());
            let set = BlobSet::new(resident.clone(), storage);

            set.insert(EvictionPolicy::Memory, &first).map(drop).unwrap();
            // This should evict `first`, as there is no external handle.
            set.insert(EvictionPolicy::Memory, &second).map(drop).unwrap();

            prop_assert_eq!(set.num_blobs(), 2);
            // The catalog knows both entries, but only the second is resident
            // and therefore consumes budget bytes.
            prop_assert_eq!(resident.usage().accounted_bytes(), second.len() as u64);
            // Fault the first entry back in.
            prop_assert_eq!(&*set.get(first_hash).unwrap(), first.as_slice());
            prop_assert_eq!(resident.usage().accounted_bytes(), first.len() as u64);

            // The refcounts should be preserved irrespective of residency.
            let usage_counter = set.usage_counter();
            prop_assert_eq!(usage_counter.get(&first_hash), Some(&1));
            prop_assert_eq!(usage_counter.get(&second_hash), Some(&1));
        }

        #[test]
        fn blob_handle_prevents_eviction(
            first in vec(any::<u8>(), 1..1024),
            second in vec(any::<u8>(), 1..1024),
        ) {
            let first_hash = BlobHash::hash_from_bytes(&first);
            let second_hash = BlobHash::hash_from_bytes(&second);
            prop_assume!(first_hash != second_hash);

            // Make the hardlimit fit only one blob.
            let hard_limit_bytes = first.len().max(second.len()) as u64;
            let budget = ByteBudget::new(ByteBudgetConfig {
                tier: CacheTier::Memory,
                low_water_bytes: 0,
                soft_limit_bytes: hard_limit_bytes,
                hard_limit_bytes,
            }).unwrap();
            let resident = Arc::new(ResidentObjects::new(budget));
            let storage = Arc::new(TieredStorage::new_for_test());
            let set = BlobSet::new(resident.clone(), storage);

            let _first_handle = set.insert(EvictionPolicy::Memory, &first).unwrap();
            prop_assert!(matches!(set.insert(EvictionPolicy::Memory, &second), Err(BlobError::MemoryLimitExceeded(_))));
        }
    }

    #[test]
    fn preparing_blobs_consumes_budget_once() {
        let bytes = b"prepared blob";
        let budget = ByteBudget::new(ByteBudgetConfig {
            tier: CacheTier::Memory,
            low_water_bytes: 0,
            soft_limit_bytes: bytes.len() as u64,
            hard_limit_bytes: bytes.len() as u64,
        })
        .unwrap();
        let resident = Arc::new(ResidentObjects::new(budget));
        let storage = Arc::new(TieredStorage::new_for_test());

        let transaction = BlobSet::new(resident.clone(), storage.clone());
        let committed = BlobSet::new(resident.clone(), storage);

        // Emulate transaction prepare.
        transaction.insert(EvictionPolicy::Memory, bytes).map(drop).unwrap();
        let pins = transaction.fault_all().unwrap();

        // Emulate transaction commit.
        // The shared [ResidentObjects] notices that it's the same object, so
        // reuses the already-held budget permit.
        assert!(committed.usage_counter().is_empty());
        committed.insert(EvictionPolicy::Memory, bytes).map(drop).unwrap();

        assert_eq!(committed.usage_counter().get(&pins[0].hash()), Some(&1));
        assert_eq!(resident.usage().accounted_bytes(), bytes.len() as u64);
    }

    #[test]
    fn never_evict_policy_survives_deduplication_in_either_order() {
        for policies in [
            [EvictionPolicy::Never, EvictionPolicy::Memory],
            [EvictionPolicy::Memory, EvictionPolicy::Never],
        ] {
            let bytes = b"shared blob";
            let budget = ByteBudget::new(ByteBudgetConfig {
                tier: CacheTier::Memory,
                low_water_bytes: 0,
                soft_limit_bytes: bytes.len() as u64,
                hard_limit_bytes: bytes.len() as u64,
            })
            .unwrap();
            let resident = Arc::new(ResidentObjects::new(budget));
            let set = BlobSet::new(resident.clone(), Arc::new(TieredStorage::new_for_test()));
            let hash = BlobHashWrapper::hash_from_bytes(bytes);

            // Table order must not matter: either owner can introduce the blob
            // first, and an evictable owner must never downgrade its policy.
            for policy in policies {
                drop(set.insert(policy, bytes).unwrap());
            }
            assert_eq!(set.num_blobs(), 1);
            assert_eq!(set.usage_counter().get(&hash), Some(&2));
            set.evict(0).unwrap();
            assert_eq!(set.resident.usage().accounted_bytes(), bytes.len() as u64);

            // All caller handles are gone. Only the table policy now prevents
            // another blob from reclaiming this one's entire memory allowance.
            assert!(matches!(
                set.insert(EvictionPolicy::Memory, b"replacement"),
                Err(BlobError::MemoryLimitExceeded(_))
            ));
            assert_eq!(set.num_blobs(), 1);
            assert_eq!(&*set.get(hash).unwrap(), bytes);

            // Removing every logical owner must still release the pinned frame.
            for policy in policies {
                set.free_ref(hash, policy).unwrap();
            }
            assert_eq!(resident.usage().accounted_bytes(), 0);
            assert!(set.insert(EvictionPolicy::Memory, b"replacement").is_ok());
        }
    }

    #[test]
    fn never_evict_insert_promotes_a_nonresident_shared_blob() {
        let set = BlobSet::new_for_test();
        let bytes = b"previously evicted";
        let hash = BlobHashWrapper::hash_from_bytes(bytes);
        drop(set.insert(EvictionPolicy::Memory, bytes).unwrap());
        set.evict(0).unwrap();
        assert_eq!(set.resident.usage().accounted_bytes(), 0);

        // This uses the nonresident insertion path, rather than updating an
        // existing resident entry. Both paths must preserve the new policy.
        drop(set.insert(EvictionPolicy::Never, bytes).unwrap());
        set.evict(0).unwrap();
        assert_eq!(set.resident.usage().accounted_bytes(), bytes.len() as u64);
        assert_eq!(set.usage_counter().get(&hash), Some(&2));
        assert_eq!(&*set.get(hash).unwrap(), bytes);
    }

    #[test]
    fn explicit_pin_faults_a_blob_without_adding_a_logical_reference() {
        let set = BlobSet::new_for_test();
        let bytes = b"restored blob";
        let hash = BlobHashWrapper::hash_from_bytes(bytes);
        drop(set.insert(EvictionPolicy::Memory, bytes).unwrap());
        drop(set.insert(EvictionPolicy::Memory, bytes).unwrap());
        set.evict(0).unwrap();
        assert_eq!(set.resident.usage().accounted_bytes(), 0);

        // Snapshot recovery assigns a pin to each of the two existing logical
        // references. Neither pin() call may add another logical reference.
        set.pin(hash).unwrap();
        set.pin(hash).unwrap();
        set.evict(0).unwrap();
        assert_eq!(set.resident.usage().accounted_bytes(), bytes.len() as u64);
        assert_eq!(set.usage_counter().get(&hash), Some(&2));
        assert_eq!(&*set.get(hash).unwrap(), bytes);

        // Releasing one restored owner must retain the other owner's pin.
        set.free_ref(hash, EvictionPolicy::Never).unwrap();
        assert_eq!(set.usage_counter().get(&hash), Some(&1));
        set.evict(0).unwrap();
        assert_eq!(set.resident.usage().accounted_bytes(), bytes.len() as u64);
        set.free_ref(hash, EvictionPolicy::Never).unwrap();
        set.evict(0).unwrap();
        assert_eq!(set.resident.usage().accounted_bytes(), 0);
        assert!(matches!(set.pin(hash), Err(BlobError::MissingBlob(_))));
    }

    proptest! {
        #[test]
        fn removing_memory_row_allows_shared_blob_disk_eviction(
            bytes in vec(any::<u8>(), 1..128),
        ) {
            use crate::tiered::{BackingStore, MemoryObjectStore};

            let storage = Arc::new(TieredStorage::empty(
                BackingStore {
                    object_store: Box::new(MemoryObjectStore::default()),
                    budget: ByteBudget::unlimited(),
                },
                Some(BackingStore {
                    object_store: Box::new(MemoryObjectStore::default()),
                    budget: ByteBudget::unlimited(),
                }),
            ));
            let resident = Arc::new(ResidentObjects::new(ByteBudget::unlimited()));
            let blobs = BlobSet::new(resident, storage.clone());
            let hash = BlobHashWrapper::hash_from_bytes(&bytes);
            drop(blobs.insert(EvictionPolicy::Memory, &bytes).unwrap());
            drop(blobs.insert(EvictionPolicy::Disk, &bytes).unwrap());
            blobs.evict(0).unwrap();

            // Memory protects the shared disk copy until that row is released.
            prop_assert_eq!(storage.evict(0).unwrap(), 0);
            blobs.free_ref(hash, EvictionPolicy::Memory).unwrap();

            // The remaining Disk row permits remote eviction and must remain readable.
            prop_assert_eq!(storage.evict(0).unwrap(), bytes.len() as u64);
            let recovered = blobs.get(hash).unwrap();
            prop_assert_eq!(&*recovered, bytes.as_slice());
        }
    }
}
