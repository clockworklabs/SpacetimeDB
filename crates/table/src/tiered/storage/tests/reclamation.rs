use super::*;
use proptest::prelude::*;

proptest! {
    #[test]
    fn failed_writes_leave_collectible_slots(
        bytes in proptest::collection::vec(any::<u8>(), 1..128),
    ) {
        let local = Arc::new(TestStore::default());
        let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());
        let key = BlobHashWrapper::hash_from_bytes(&bytes).into();

        // Failure returns no owner and publishes neither bytes nor a permit.
        local.fail_writes.store(true, Ordering::Relaxed);
        prop_assert!(storage
            .write(object(&bytes))
            .is_err());
        prop_assert!(!storage.contains(key));
        prop_assert!(local.stat(key).unwrap().is_none());
        prop_assert_eq!(storage.local.budget.usage().accounted_bytes, 0);

        // Even an empty failed-write slot becomes collectible without caller cleanup.
        prop_assert_eq!(storage.reclaim().unwrap(), 0);
        prop_assert!(storage.catalog.read().is_empty());

        // Retrying establishes a fresh backing lifetime.
        local.fail_writes.store(false, Ordering::Relaxed);
        let reference = storage
            .write(object(&bytes))
            .unwrap();
        prop_assert!(storage.contains(key));
        drop(reference);
        prop_assert_eq!(storage.reclaim().unwrap(), bytes.len() as u64);
    }
}

proptest! {
    #[test]
    fn last_owner_releases_local_capacity(
        bytes in proptest::collection::vec(any::<u8>(), 1..128),
        owner_count in 1u8..16,
        clone_owners in any::<bool>(),
    ) {
        // Both clones and independently retained handles protect the same bytes.
        let storage = TieredStorage::new_for_test();
        let object_to_write = object(&bytes);
        let first = storage
            .write(object_to_write)
            .unwrap();
        let key = first.key();
        let owners = (0..owner_count)
            .map(|_| {
                if clone_owners {
                    first.clone()
                } else {
                    storage
                        .write(object(&bytes))
                        .unwrap()
                }
            })
            .collect::<Vec<_>>();

        // Releasing one owner cannot remove another owner's backing object.
        drop(first);
        prop_assert_eq!(storage.reclaim().unwrap(), 0);
        prop_assert!(storage.local.object_store.stat(key).unwrap().is_some());

        // The last release reclaims both the file and its full reservation.
        drop(owners);
        prop_assert_eq!(storage.reclaim().unwrap(), bytes.len() as u64);
        prop_assert!(!storage.contains(key));
        prop_assert!(storage.local.object_store.stat(key).unwrap().is_none());
        prop_assert_eq!(storage.local.budget.usage().accounted_bytes, 0);
        prop_assert!(storage.catalog.read().is_empty());
    }
}

proptest! {
    #[test]
    fn failed_removal_preserves_capacity_until_retry(
        bytes in proptest::collection::vec(any::<u8>(), 1..128),
    ) {
        let local = Arc::new(TestStore::default());
        let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());
        let object_to_write = object(&bytes);
        let owner = storage
            .write(object_to_write)
            .unwrap();
        let key = owner.key();

        // Failed removal must not release a reservation for bytes still on disk.
        drop(owner);
        local.fail_remove.store(true, Ordering::Relaxed);
        prop_assert!(storage.reclaim().is_err());
        prop_assert_eq!(
            storage.local.budget.usage().accounted_bytes,
            bytes.len() as u64
        );
        prop_assert!(local.stat(key).unwrap().is_some());

        // A new owner cancels pending deletion until it releases the object again.
        let revived = storage
            .write(object(&bytes))
            .unwrap();
        local.fail_remove.store(false, Ordering::Relaxed);
        prop_assert_eq!(storage.reclaim().unwrap(), 0);
        prop_assert!(storage.contains(key));

        drop(revived);
        prop_assert_eq!(storage.reclaim().unwrap(), bytes.len() as u64);
        prop_assert_eq!(storage.local.budget.usage().accounted_bytes, 0);
    }
}

#[test]
fn busy_slots_are_retried_after_unlock() {
    let storage = Arc::new(TieredStorage::new_for_test());
    let owner = storage.write(object(b"aaaa")).unwrap();
    let key = owner.key();
    let slot = storage.catalog.read().get(&key).unwrap().clone();
    drop(owner);
    let state = slot.state.lock();

    // Busy slots must be skipped even when their final owner has already been released.
    let (sender, receiver) = mpsc::sync_channel(1);
    let collecting = storage.clone();
    let collector = thread::spawn(move || sender.send(collecting.reclaim()).unwrap());
    assert_eq!(receiver.recv_timeout(TIMEOUT).unwrap().unwrap(), 0);
    collector.join().unwrap();
    assert!(storage.should_reclaim.load(Ordering::Acquire));
    assert!(Arc::ptr_eq(storage.catalog.read().get(&key).unwrap(), &slot));
    drop(state);

    // Releasing the lock makes the same object collectible on the next pass.
    assert_eq!(storage.reclaim().unwrap(), 4);
    assert!(storage.catalog.read().is_empty());
    assert_eq!(storage.local.budget.usage().accounted_bytes, 0);
}

#[test]
fn full_disk_reclaims_unreferenced_objects() {
    let budget = ByteBudget::new(ByteBudgetConfig {
        tier: crate::tiered::metrics::CacheTier::Disk,
        low_water_bytes: 0,
        soft_limit_bytes: 4,
        hard_limit_bytes: 4,
    })
    .unwrap();
    let storage = storage(Arc::default(), Arc::default(), budget);
    let owner = storage.write(object(b"aaaa")).unwrap();
    let old_key = owner.key();
    drop(owner);

    // The new write cannot reserve capacity until collection removes the old object.
    let (sender, receiver) = mpsc::sync_channel(1);
    let writing = storage.clone();
    let writer = thread::spawn(move || {
        sender.send(writing.write(object(b"bbbb"))).unwrap();
    });
    let owner = receiver.recv_timeout(TIMEOUT).unwrap().unwrap();
    writer.join().unwrap();
    assert!(!storage.contains(old_key));
    assert_eq!(&*storage.read(owner.key()).unwrap(), b"bbbb");
    assert_eq!(storage.local.budget.usage().accounted_bytes, 4);

    drop(owner);
    assert_eq!(storage.reclaim().unwrap(), 4);
    assert!(storage.catalog.read().is_empty());
}
