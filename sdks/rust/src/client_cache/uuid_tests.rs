use super::*;
use spacetimedb_lib::Uuid;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct CountedRow {
    id: Uuid,
    value: u64,
    clones: Arc<AtomicUsize>,
}
impl Clone for CountedRow {
    fn clone(&self) -> Self {
        self.clones.fetch_add(1, Ordering::SeqCst);
        Self {
            id: self.id,
            value: self.value,
            clones: self.clones.clone(),
        }
    }
}

fn row(id: Uuid, value: u64, clones: &Arc<AtomicUsize>) -> WithBsatn<CountedRow> {
    WithBsatn {
        bsatn: spacetimedb_lib::bsatn::to_vec(&(id, value)).unwrap().into(),
        row: CountedRow {
            id,
            value,
            clones: clones.clone(),
        },
    }
}

#[test]
fn uuid_unique_lookup_clones_only_the_match_and_tracks_updates_and_deletes() {
    let clones = Arc::new(AtomicUsize::new(0));
    let mut cache = TableCache::<CountedRow>::new(None);
    // This is the exact registration emitted by Rust codegen.
    cache.add_unique_constraint::<Uuid>("id", |row| &row.id);
    let ids = (0u8..128)
        .map(|n| Uuid::from_random_bytes_v4([n; 16]))
        .collect::<Vec<_>>();
    cache.apply_diff(&TableUpdate {
        inserts: ids
            .iter()
            .enumerate()
            .map(|(n, id)| row(*id, n as u64, &clones))
            .collect(),
        deletes: vec![],
    });
    clones.store(0, Ordering::SeqCst);
    // UniqueConstraintHandle::find delegates to this lookup and clones only
    // its selected row. A whole-cache snapshot would increment this 128 times.
    let found = cache.find_by_unique_index("id", &ids[63]).cloned().unwrap();
    assert_eq!((found.id, found.value), (ids[63], 63));
    assert_eq!(clones.load(Ordering::SeqCst), 1);
    let absent = Uuid::from_random_bytes_v4([255; 16]);
    assert!(cache.find_by_unique_index("id", &absent).cloned().is_none());
    assert_eq!(clones.load(Ordering::SeqCst), 1);

    cache.apply_diff(&TableUpdate {
        inserts: vec![row(ids[63], 999, &clones)],
        deletes: vec![row(ids[63], 63, &clones)],
    });
    clones.store(0, Ordering::SeqCst);
    assert_eq!(cache.find_by_unique_index("id", &ids[63]).cloned().unwrap().value, 999);
    assert_eq!(clones.load(Ordering::SeqCst), 1);
    cache.apply_diff(&TableUpdate {
        inserts: vec![],
        deletes: vec![row(ids[63], 999, &clones)],
    });
    clones.store(0, Ordering::SeqCst);
    assert!(cache.find_by_unique_index("id", &ids[63]).cloned().is_none());
    assert_eq!(clones.load(Ordering::SeqCst), 0);
    assert_eq!(cache.find_by_unique_index("id", &ids[64]).unwrap().value, 64);
}
