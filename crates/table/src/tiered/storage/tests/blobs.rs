use super::*;
use crate::tiered::EvictionPolicy;
use proptest::prelude::*;

proptest! {
    #[test]
    fn resident_data_survives_a_failed_first_write(
        bytes in proptest::collection::vec(any::<u8>(), 1..128),
        restored in any::<bool>(),
    ) {
        use crate::tiered::{blob_set::BlobMap, BlobSet, ResidentObjects};

        let local = Arc::new(TestStore::default());
        let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());
        let resident = Arc::new(ResidentObjects::new(ByteBudget::unlimited()));
        let hash = BlobHashWrapper::hash_from_bytes(&bytes);
        let policy = EvictionPolicy::Evictable;
        let blobs = if restored {
            let mut map = BlobMap::default();
            map.insert_with_uses(
                &hash.into(),
                std::num::NonZeroUsize::MIN,
                bytes.clone().into(),
            );
            BlobSet::from_blob_map(resident, storage.clone(), map)
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

        // The retry transfers ownership into persisted backing.
        local.fail_writes.store(false, Ordering::Relaxed);
        prop_assert_eq!(blobs.evict(0).unwrap(), bytes.len() as u64);
        prop_assert_eq!(blobs.resident_bytes_used_by_blobs(), 0);
        let recovered = blobs.get(hash).unwrap();
        prop_assert_eq!(&*recovered, bytes.as_slice());
        drop(recovered);

        blobs.free_ref(hash, policy).unwrap();
        prop_assert_eq!(storage.reclaim().unwrap(), bytes.len() as u64);
        prop_assert!(storage.catalog.read().is_empty());
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
    drop(first.insert(EvictionPolicy::Evictable, b"shared").unwrap());
    first.evict(0).unwrap();
    drop(second.insert(EvictionPolicy::Evictable, b"shared").unwrap());
    second.evict(0).unwrap();

    // Destroying a set must not invalidate another set's nonresident entry.
    drop(first);
    assert_eq!(storage.reclaim().unwrap(), 0);
    assert_eq!(&*second.get(hash).unwrap(), b"shared");

    // The final set release removes the remaining local reservation.
    drop(second);
    assert_eq!(storage.reclaim().unwrap(), 6);
}
