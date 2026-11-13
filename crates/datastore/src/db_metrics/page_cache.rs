use super::DB_METRICS;
use prometheus::{Histogram, IntCounter, IntGauge};
use spacetimedb_lib::Identity;
use spacetimedb_table::tiered::{
    metrics::{
        CacheTier, DiskOperation, EvictionDirection, FaultSource, Metrics, ObjectType, OperationMetrics, StorageMetrics,
    },
    TieredDatabaseStorage,
};
use std::sync::OnceLock;

#[derive(Debug, Default)]
struct PageCacheMetrics {
    pages: ObjectCacheMetrics,
    blobs: ObjectCacheMetrics,
    faults: FaultMetrics,
    memory: TierMetrics,
    disk: TierMetrics,
    disk_io: DiskIoMetrics,
}

#[derive(Debug, Default)]
struct ObjectCacheMetrics {
    memory_lookups: LookupCounters,
    disk_lookups: LookupCounters,
    memory_to_disk: EvictionCounters,
    disk_to_remote: EvictionCounters,
}

#[derive(Debug, Default)]
struct FaultMetrics {
    in_flight: OnceLock<IntGauge>,
    pages: FaultTimings,
    blobs: FaultTimings,
}

#[derive(Debug, Default)]
struct FaultTimings {
    disk: FaultResults,
    remote: FaultResults,
    no_source: FaultResults,
}

#[derive(Debug, Default)]
struct FaultResults {
    success: OnceLock<Histogram>,
    failure: OnceLock<Histogram>,
}

#[derive(Debug, Default)]
struct TierMetrics {
    budget_rejections: OnceLock<IntCounter>,
    eviction_time: OnceLock<Histogram>,
}

#[derive(Debug, Default)]
struct DiskIoMetrics {
    read: OnceLock<Histogram>,
    write: OnceLock<Histogram>,
    read_bytes: OnceLock<IntCounter>,
    write_bytes: OnceLock<IntCounter>,
}

#[derive(Debug, Default)]
struct LookupCounters {
    hits: OnceLock<IntCounter>,
    misses: OnceLock<IntCounter>,
}

#[derive(Debug, Default)]
struct EvictionCounters {
    objects: OnceLock<IntCounter>,
    bytes: OnceLock<IntCounter>,
}

impl PageCacheMetrics {
    fn object_metrics(&self, object: ObjectType) -> &ObjectCacheMetrics {
        match object {
            ObjectType::Page => &self.pages,
            ObjectType::Blob => &self.blobs,
        }
    }

    fn tier_metrics(&self, tier: CacheTier) -> &TierMetrics {
        match tier {
            CacheTier::Memory => &self.memory,
            CacheTier::Disk => &self.disk,
        }
    }

    fn record(&self, stats: &StorageMetrics, event: Option<OperationMetrics>) {
        let Some(event) = event else {
            remove_series(stats);
            return;
        };
        let (db, replica) = (&stats.database, &stats.replica);
        match event {
            OperationMetrics::Lookup { object, tier, hit } => {
                self.object_metrics(object).lookup(stats, object, tier, hit);
            }
            OperationMetrics::BudgetRejected { tier } => {
                self.tier_metrics(tier)
                    .budget_rejections
                    .get_or_init(|| {
                        DB_METRICS
                            .page_cache_rejections
                            .with_label_values(db, replica, tier.as_str())
                    })
                    .inc();
            }
            OperationMetrics::Evicted {
                object,
                direction,
                bytes,
            } => {
                self.object_metrics(object).evicted(stats, object, direction, bytes);
            }
            OperationMetrics::FaultStarted => self.faults.in_flight(stats).inc(),
            OperationMetrics::FaultFinished {
                object,
                source,
                success,
                nanos,
            } => {
                self.faults.record(stats, object, source, success, nanos);
            }
            OperationMetrics::Eviction { direction, nanos } => {
                let metrics = match direction {
                    EvictionDirection::MemoryToDisk => &self.memory,
                    EvictionDirection::DiskToRemote => &self.disk,
                };
                metrics
                    .eviction_time
                    .get_or_init(|| {
                        DB_METRICS
                            .page_cache_eviction_time
                            .with_label_values(db, replica, direction.as_str())
                    })
                    .observe(nanos as f64 / 1e9);
            }
            OperationMetrics::DiskIo {
                operation,
                nanos,
                bytes,
            } => {
                self.disk_io.record(stats, operation, nanos, bytes);
            }
        }
    }
}

impl ObjectCacheMetrics {
    fn lookup(&self, stats: &StorageMetrics, object: ObjectType, tier: CacheTier, hit: bool) {
        let lookups = match tier {
            CacheTier::Memory => &self.memory_lookups,
            CacheTier::Disk => &self.disk_lookups,
        };
        let counter = if hit { &lookups.hits } else { &lookups.misses };
        counter
            .get_or_init(|| {
                DB_METRICS.page_cache_lookups.with_label_values(
                    &stats.database,
                    &stats.replica,
                    object.as_str(),
                    tier.as_str(),
                    &hit,
                )
            })
            .inc();
    }

    fn evicted(&self, stats: &StorageMetrics, object: ObjectType, direction: EvictionDirection, bytes: u64) {
        let evicted = match direction {
            EvictionDirection::MemoryToDisk => &self.memory_to_disk,
            EvictionDirection::DiskToRemote => &self.disk_to_remote,
        };
        let (db, replica) = (&stats.database, &stats.replica);
        evicted
            .objects
            .get_or_init(|| {
                DB_METRICS.page_cache_evicted_objects.with_label_values(
                    db,
                    replica,
                    object.as_str(),
                    direction.as_str(),
                )
            })
            .inc();
        evicted
            .bytes
            .get_or_init(|| {
                DB_METRICS
                    .page_cache_evicted_bytes
                    .with_label_values(db, replica, object.as_str(), direction.as_str())
            })
            .inc_by(bytes);
    }
}

