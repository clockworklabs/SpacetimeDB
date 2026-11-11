//! The host supplies a monotonic nanosecond clock; the datastore owns Prometheus.
use spacetimedb_lib::Identity;
use std::{fmt, sync::Arc};

mod labels;
pub use labels::{CacheTier, DiskOperation, EvictionDirection, FaultSource, ObjectType};

#[derive(Debug)]
pub enum OperationMetrics {
    Lookup {
        object: ObjectType,
        tier: CacheTier,
        hit: bool,
    },
    BudgetRejected {
        tier: CacheTier,
    },
    Evicted {
        object: ObjectType,
        direction: EvictionDirection,
        bytes: u64,
    },
    FaultStarted,
    FaultFinished {
        object: ObjectType,
        source: Option<FaultSource>,
        success: bool,
        nanos: u64,
    },
    Eviction {
        direction: EvictionDirection,
        nanos: u64,
    },
    DiskIo {
        bytes: u64,
        operation: DiskOperation,
        nanos: u64,
    },
}

type Publish = dyn Fn(&StorageMetrics, Option<OperationMetrics>) + Send + Sync;

pub struct StorageMetrics {
    pub database: Identity,
    pub replica: u64,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
    publish: Box<Publish>,
}
impl Drop for StorageMetrics {
    fn drop(&mut self) {
        (self.publish)(self, None);
    }
}

#[derive(Clone, Default)]
pub struct Metrics(Option<Arc<StorageMetrics>>);
impl fmt::Debug for Metrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Metrics").field("enabled", &self.0.is_some()).finish()
    }
}
impl Metrics {
    pub fn new(
        database: Identity,
        replica: u64,
        clock: impl Fn() -> u64 + Send + Sync + 'static,
        publish: impl Fn(&StorageMetrics, Option<OperationMetrics>) + Send + Sync + 'static,
    ) -> Self {
        Self(Some(Arc::new(StorageMetrics {
            database,
            replica,
            clock: Box::new(clock),
            publish: Box::new(publish),
        })))
    }
    pub fn collected(&self) -> Option<&StorageMetrics> {
        self.0.as_deref()
    }
    fn record(&self, observation: OperationMetrics) {
        if let Some(stats) = &self.0 {
            (stats.publish)(stats, Some(observation));
        }
    }
    pub fn lookup(&self, object: ObjectType, tier: CacheTier, hit: bool) {
        self.record(OperationMetrics::Lookup { object, tier, hit });
    }
    pub fn budget_rejected(&self, tier: CacheTier) {
        self.record(OperationMetrics::BudgetRejected { tier });
    }
    pub fn evicted(&self, object: ObjectType, direction: EvictionDirection, bytes: u64) {
        self.record(OperationMetrics::Evicted {
            object,
            direction,
            bytes,
        });
    }
    pub fn eviction(&self, direction: EvictionDirection, nanos: u64) {
        self.record(OperationMetrics::Eviction { direction, nanos });
    }
    pub fn disk_io(&self, operation: DiskOperation, nanos: u64, bytes: u64) {
        self.record(OperationMetrics::DiskIo {
            bytes,
            operation,
            nanos,
        });
    }
    pub fn timer(&self) -> Timer<'_> {
        Timer {
            metrics: self,
            start: self.0.as_ref().map(|stats| (stats.clock)()),
        }
    }
    pub fn fault(&self, object: ObjectType) -> Fault<'_> {
        self.record(OperationMetrics::FaultStarted);
        Fault {
            timer: self.timer(),
            object,
            source: None,
            success: false,
        }
    }
}

pub struct Timer<'a> {
    metrics: &'a Metrics,
    start: Option<u64>,
}
impl Timer<'_> {
    pub fn nanos(&self) -> u64 {
        self.start
            .zip(self.metrics.0.as_ref())
            .map_or(0, |(start, stats)| (stats.clock)().saturating_sub(start))
    }
}

pub struct Fault<'a> {
    timer: Timer<'a>,
    object: ObjectType,
    pub(super) source: Option<FaultSource>,
    pub(super) success: bool,
}
impl Drop for Fault<'_> {
    fn drop(&mut self) {
        self.timer.metrics.record(OperationMetrics::FaultFinished {
            object: self.object,
            source: self.source,
            success: self.success,
            nanos: self.timer.nanos(),
        });
    }
}

// Keep one fault guard across retries; errors and unwinding leave success=false.
pub(super) fn with_fault<'a, T, E>(f: impl FnOnce(&mut Option<Fault<'a>>) -> Result<T, E>) -> Result<T, E> {
    let mut fault = None;
    let result = f(&mut fault);
    if let Some(fault) = &mut fault {
        fault.success = result.is_ok();
    }
    result
}
