use super::*;
use proptest::prelude::*;

proptest! {
    #[test]
    fn resident_data_survives_a_failed_first_write(
        bytes in proptest::collection::vec(any::<u8>(), 1..128),
        required in any::<bool>(),
        restored in any::<bool>(),
    ) {
        use crate::tiered::{blob_set::BlobMap, BlobSet, ResidentObjects};

        let local = Arc::new(TestStore::default());
        let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());
        let resident = Arc::new(ResidentObjects::new(ByteBudget::unlimited()));
        let hash = BlobHashWrapper::hash_from_bytes(&bytes);
        let policy = if required {
            EvictionPolicy::Memory
        } else {
            EvictionPolicy::Disk
        };
        let blobs = if restored {
            let mut map = BlobMap::default();
            map.insert_with_uses(
                &hash.into(),
                std::num::NonZeroUsize::MIN,
                bytes.clone().into(),
            );
            let blobs = BlobSet::from_blob_map(resident, storage.clone(), map);
            // Snapshot restore installs the Memory row's requirement before trimming.
            if required {
                blobs.pin_disk(hash);
            }
            blobs
        } else {
            let blobs = BlobSet::new(resident, storage.clone());
            drop(blobs.insert(policy, &bytes).unwrap());
            blobs
        };

        // Fresh resident entries own only memory, including after snapshot restoration.
        prop_assert!(storage.catalog.read().is_empty());
        prop_assert_eq!(storage.local.budget.usage().accounted_bytes, 0);

        // A failed eviction keeps resident bytes available and releases the in-flight owner.
        local.fail_writes.store(true, Ordering::Relaxed);
        prop_assert!(blobs.evict(0).is_err());
        storage.reclaim().unwrap();
        prop_assert!(storage.catalog.read().is_empty());
        let recovered = blobs.get(hash).unwrap();
        prop_assert_eq!(&*recovered, bytes.as_slice());
        drop(recovered);

        // The retry transfers ownership and the row's requirement into persisted backing.
        local.fail_writes.store(false, Ordering::Relaxed);
        prop_assert_eq!(blobs.evict(0).unwrap(), bytes.len() as u64);
        prop_assert_eq!(blobs.resident_bytes_used_by_blobs(), 0);
        let slot = storage
            .catalog
            .read()
            .get(&ObjectKey::from(hash))
            .unwrap()
            .clone();
        prop_assert_eq!(slot.state.lock().ownership.is_pinned(), required);
        let recovered = blobs.get(hash).unwrap();
        prop_assert_eq!(&*recovered, bytes.as_slice());
        drop(recovered);

        blobs.free_ref(hash, policy).unwrap();
        prop_assert_eq!(storage.reclaim().unwrap(), bytes.len() as u64);
        prop_assert!(storage.catalog.read().is_empty());
    }
}

proptest! {
    #[test]
    fn snapshot_restore_preserves_repeated_disk_requirements(
        bytes in proptest::collection::vec(any::<u8>(), 1..128),
        required_rows in 0u8..8,
    ) {
        use crate::tiered::{blob_set::BlobMap, BlobSet, ResidentObjects};

        let storage = Arc::new(TieredStorage::new_for_test());
        let resident = Arc::new(ResidentObjects::new(ByteBudget::unlimited()));
        let hash = BlobHashWrapper::hash_from_bytes(&bytes);
        let uses = std::num::NonZeroUsize::new(usize::from(required_rows) + 1).unwrap();
        let mut restored = BlobMap::default();
        restored.insert_with_uses(&hash.into(), uses, bytes.clone().into());
        let blobs = BlobSet::from_blob_map(resident, storage.clone(), restored);

        // Restore one Memory requirement per row; the additional row has Disk policy.
        for _ in 0..required_rows {
            blobs.pin_disk(hash);
        }
        prop_assert!(!storage.contains(hash.into()));
        blobs.evict(0).unwrap();
        let slot = storage
            .catalog
            .read()
            .get(&ObjectKey::from(hash))
            .unwrap()
            .clone();
        prop_assert_eq!(slot.state.lock().ownership.is_pinned(), required_rows > 0);

        // Faulting and evicting again must preserve all restored requirements.
        let recovered = blobs.get(hash).unwrap();
        prop_assert_eq!(&*recovered, bytes.as_slice());
        drop(recovered);
        blobs.evict(0).unwrap();
        for _ in 0..required_rows {
            blobs.free_ref(hash, EvictionPolicy::Memory).unwrap();
        }
        prop_assert!(!slot.state.lock().ownership.is_pinned());

        // The final Disk row owns the backing until it too is released.
        prop_assert_eq!(storage.reclaim().unwrap(), 0);
        blobs.free_ref(hash, EvictionPolicy::Disk).unwrap();
        prop_assert_eq!(storage.reclaim().unwrap(), bytes.len() as u64);
        prop_assert!(storage.catalog.read().is_empty());
    }
}

