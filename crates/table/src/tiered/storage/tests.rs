// Shared stores and synchronization helpers for deterministic I/O failures and races.
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
    after_write: Option<Arc<Pause>>,
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
        let key = self.inner.write(object)?;
        if let Some(pause) = &self.after_write {
            pause.wait();
        }

        Ok(key)
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

fn wait_for_pending_writer(slot: &ObjectSlot) {
    let deadline = std::time::Instant::now() + TIMEOUT;
    while slot.registering.load(Ordering::Relaxed) == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "writer did not start registration"
        );
        thread::yield_now();
    }
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
    let object_to_write = object(b"aaaa");
    let owner = storage.write(object_to_write).unwrap();
    let key = owner.key();
    let slot = storage.catalog.read().get(&key).unwrap().clone();
    let reader_owner = owner.clone();

    let evicting = storage.clone();
    let eviction = thread::spawn(move || evicting.evict(0));
    reached.recv_timeout(TIMEOUT).unwrap();

    let transition_is_locked = slot.state.try_lock().is_none();
    let reading = storage.clone();
    let reader = thread::spawn(move || {
        if overwrite {
            let _rewritten = reading.write(object(b"aaaa"))?;
        }
        let bytes = reading.read(key);
        drop(reader_owner);
        bytes
    });
    // Ensure the overwriter is waiting on this key before testing unrelated writes.
    if overwrite {
        wait_for_pending_writer(&slot);
    }

    // Unrelated keys can still be written while the eviction's I/O is paused.
    let object_to_write = object(b"bbbb");
    let other_owner = storage.write(object_to_write).unwrap();
    let other_key = other_owner.key();
    resume.send(()).unwrap();

    assert!(transition_is_locked);
    assert_eq!(eviction.join().unwrap().unwrap(), 4);
    assert_eq!(&*reader.join().unwrap().unwrap(), b"aaaa");
    assert!(local.stat(key).unwrap().is_some());
    assert!(local.stat(other_key).unwrap().is_some());
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
    let object_to_write = object(b"aaaa");
    let owner = storage.write(object_to_write).unwrap();
    let key = owner.key();

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
        tier: CacheTier::Disk,
        low_water_bytes: 0,
        soft_limit_bytes: 4,
        hard_limit_bytes: 4,
    })
    .unwrap();
    let storage = storage(Arc::default(), Arc::default(), budget);

    let object_to_write = object(b"aaaa");
    let owner = storage.write(object_to_write).unwrap();
    let key = owner.key();
    let object_to_write = object(b"bbbb");
    let _other_owner = storage.write(object_to_write).unwrap();

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

    let object_to_write = object(b"aaaa");
    let owner = storage.write(object_to_write).unwrap();
    let key = owner.key();
    let slot = storage.catalog.read().get(&key).unwrap().clone();
    let _rewritten = storage.write(object(b"aaaa")).unwrap();

    assert!(Arc::ptr_eq(&slot, storage.catalog.read().get(&key).unwrap()));
    assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
    assert_eq!(&*storage.read(key).unwrap(), b"aaaa");
}

#[test]
fn failed_initial_write_does_not_publish_the_object() {
    // Failed I/O must leave no published object or budget permit. The
    // in-flight ownership is released automatically so a retry needs no handle.
    let local = Arc::new(TestStore::default());
    local.fail_writes.store(true, Ordering::Relaxed);
    let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());

    let key = BlobHashWrapper::hash_from_bytes(b"aaaa").into();
    assert!(storage.write(object(b"aaaa")).is_err());

    assert!(!storage.contains(key));
    assert!(matches!(storage.stat(key), Err(ObjectStoreError::UnknownObject(_))));
    assert!(matches!(storage.read(key), Err(ObjectStoreError::UnknownObject(_))));
    assert_eq!(storage.local.budget.usage().accounted_bytes, 0);
    assert_eq!(storage.evict(0).unwrap(), 0);

    // Collection can remove the failed slot before a retry registers a new one.
    storage.reclaim().unwrap();
    assert!(storage.catalog.read().is_empty());
    local.fail_writes.store(false, Ordering::Relaxed);
    let _rewritten = storage.write(object(b"aaaa")).unwrap();
    assert_eq!(storage.stat(key).unwrap().size, 4);
    assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
}

#[test]
fn failed_remote_write_preserves_local_bytes_and_permit() {
    let local = Arc::new(TestStore::default());
    let remote = Arc::new(TestStore::default());
    remote.fail_writes.store(true, Ordering::Relaxed);

    let storage = storage(local.clone(), remote, ByteBudget::unlimited());

    let object_to_write = object(b"aaaa");
    let owner = storage.write(object_to_write).unwrap();
    let key = owner.key();

    assert!(storage.evict(0).is_err());
    assert!(local.stat(key).unwrap().is_some());
    assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
    assert_eq!(&*storage.read(key).unwrap(), b"aaaa");
}

#[test]
fn eviction_does_nothing_when_target_is_met() {
    let local = Arc::new(TestStore::default());
    let storage = storage(local.clone(), Arc::default(), ByteBudget::unlimited());

    let object_to_write = object(b"aaaa");
    let owner = storage.write(object_to_write).unwrap();
    let key = owner.key();

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
    let object_to_write = object(b"aaaa");
    let owner = storage.write(object_to_write).unwrap();
    let key = owner.key();

    assert!(storage.evict(0).is_err());
    assert!(local.stat(key).unwrap().is_some());
    assert_eq!(storage.local.budget.usage().accounted_bytes, 4);
    assert_eq!(&*storage.read(key).unwrap(), b"aaaa");
}

mod blobs;
mod races;
mod reclamation;