impl FaultMetrics {
    fn in_flight(&self, stats: &StorageMetrics) -> &IntGauge {
        self.in_flight.get_or_init(|| {
            DB_METRICS
                .page_cache_in_flight
                .with_label_values(&stats.database, &stats.replica)
        })
    }

    fn record(
        &self,
        stats: &StorageMetrics,
        object: ObjectType,
        source: Option<FaultSource>,
        success: bool,
        nanos: u64,
    ) {
        self.in_flight(stats).dec();
        // A retry may find a page loaded by another caller, without reading backing storage.
        // For example, a concurrent read-only transaction may load the page before this retry.
        if success && source.is_none() {
            return;
        }
        let times = match object {
            ObjectType::Page => &self.pages,
            ObjectType::Blob => &self.blobs,
        };
        let results = match source {
            Some(FaultSource::Disk) => &times.disk,
            Some(FaultSource::Remote) => &times.remote,
            None => &times.no_source,
        };
        let histogram = if success { &results.success } else { &results.failure };
        histogram
            .get_or_init(|| {
                DB_METRICS.page_cache_faults.with_label_values(
                    &stats.database,
                    &stats.replica,
                    object.as_str(),
                    source.map_or("", FaultSource::as_str),
                    &success,
                )
            })
            .observe(nanos as f64 / 1e9);
    }
}

impl DiskIoMetrics {
    fn record(&self, stats: &StorageMetrics, operation: DiskOperation, nanos: u64, bytes: u64) {
        let (duration, transferred_bytes) = match operation {
            DiskOperation::Read => (&self.read, &self.read_bytes),
            DiskOperation::Write => (&self.write, &self.write_bytes),
        };
        let (db, replica) = (&stats.database, &stats.replica);
        duration
            .get_or_init(|| {
                DB_METRICS
                    .page_cache_disk_time
                    .with_label_values(db, replica, operation.as_str())
            })
            .observe(nanos as f64 / 1e9);
        if bytes > 0 {
            transferred_bytes
                .get_or_init(|| {
                    DB_METRICS
                        .page_cache_disk_bytes
                        .with_label_values(db, replica, operation.as_str())
                })
                .inc_by(bytes);
        }
    }
}

fn remove_series(stats: &StorageMetrics) {
    let (db, replica) = (&stats.database, &stats.replica);
    let _ = DB_METRICS.page_cache_in_flight.remove_label_values(db, replica);
    for tier in [CacheTier::Memory, CacheTier::Disk] {
        let _ = DB_METRICS
            .page_cache_budget_usage
            .remove_label_values(db, replica, tier.as_str());
        let _ = DB_METRICS
            .page_cache_budget_limit
            .remove_label_values(db, replica, tier.as_str());
        let _ = DB_METRICS
            .page_cache_rejections
            .remove_label_values(db, replica, tier.as_str());
    }
    for object in [ObjectType::Page, ObjectType::Blob] {
        for tier in [CacheTier::Memory, CacheTier::Disk] {
            for hit in [true, false] {
                let _ = DB_METRICS.page_cache_lookups.remove_label_values(
                    db,
                    replica,
                    object.as_str(),
                    tier.as_str(),
                    &hit,
                );
            }
        }
        for source in ["", "disk", "remote"] {
            for success in [true, false] {
                let _ =
                    DB_METRICS
                        .page_cache_faults
                        .remove_label_values(db, replica, object.as_str(), source, &success);
            }
        }
        for direction in [EvictionDirection::MemoryToDisk, EvictionDirection::DiskToRemote] {
            let _ = DB_METRICS.page_cache_evicted_objects.remove_label_values(
                db,
                replica,
                object.as_str(),
                direction.as_str(),
            );
            let _ = DB_METRICS.page_cache_evicted_bytes.remove_label_values(
                db,
                replica,
                object.as_str(),
                direction.as_str(),
            );
        }
    }
    for direction in [EvictionDirection::MemoryToDisk, EvictionDirection::DiskToRemote] {
        let _ = DB_METRICS
            .page_cache_eviction_time
            .remove_label_values(db, replica, direction.as_str());
    }
    for operation in [DiskOperation::Read, DiskOperation::Write] {
        let _ = DB_METRICS
            .page_cache_disk_time
            .remove_label_values(db, replica, operation.as_str());
        let _ = DB_METRICS
            .page_cache_disk_bytes
            .remove_label_values(db, replica, operation.as_str());
    }
}

pub fn new_metrics(database: Identity, replica: u64, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Metrics {
    let recorder = PageCacheMetrics::default();
    Metrics::new(database, replica, clock, move |stats, event| {
        recorder.record(stats, event)
    })
}

pub fn report(storage: &TieredDatabaseStorage) {
    let Some(stats) = storage.metrics().collected() else {
        return;
    };
    let db = &stats.database;
    let replica = &stats.replica;
    for (tier, budget) in storage.budgets() {
        let tier = tier.as_str();
        DB_METRICS
            .page_cache_budget_usage
            .with_label_values(db, replica, tier)
            .set(budget.usage().accounted_bytes as f64);
        DB_METRICS
            .page_cache_budget_limit
            .with_label_values(db, replica, tier)
            .set(budget.config().hard_limit_bytes as f64);
    }
}
