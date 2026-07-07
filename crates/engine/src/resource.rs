use spacetimedb_lib::Identity;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DatabaseMemoryType {
    /// Module instance memory such as Wasmtime linear memory and V8 physical heap memory.
    Module,
    /// Memory allocated and managed by the datastore.
    ///
    /// Currently only page-level memory.
    Datastore,
}

#[derive(Clone, Debug)]
pub struct MemoryObservation {
    pub database_identity: Identity,
    pub kind: DatabaseMemoryType,
    pub bytes: u64,
}

pub trait MemoryObserver: Send + Sync + 'static {
    fn memory_observed(&self, _: MemoryObservation) {}
}

impl MemoryObserver for () {}

#[derive(Clone)]
pub struct ModuleInstanceMemoryTracker {
    inner: Arc<ModuleInstanceMemoryTrackerInner>,
}

struct ModuleInstanceMemoryTrackerInner {
    database_identity: Identity,
    observer: Arc<dyn MemoryObserver>,
    /// Database-wide aggregate across all live module instances.
    ///
    /// Wasm and V8 instances each keep their own `last_observed` value and
    /// report only the delta when they are sampled or dropped,
    /// so that we can compare against a single limit for the entire database.
    instance_bytes: AtomicU64,
}

impl ModuleInstanceMemoryTracker {
    pub fn new(database_identity: Identity, observer: Arc<dyn MemoryObserver>) -> Self {
        Self {
            inner: Arc::new(ModuleInstanceMemoryTrackerInner {
                database_identity,
                observer,
                instance_bytes: AtomicU64::new(0),
            }),
        }
    }

    pub fn adjust_wasm_linear(&self, delta: i64) {
        self.adjust_instance(delta);
    }

    pub fn adjust_v8_physical(&self, total_physical_delta: i64) {
        self.adjust_instance(total_physical_delta);
    }

    fn adjust_instance(&self, delta: i64) {
        let bytes = adjust_atomic_u64(&self.inner.instance_bytes, delta);
        self.inner.observer.memory_observed(MemoryObservation {
            database_identity: self.inner.database_identity,
            kind: DatabaseMemoryType::Module,
            bytes,
        });
    }
}

fn adjust_atomic_u64(value: &AtomicU64, delta: i64) -> u64 {
    if delta >= 0 {
        let delta = delta as u64;
        value.fetch_add(delta, Ordering::Relaxed) + delta
    } else {
        let delta = delta.unsigned_abs();
        value.fetch_sub(delta, Ordering::Relaxed) - delta
    }
}
