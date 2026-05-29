use crate::messages::control_db::Database;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DatabaseResourceLimits {
    pub memory_limit_bytes: Option<usize>,
}

impl DatabaseResourceLimits {
    pub const UNLIMITED: Self = Self {
        memory_limit_bytes: None,
    };

    pub fn with_memory_limit(memory_limit_bytes: Option<usize>) -> Self {
        Self { memory_limit_bytes }
    }
}

/// Supplies resource limits for a database at host initialization time.
///
/// The current cloud configuration uses a fixed global limit for every database.
/// Keeping this as a provider lets cloud switch to per-database limits later
/// without changing `RelationalDB` or the module runtime accounting paths.
pub trait DatabaseResourceLimitsProvider: Send + Sync + 'static {
    fn limits_for_database(&self, database: &Database) -> DatabaseResourceLimits;
}

#[derive(Clone, Debug)]
pub struct FixedDatabaseResourceLimits {
    limits: DatabaseResourceLimits,
}

impl FixedDatabaseResourceLimits {
    pub fn new(limits: DatabaseResourceLimits) -> Self {
        Self { limits }
    }

    pub fn with_memory_limit(memory_limit_bytes: Option<usize>) -> Self {
        Self::new(DatabaseResourceLimits::with_memory_limit(memory_limit_bytes))
    }
}

impl Default for FixedDatabaseResourceLimits {
    fn default() -> Self {
        Self::new(DatabaseResourceLimits::UNLIMITED)
    }
}

impl DatabaseResourceLimitsProvider for FixedDatabaseResourceLimits {
    fn limits_for_database(&self, _database: &Database) -> DatabaseResourceLimits {
        self.limits
    }
}

/// Tracks the memory charged against a database.
///
/// The relational component is computed at commit time from the datastore.
/// Runtime memory is accounted incrementally by each module runtime instance
/// and aggregated here so commit-time checks can see the full database budget.
#[derive(Debug)]
pub struct DatabaseMemoryBudget {
    limit_bytes: Option<usize>,
    relational_bytes: AtomicUsize,
    wasm_runtime_bytes: AtomicUsize,
    v8_runtime_bytes: AtomicUsize,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("database memory limit exceeded: used {used_bytes} bytes exceeds limit {limit_bytes} bytes")]
pub struct DatabaseMemoryLimitExceeded {
    pub used_bytes: usize,
    pub limit_bytes: usize,
}

impl DatabaseMemoryBudget {
    pub(crate) fn new(limit_bytes: Option<usize>) -> Self {
        Self {
            limit_bytes,
            relational_bytes: AtomicUsize::new(0),
            wasm_runtime_bytes: AtomicUsize::new(0),
            v8_runtime_bytes: AtomicUsize::new(0),
        }
    }

    pub(crate) fn unlimited() -> Self {
        Self::new(None)
    }

    fn runtime_bytes(&self) -> usize {
        self.wasm_runtime_bytes().saturating_add(self.v8_runtime_bytes())
    }

    fn relational_bytes(&self) -> usize {
        self.relational_bytes.load(Ordering::Relaxed)
    }

    pub(crate) fn set_relational_bytes(&self, bytes: usize) {
        self.relational_bytes.store(bytes, Ordering::Relaxed);
    }

    fn wasm_runtime_bytes(&self) -> usize {
        self.wasm_runtime_bytes.load(Ordering::Relaxed)
    }

    fn v8_runtime_bytes(&self) -> usize {
        self.v8_runtime_bytes.load(Ordering::Relaxed)
    }

    pub(crate) fn check(&self, relational_bytes: usize) -> Result<(), DatabaseMemoryLimitExceeded> {
        self.check_used(relational_bytes.saturating_add(self.runtime_bytes()))
    }

    /// Reserve wasm runtime bytes, rejecting the reservation when wasm memory
    /// alone would exceed the database limit.
    ///
    /// This intentionally does not synchronously read relational memory. Wasm
    /// growth can happen while a mutable tx is open, and reading the datastore's
    /// committed memory from there can deadlock. The aggregate relational +
    /// runtime budget is enforced at commit time.
    pub(crate) fn try_reserve_wasm_bytes(&self, bytes: usize) -> Result<(), DatabaseMemoryLimitExceeded> {
        if bytes == 0 {
            return Ok(());
        }

        let previous = self.wasm_runtime_bytes.fetch_add(bytes, Ordering::Relaxed);
        let used = self
            .relational_bytes()
            .saturating_add(previous)
            .saturating_add(bytes)
            .saturating_add(self.v8_runtime_bytes());
        match self.check_used(used) {
            Ok(()) => Ok(()),
            Err(err) => {
                self.release_wasm_bytes(bytes);
                Err(err)
            }
        }
    }

    pub(crate) fn release_wasm_bytes(&self, bytes: usize) {
        saturating_sub(&self.wasm_runtime_bytes, bytes);
    }

    pub(crate) fn adjust_v8_bytes(&self, delta: i64) {
        adjust_counter(&self.v8_runtime_bytes, delta);
    }

    fn check_used(&self, used_bytes: usize) -> Result<(), DatabaseMemoryLimitExceeded> {
        match self.limit_bytes {
            Some(limit_bytes) if used_bytes > limit_bytes => Err(DatabaseMemoryLimitExceeded {
                used_bytes,
                limit_bytes,
            }),
            _ => Ok(()),
        }
    }
}

fn adjust_counter(counter: &AtomicUsize, delta: i64) {
    if delta > 0 {
        counter.fetch_add(delta as usize, Ordering::Relaxed);
    } else if delta < 0 {
        saturating_sub(counter, delta.unsigned_abs() as usize);
    }
}

fn saturating_sub(counter: &AtomicUsize, bytes: usize) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_sub(bytes))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_includes_runtime_memory() {
        let budget = DatabaseMemoryBudget::new(Some(100));

        budget.set_relational_bytes(20);
        budget.try_reserve_wasm_bytes(40).unwrap();
        budget.adjust_v8_bytes(20);

        assert!(budget.check(40).is_ok());
        let err = budget.check(41).unwrap_err();
        assert_eq!(err.used_bytes, 101);
        assert_eq!(err.limit_bytes, 100);
    }

    #[test]
    fn rejected_wasm_reservation_is_not_retained() {
        let budget = DatabaseMemoryBudget::new(Some(100));

        budget.try_reserve_wasm_bytes(90).unwrap();
        assert!(budget.try_reserve_wasm_bytes(11).is_err());

        assert_eq!(budget.wasm_runtime_bytes(), 90);
    }

    #[test]
    fn wasm_reservation_checks_cached_relational_memory() {
        let budget = DatabaseMemoryBudget::new(Some(100));

        budget.set_relational_bytes(90);

        let err = budget.try_reserve_wasm_bytes(11).unwrap_err();
        assert_eq!(err.used_bytes, 101);
        assert_eq!(err.limit_bytes, 100);
        assert_eq!(budget.wasm_runtime_bytes(), 0);
    }
}
