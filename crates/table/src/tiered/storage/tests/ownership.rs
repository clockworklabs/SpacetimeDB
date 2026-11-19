use super::*;
use proptest::prelude::*;

proptest! {
    #[test]
    fn write_returns_readable_backing_with_requested_retention(
        bytes in proptest::collection::vec(any::<u8>(), 1..128),
        retention in prop_oneof![
            Just(DiskRetention::Evictable),
            Just(DiskRetention::Required),
        ],
    ) {
        let local = Arc::new(TestStore::default());
        let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());
        let reference = storage.write(object(&bytes), retention).unwrap();
        let key = reference.key();

        // Success publishes readable bytes together with their lifetime and retention.
        prop_assert!(storage.contains(key));
        prop_assert_eq!(
            reference.pins(),
            u64::from(retention == DiskRetention::Required)
        );
        prop_assert_eq!(
            storage.local.budget.usage().accounted_bytes,
            bytes.len() as u64
        );
        prop_assert_eq!(storage.reclaim().unwrap(), 0);
        let recovered = storage.read(key).unwrap();
        prop_assert_eq!(recovered.as_ref(), bytes.as_slice());

        drop(reference);
        prop_assert_eq!(storage.reclaim().unwrap(), bytes.len() as u64);
        prop_assert!(storage.catalog.read().is_empty());
        prop_assert_eq!(storage.local.budget.usage().accounted_bytes, 0);
    }
}

proptest! {
    #[test]
    fn local_pins_block_remote_eviction(
        bytes in proptest::collection::vec(any::<u8>(), 1..128),
        pinned_owner_count in 1u8..16,
    ) {
        // One object's exact capacity must also suffice for deduplicated writes.
        let local = Arc::new(TestStore::default());
        let remote = Arc::new(TestStore::default());
        let size = bytes.len() as u64;
        let budget = ByteBudget::new(ByteBudgetConfig {
            tier: crate::tiered::metrics::CacheTier::Disk,
            low_water_bytes: 0,
            soft_limit_bytes: size,
            hard_limit_bytes: size,
        })
        .unwrap();
        let storage = storage(local.clone(), remote.clone(), budget);
        let object_to_write = object(&bytes);
        let pinned = storage
            .write(object_to_write, DiskRetention::Required)
            .unwrap();
        let object_to_write = object(&bytes);
        let shared = storage
            .write(object_to_write, DiskRetention::Evictable)
            .unwrap();
        let key = pinned.key();
        let mut owners = (0..pinned_owner_count)
            .map(|_| pinned.clone())
            .collect::<Vec<_>>();
        drop(pinned);

        // Each remaining required owner independently blocks remote-only eviction.
        for _ in 0..pinned_owner_count {
            prop_assert_eq!(storage.evict(0).unwrap(), 0);
            prop_assert!(local.stat(key).unwrap().is_some());
            prop_assert!(remote.stat(key).unwrap().is_none());
            owners.pop();
        }

        // The evictable owner preserves the bytes after the local requirement ends.
        prop_assert_eq!(storage.evict(0).unwrap(), size);
        prop_assert!(local.stat(key).unwrap().is_none());
        prop_assert!(remote.stat(key).unwrap().is_some());
        let restored = storage.read(key).unwrap();
        prop_assert_eq!(restored.as_ref(), bytes.as_slice());

        // Local lifetime tracking must not delete a retained remote snapshot object.
        drop(shared);
        storage.reclaim().unwrap();
        prop_assert!(!storage.contains(key));
        prop_assert!(remote.stat(key).unwrap().is_some());
    }
}

