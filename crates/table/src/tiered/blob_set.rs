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
        storage::Object,
        ByteBudget, ByteBudgetUsage, ResidentObjects, TieredStorage,
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

    /// Create a new [BlobSet] from a catalog of entries.
    ///
    /// The blobs in the catalog will be faulted in lazily as they are accessed.
    pub fn from_catalog(
        resident: Arc<ResidentObjects>,
        storage: Arc<TieredStorage>,
        catalog: impl IntoIterator<Item = CatalogEntry>,
    ) -> Self {
        let entries = catalog
            .into_iter()
            .map(|CatalogEntry { hash, uses, size }| (hash, BlobEntry::NonResident { uses, size }))
            .collect();

        Self {
            resident,
            storage,
            entries: RwLock::new(entries),
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
                    blob: BlobHandle { frame },
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

    /// Attempt to evict resident blobs in this set that have no external
    /// references.
    pub fn evict(&self) -> Result<ByteBudgetUsage, BlobError> {
        let mut candidates = self.evictable.lock().unwrap();
        self.resident.evictable_blobs(&mut candidates);
        for candidate in candidates.drain(..) {
            let maybe_resident = self.entries.read().get(&candidate.key).and_then(|entry| {
                let BlobEntry::Resident { blob, .. } = entry else {
                    return None;
                };
                Some(blob.clone())
            });

            if let Some(blob) = maybe_resident {
                let key = self.storage.write(Object::from(&blob))?;
                assert_eq!(
                    key.as_blob_hash().expect("storage key to be a blob hash"),
                    &candidate.key
                );

                let mut entries = self.entries.write();
                let Some(entry) = entries.get_mut(&candidate.key) else {
                    continue;
                };
                let BlobEntry::Resident { uses, blob: current } = entry else {
                    continue;
                };
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

                *entry = BlobEntry::NonResident {
                    uses: *uses,
                    size: blob.size(),
                };
                drop(blob);
                self.resident.evict_blob(candidate);
                drop(entries);
            }
        }

        Ok(self.resident.usage())
    }

    fn reserve(&self, hash: BlobHashWrapper, bytes: &[u8]) -> Result<ReservedBlob, BlobError> {
        debug_assert_eq!(hash, BlobHashWrapper::hash_from_bytes(bytes));
        let size = bytes.len() as u64;
        self.resident.reserve_blob(size, hash, || bytes.into()).or_else(|_| {
            self.evict()?;
            self.resident
                .reserve_blob(size, hash, || bytes.into())
                .map_err(BlobError::from)
        })
    }

    /// Insert `bytes` into this set, making it a resident blob.
    ///
    /// Subject to the memory budget.
    pub fn insert(&self, bytes: &[u8]) -> Result<BlobHandle, BlobError> {
        self.insert_with_uses(NonZeroUsize::MIN, bytes)
    }

    fn insert_with_uses(&self, uses: NonZeroUsize, bytes: &[u8]) -> Result<BlobHandle, BlobError> {
        let hash = BlobHashWrapper::hash_from_bytes(bytes);
        {
            let mut entries = self.entries.write();
            if let Some(BlobEntry::Resident {
                uses: current_uses,
                blob,
            }) = entries.get_mut(&hash)
            {
                *current_uses += uses.get();
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
                    blob: handle.clone(),
                });
                Ok(handle)
            }
            hash_map::Entry::Occupied(mut entry) => match entry.get_mut() {
                // Entry became resident while we didn't hold the entries lock.
                // Drop the reservation and update the entry's refcount.
                BlobEntry::Resident {
                    uses: current_uses,
                    blob,
                } => {
                    *current_uses += uses.get();
                    Ok(blob.clone())
                }
                BlobEntry::NonResident { uses: current_uses, .. } => {
                    let new_uses = *current_uses + uses.get();
                    let frame = self.resident.redeem_blob(reservation);
                    let handle = BlobHandle { frame };
                    entry.insert(BlobEntry::Resident {
                        uses: new_uses,
                        blob: handle.clone(),
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
    /// If the refcount is zero of decrementing, the entry is removed from the
    /// entry catalog.
    ///
    /// Returns [BlobError::MissingBlob] if no such blob is in this blob set's
    /// entry catalog.
    ///
    /// On success returns the size in bytes of the blob.
    pub fn free_ref(&self, hash: BlobHashWrapper) -> Result<u64, BlobError> {
        let mut entries = self.entries.write();
        match entries.entry(hash) {
            hash_map::Entry::Vacant(_) => Err(BlobError::MissingBlob(Box::new(hash.into()))),
            hash_map::Entry::Occupied(mut entry) => {
                let size = entry.get().size();
                entry.get_mut().dec_uses();
                if entry.get().uses() == 0 {
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
        let blob_or_bytes = {
            let entries = self.entries.read();
            let entry = entries
                .get(&hash)
                .ok_or_else(|| BlobError::MissingBlob(Box::new(hash.into())))?;
            match entry {
                BlobEntry::Resident { blob, .. } => Ok::<_, BlobError>(Ok(blob.clone())),
                BlobEntry::NonResident { .. } => {
                    let bytes = self.storage.read(hash.into())?;
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
                    BlobEntry::NonResident { uses, .. } => {
                        let frame = self.resident.redeem_blob(reservation);
                        let handle = BlobHandle { frame };
                        *entry = BlobEntry::Resident {
                            uses: *uses,
                            blob: handle.clone(),
                        };
                        Ok(handle)
                    }
                }
            }
        }?;
        assert_eq!(hash, handle.hash());
        self.resident.touch_blob(&handle.frame);

        Ok(handle)
    }

    /// Fault all blobs in this set and return their pin handles.
    ///
    /// This is intended to be used on a transaction's blob set during
    /// preparation, not the entire database's blob storage.
    ///
    /// Subject to the resident memory budget.
    pub fn pin_all(&self) -> Result<Box<[BlobHandle]>, BlobError> {
        let hashes: Box<[_]> = self
            .entries
            .read()
            .iter()
            .filter_map(|(&hash, entry)| (entry.uses() > 0).then_some(hash))
            .collect();
        hashes.into_iter().map(|hash| self.get(hash)).collect()
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

/// A catalog entry is metadata about a blob, without the blob's content.
///
/// Can be used to instantiate a [BlobSet] without any resident blobs (see:
/// [BlobSet::from_catalog]).
pub struct CatalogEntry {
    pub hash: BlobHashWrapper,
    pub uses: usize,
    pub size: u64,
}

#[derive(Debug)]
enum BlobEntry {
    Resident {
        uses: usize,
        blob: BlobHandle,
    },
    #[allow(unused)]
    NonResident {
        uses: usize,
        size: u64,
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
}

struct BlobMapEntry {
    uses: NonZeroUsize,
    blob: Box<[u8]>,
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use proptest::{collection::vec, prelude::*};

    use crate::{
        blob_store::BlobHash,
        tiered::{
            map::BlobHashWrapper, BlobError, BlobSet, ByteBudget, ByteBudgetConfig, ResidentObjects, TieredStorage,
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
                let handle = set.insert(bytes).unwrap();
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
                prop_assert_eq!(set.free_ref(hash.into()).unwrap(), bytes.len() as u64);
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
                low_water_bytes: 0,
                soft_limit_bytes: hard_limit_bytes,
                hard_limit_bytes,
            }).unwrap();
            let resident = Arc::new(ResidentObjects::new(budget));
            let storage = Arc::new(TieredStorage::new_for_test());
            let set = BlobSet::new(resident.clone(), storage);

            set.insert(&first).map(drop).unwrap();
            // This should evict `first`, as there is no external handle.
            set.insert(&second).map(drop).unwrap();

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
                low_water_bytes: 0,
                soft_limit_bytes: hard_limit_bytes,
                hard_limit_bytes,
            }).unwrap();
            let resident = Arc::new(ResidentObjects::new(budget));
            let storage = Arc::new(TieredStorage::new_for_test());
            let set = BlobSet::new(resident.clone(), storage);

            let _first_handle = set.insert(&first).unwrap();
            prop_assert!(matches!(set.insert(&second), Err(BlobError::MemoryLimitExceeded(_))));
        }
    }

    #[test]
    fn preparing_blobs_consumes_budget_once() {
        let bytes = b"prepared blob";
        let budget = ByteBudget::new(ByteBudgetConfig {
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
        transaction.insert(bytes).map(drop).unwrap();
        let pins = transaction.pin_all().unwrap();

        // Emulate transaction commit.
        // The shared [ResidentObjects] notices that it's the same object, so
        // reuses the already-held budget permit.
        assert!(committed.usage_counter().is_empty());
        committed.insert(bytes).map(drop).unwrap();

        assert_eq!(committed.usage_counter().get(&pins[0].hash()), Some(&1));
        assert_eq!(resident.usage().accounted_bytes(), bytes.len() as u64);
    }
}
