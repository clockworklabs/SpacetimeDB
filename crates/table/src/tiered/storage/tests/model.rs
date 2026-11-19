use super::*;
use proptest::prelude::*;

#[derive(Clone, Debug)]
struct ModelOwner {
    object: DiskObject,
    expected_pins: u64,
}

#[derive(Clone, Debug)]
enum OwnershipAction {
    Write,
    Clone(u8),
    Pin(u8),
    Unpin(u8),
    Release(u8),
    Reclaim,
    Evict,
    ReclaimWithFailedRemoval,
}

fn ownership_action() -> impl Strategy<Value = OwnershipAction> {
    prop_oneof![
        Just(OwnershipAction::Write),
        any::<u8>().prop_map(OwnershipAction::Clone),
        any::<u8>().prop_map(OwnershipAction::Pin),
        any::<u8>().prop_map(OwnershipAction::Unpin),
        any::<u8>().prop_map(OwnershipAction::Release),
        Just(OwnershipAction::Reclaim),
        Just(OwnershipAction::Evict),
        Just(OwnershipAction::ReclaimWithFailedRemoval),
    ]
}

proptest! {
    #[test]
    fn ownership_actions_preserve_data_and_disk_accounting(
        bytes in proptest::collection::vec(any::<u8>(), 1..64),
        actions in proptest::collection::vec(ownership_action(), 1..96),
    ) {
        let local = Arc::new(TestStore::default());
        let remote = Arc::new(TestStore::default());
        let storage = storage(local.clone(), remote.clone(), ByteBudget::unlimited());
        let object_to_write = object(&bytes);
        let first = storage
            .write(object_to_write, DiskRetention::Evictable)
            .unwrap();
        let key = first.key();

        // The model tracks requirements per handle, independently of catalog state.
        let mut owners = vec![ModelOwner {
            object: first,
            expected_pins: 0,
        }];
        for action in actions {
            match action {
                OwnershipAction::Write => {
                    let object_to_write = object(&bytes);
                    let owner = storage
                        .write(object_to_write, DiskRetention::Evictable)
                        .unwrap();
                    owners.push(ModelOwner {
                        object: owner,
                        expected_pins: 0,
                    });
                }
                OwnershipAction::Clone(selector) if !owners.is_empty() => {
                    let index = usize::from(selector) % owners.len();
                    owners.push(owners[index].clone());
                }
                OwnershipAction::Pin(selector) if !owners.is_empty() => {
                    let index = usize::from(selector) % owners.len();

                    // Fulfil the caller's obligation to provide local or resident bytes.
                    let object_to_write = object(&bytes);
                    let resident = storage
                        .write(object_to_write, DiskRetention::Evictable)
                        .unwrap();
                    owners[index].object.pin();
                    owners[index].expected_pins += 1;
                    drop(resident);
                }
                OwnershipAction::Unpin(selector) if !owners.is_empty() => {
                    let index = usize::from(selector) % owners.len();
                    if owners[index].expected_pins > 0 {
                        owners[index].object.unpin();
                        owners[index].expected_pins -= 1;
                    }
                }
                OwnershipAction::Release(selector) if !owners.is_empty() => {
                    let index = usize::from(selector) % owners.len();
                    owners.swap_remove(index);
                }
                OwnershipAction::Reclaim => {
                    storage.reclaim().unwrap();
                    if owners.is_empty() {
                        prop_assert!(!storage.contains(key));
                        prop_assert!(local.stat(key).unwrap().is_none());
                    }
                }
                OwnershipAction::Evict => {
                    storage.evict(0).unwrap();
                    if !owners.is_empty() && owners.iter().all(|owner| owner.expected_pins == 0) {
                        prop_assert!(local.stat(key).unwrap().is_none());
                        prop_assert!(remote.stat(key).unwrap().is_some());
                    }
                }
                OwnershipAction::ReclaimWithFailedRemoval => {
                    // Removal failures preserve both catalog visibility and byte accounting.
                    let existed = storage.contains(key);
                    let usage = storage.local.budget.usage().accounted_bytes;
                    local.fail_remove.store(true, Ordering::Relaxed);
                    let result = storage.reclaim();
                    prop_assert_eq!(result.is_err(), existed && owners.is_empty());
                    prop_assert_eq!(storage.local.budget.usage().accounted_bytes, usage);
                    prop_assert_eq!(storage.contains(key), existed);
                    local.fail_remove.store(false, Ordering::Relaxed);
                }
                _ => {}
            }

            // A live owner preserves reachability; any local requirement preserves the file.
            let local_present = local.stat(key).unwrap().is_some();
            if !owners.is_empty() {
                prop_assert!(storage.contains(key));
                prop_assert!(local_present || remote.stat(key).unwrap().is_some());
            }
            if owners.iter().any(|owner| owner.expected_pins > 0) {
                prop_assert!(local_present);
            }

            let expected_usage = if local_present { bytes.len() as u64 } else { 0 };
            prop_assert_eq!(storage.local.budget.usage().accounted_bytes, expected_usage);
        }

        // Every sequence must leave no local reservation after its final release.
        drop(owners);
        storage.reclaim().unwrap();
        prop_assert!(!storage.contains(key));
        prop_assert!(local.stat(key).unwrap().is_none());
        prop_assert_eq!(storage.local.budget.usage().accounted_bytes, 0);
    }
}