proptest! {
    #[test]
    fn retention_policy_applies_to_pages_and_blobs(
        policy in prop_oneof![
            Just(EvictionPolicy::Never),
            Just(EvictionPolicy::Memory),
            Just(EvictionPolicy::Disk),
        ],
        is_page in any::<bool>(),
        bytes in proptest::collection::vec(any::<u8>(), 1..128),
    ) {
        use crate::{
            page_pool::PagePool,
            tiered::{PageManager, ResidentObjects},
        };
        use spacetimedb_sats::layout::Size;

        let local = Arc::new(TestStore::default());
        let remote = Arc::new(TestStore::default());
        let storage = storage(local.clone(), remote.clone(), ByteBudget::unlimited());

        // The same retention conversion governs serialized page versions and blobs.
        let object = if is_page {
            let manager = PageManager::new(
                Arc::new(ResidentObjects::new(ByteBudget::unlimited())),
                PagePool::new_for_test(),
                storage.clone(),
            );
            let slot = manager.allocate(Size(8), policy).unwrap();
            let handle = slot.page().unwrap();
            Object::from(&handle)
        } else {
            object(&bytes)
        };
        let object_to_write = object;
        let owner = storage.write(object_to_write, policy).unwrap();
        let key = owner.key();
        let original = storage.read(key).unwrap();
        prop_assert_eq!(key.as_page_hash().is_some(), is_page);

        // Only Memory requires local backing. Never retains bytes in memory, so disk
        // copies left by earlier policies may be evicted just like Disk backing.
        let freed = storage.evict(0).unwrap();
        match policy {
            EvictionPolicy::Memory => {
                prop_assert_eq!(freed, 0);
                prop_assert!(local.stat(key).unwrap().is_some());
                prop_assert!(remote.stat(key).unwrap().is_none());
            }
            EvictionPolicy::Never | EvictionPolicy::Disk => {
                prop_assert_eq!(freed, original.len() as u64);
                prop_assert!(local.stat(key).unwrap().is_none());
                prop_assert!(remote.stat(key).unwrap().is_some());
            }
        }
        let recovered = storage.read(key).unwrap();
        prop_assert_eq!(recovered.as_ref(), original.as_ref());

        // Reclamation still releases any local copy populated by the read above.
        drop(owner);
        storage.reclaim().unwrap();
        prop_assert!(local.stat(key).unwrap().is_none());
        prop_assert!(!storage.contains(key));
        prop_assert_eq!(storage.local.budget.usage().accounted_bytes, 0);
    }
}

proptest! {
    #[test]
    fn changing_pins_preserves_all_owner_contributions(
        pins in 0u64..16,
        extra_owners in 0u8..8,
    ) {
        let storage = TieredStorage::new_for_test();
        let mut owner = storage
            .write(object(b"shared"), DiskRetention::Evictable)
            .unwrap();
        let unpinned_clones = (0..extra_owners).map(|_| owner.clone()).collect::<Vec<_>>();
        let slot = owner.slot.clone();
        let counts = || {
            let state = slot.state.lock();
            match state.ownership {
                ObjectOwnership::Referenced { owners, total_pins } => (owners.get(), total_pins),
                ObjectOwnership::Unreferenced => (0, 0),
            }
        };
        let owners = 1 + u64::from(extra_owners);

        // Replacement must preserve ownership, including when this is the sole owner.
        owner.set_pins(pins);
        proptest::prop_assert_eq!(owner.pins(), pins);
        proptest::prop_assert_eq!(counts(), (owners, pins));
        let mut pinned_clone = owner.clone();
        proptest::prop_assert_eq!(counts(), (owners + 1, pins * 2));

        // Incremental adjustments use replacement without losing another handle's pins.
        owner.pin();
        proptest::prop_assert_eq!(counts(), (owners + 1, pins * 2 + 1));
        owner.unpin();
        proptest::prop_assert_eq!(owner.pins(), pins);
        proptest::prop_assert_eq!(counts(), (owners + 1, pins * 2));
        owner.set_pins(0);
        proptest::prop_assert_eq!(owner.pins(), 0);
        proptest::prop_assert_eq!(pinned_clone.pins(), pins);
        proptest::prop_assert_eq!(counts(), (owners + 1, pins));
        pinned_clone.set_pins(0);
        proptest::prop_assert_eq!(counts(), (owners + 1, 0));
        proptest::prop_assert!(!storage.should_reclaim.load(Ordering::Acquire));

        // Only the final handle release may request reclamation.
        drop(pinned_clone);
        drop(unpinned_clones);
        proptest::prop_assert_eq!(counts(), (1, 0));
        proptest::prop_assert!(!storage.should_reclaim.load(Ordering::Acquire));
        drop(owner);
        proptest::prop_assert_eq!(counts(), (0, 0));
        proptest::prop_assert!(storage.should_reclaim.load(Ordering::Acquire));
    }
}
