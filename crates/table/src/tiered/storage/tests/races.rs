use super::*;

#[test]
fn pending_writer_prevents_slot_reclamation() {
    let storage = Arc::new(TieredStorage::new_for_test());
    let key = ObjectKey::from(BlobHashWrapper::hash_from_bytes(b"aaaa"));
    let slot = storage.catalog.write().entry(key).or_default().clone();
    let state = slot.state.lock();

    // Pause registration before it can acquire ownership of an otherwise unowned slot.
    let writing = storage.clone();
    let writer = thread::spawn(move || writing.write(object(b"aaaa"), DiskRetention::Required));
    wait_for_pending_writer(&slot);

    // Collection must skip the reservation without waiting for its object lock.
    storage.should_reclaim.store(true, Ordering::Release);
    assert_eq!(storage.reclaim().unwrap(), 0);
    assert!(storage.should_reclaim.load(Ordering::Acquire));
    assert!(Arc::ptr_eq(storage.catalog.read().get(&key).unwrap(), &slot));
    drop(state);

    // The writer publishes through the same slot, then normal last-owner cleanup applies.
    let owner = writer.join().unwrap().unwrap();
    assert_eq!(slot.registering.load(Ordering::Relaxed), 0);
    assert_eq!(storage.reclaim().unwrap(), 0);
    assert_eq!(&*storage.read(key).unwrap(), b"aaaa");
    drop(owner);
    assert_eq!(storage.reclaim().unwrap(), 4);
    assert!(storage.catalog.read().is_empty());
}

#[test]
fn policy_change_during_write_controls_eviction() {
    use crate::tiered::{BlobSet, ResidentObjects};

    for require_memory in [false, true] {
        let (pause, reached, resume) = pause();
        let local = Arc::new(TestStore {
            after_write: Some(pause),
            ..<_>::default()
        });
        let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());
        let resident = Arc::new(ResidentObjects::new(ByteBudget::unlimited()));
        let blobs = Arc::new(BlobSet::new(resident, storage.clone()));
        let hash = BlobHashWrapper::hash_from_bytes(b"shared");
        drop(blobs.insert(EvictionPolicy::Disk, b"shared").unwrap());

        // Stop the first write while the blob's entry still owns only resident bytes.
        let evicting = blobs.clone();
        let eviction = thread::spawn(move || evicting.evict(0));
        reached.recv_timeout(TIMEOUT).unwrap();

        // Install the new requirement while I/O holds no blob catalog lock.
        if require_memory {
            // Never must cancel eviction even though writing has already started.
            blobs.pin(hash).unwrap();
        } else {
            // Memory must protect the completed disk copy before memory is released.
            blobs.pin_disk(hash);
        }
        resume.send(()).unwrap();
        let freed = eviction.join().unwrap().unwrap();

        if require_memory {
            assert_eq!(freed, 0);
            assert!(blobs.resident_bytes_used_by_blobs() >= 6);
            // The unused write reference was released; its redundant copy is collectible.
            assert_eq!(storage.reclaim().unwrap(), 6);
        } else {
            assert_eq!(freed, 6);
            assert_eq!(blobs.resident_bytes_used_by_blobs(), 0);
            assert_eq!(storage.evict(0).unwrap(), 0);
            assert!(local.stat(hash.into()).unwrap().is_some());
        }
        assert_eq!(&*blobs.get(hash).unwrap(), b"shared");

        drop(blobs);
        storage.reclaim().unwrap();
        assert!(storage.catalog.read().is_empty());
        assert_eq!(storage.local.budget.usage().accounted_bytes, 0);
    }
}

#[test]
fn write_during_reclamation_uses_replacement_slot() {
    let (pause, reached, resume) = pause();
    let local = Arc::new(TestStore {
        after_remove: Some(pause),
        ..<_>::default()
    });
    let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());
    let object_to_write = object(b"aaaa");
    let owner = storage.write(object_to_write, DiskRetention::Evictable).unwrap();
    let key = owner.key();

    // Pause after the file disappears while collection still excludes new owners.
    drop(owner);
    let collecting = storage.clone();
    let collector = thread::spawn(move || collecting.reclaim());
    reached.recv_timeout(TIMEOUT).unwrap();
    let catalog_is_locked = storage.catalog.try_read().is_none();

    // A concurrent writer must register against the replacement catalog slot.
    let writing = storage.clone();
    let writer = thread::spawn(move || {
        let object_to_write = object(b"aaaa");
        let owner = writing.write(object_to_write, DiskRetention::Required)?;

        Ok::<_, ObjectStoreError>(owner)
    });
    resume.send(()).unwrap();

    assert!(catalog_is_locked);
    assert_eq!(collector.join().unwrap().unwrap(), 4);
    let owner = writer.join().unwrap().unwrap();

    // The replacement remains reachable and retains exactly one reservation.
    assert_eq!(owner.key(), key);
    assert_eq!(&*storage.read(key).unwrap(), b"aaaa");
    assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
}

#[test]
fn pending_registration_protects_an_unlocked_slot() {
    let storage = TieredStorage::new_for_test();
    let garbage = storage.write(object(b"garbage"), DiskRetention::Evictable).unwrap();
    drop(garbage);

    // Pause a writer immediately after catalog lookup, before it takes the slot lock.
    let key = ObjectKey::from(BlobHashWrapper::hash_from_bytes(b"pending"));
    let pending = {
        let mut catalog = storage.catalog.write();
        let slot = catalog.entry(key).or_default().clone();
        slot.registering.fetch_add(1, Ordering::Relaxed);
        slot
    };

    // The slot is unowned and unlocked; only the pending registration protects its identity.
    assert!(pending.state.try_lock().is_some());
    assert_eq!(storage.reclaim().unwrap(), 7);
    assert!(storage
        .catalog
        .read()
        .get(&key)
        .is_some_and(|slot| Arc::ptr_eq(slot, &pending)));

    // Abandoning that pending write permits collection of its empty slot.
    pending.registering.fetch_sub(1, Ordering::Relaxed);
    assert_eq!(storage.reclaim().unwrap(), 0);
    assert!(!storage.catalog.read().contains_key(&key));
}