#[test]
fn shared_disk_requirements_survive_fault_and_eviction() {
    use crate::tiered::{BlobSet, EvictionPolicy, ResidentObjects};
    for policies in [
        [EvictionPolicy::Memory, EvictionPolicy::Disk],
        [EvictionPolicy::Disk, EvictionPolicy::Memory],
    ] {
        let local = Arc::new(TestStore::default());
        let remote = Arc::new(TestStore::default());
        let storage = storage(local.clone(), remote.clone(), ByteBudget::unlimited());
        let blobs = BlobSet::new(Arc::new(ResidentObjects::new(ByteBudget::unlimited())), storage.clone());
        let hash = BlobHashWrapper::hash_from_bytes(b"shared");

        // Insertion order cannot let an evictable row downgrade a local requirement.
        for policy in policies {
            drop(blobs.insert(policy, b"shared").unwrap());
        }

        blobs.evict(0).unwrap();
        assert_eq!(storage.evict(0).unwrap(), 0);
        assert!(local.stat(hash.into()).unwrap().is_some());
        assert!(remote.stat(hash.into()).unwrap().is_none());

        // Faulting and re-evicting must transfer the current requirement.
        drop(blobs.get(hash).unwrap());
        blobs.evict(0).unwrap();
        assert_eq!(storage.evict(0).unwrap(), 0);

        // The remaining evictable row still needs the bytes after the local pin ends.
        blobs.free_ref(hash, EvictionPolicy::Memory).unwrap();
        assert_eq!(storage.evict(0).unwrap(), 6);
        assert_eq!(&*blobs.get(hash).unwrap(), b"shared");

        // Releasing that final row makes the local copy collectible.
        blobs.free_ref(hash, EvictionPolicy::Disk).unwrap();
        storage.reclaim().unwrap();
        assert!(!storage.contains(hash.into()));
        assert!(local.stat(hash.into()).unwrap().is_none());
    }
}

#[test]
fn dropping_one_set_preserves_anothers_backing() {
    use crate::tiered::{BlobSet, EvictionPolicy, ResidentObjects};
    let storage = Arc::new(TieredStorage::new_for_test());
    let resident = Arc::new(ResidentObjects::new(ByteBudget::unlimited()));
    let first = BlobSet::new(resident.clone(), storage.clone());
    let second = BlobSet::new(resident, storage.clone());
    let hash = BlobHashWrapper::hash_from_bytes(b"shared");

    // Give both sets independent backing owners for the same content.
    drop(first.insert(EvictionPolicy::Disk, b"shared").unwrap());
    first.evict(0).unwrap();
    drop(second.insert(EvictionPolicy::Memory, b"shared").unwrap());
    second.evict(0).unwrap();

    // Destroying a set must not invalidate another set's nonresident entry.
    drop(first);
    assert_eq!(storage.reclaim().unwrap(), 0);
    assert_eq!(&*second.get(hash).unwrap(), b"shared");

    // The final set release removes the remaining local reservation.
    drop(second);
    assert_eq!(storage.reclaim().unwrap(), 6);
}
