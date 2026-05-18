//! Shared storage fault-injection primitives for DST simulation helpers.
//!
//! Fault decisions use [`spacetimedb_runtime::sim::Handle::buggify_with_prob`]
//! so they are gated by the runtime's centralized buggify flag.

use std::{
    io,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use crate::config::CommitlogFaultProfile;

const INJECTED_ERROR_PREFIX: &str = "dst injected ";

pub(crate) fn is_injected_fault_text(domain: StorageFaultDomain, text: &str) -> bool {
    text.contains(&format!("{INJECTED_ERROR_PREFIX}{} ", domain.label()))
}

/// API-level storage fault profile for DST-only storage wrappers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StorageFaultConfig {
    pub(crate) profile: CommitlogFaultProfile,
    pub(crate) latency_prob: f64,
    pub(crate) long_latency_prob: f64,
    pub(crate) short_io_prob: f64,
    pub(crate) read_error_prob: f64,
    pub(crate) write_error_prob: f64,
    pub(crate) flush_error_prob: f64,
    pub(crate) fsync_error_prob: f64,
    pub(crate) open_error_prob: f64,
    pub(crate) metadata_error_prob: f64,
    pub(crate) max_short_io_divisor: usize,
    pub(crate) no_space_prob: f64,
    pub(crate) partial_failure_prob: f64,
}

impl StorageFaultConfig {
    pub(crate) fn for_profile(profile: CommitlogFaultProfile) -> Self {
        match profile {
            CommitlogFaultProfile::Off => Self {
                profile,
                latency_prob: 0.0,
                long_latency_prob: 0.0,
                short_io_prob: 0.0,
                read_error_prob: 0.0,
                write_error_prob: 0.0,
                flush_error_prob: 0.0,
                fsync_error_prob: 0.0,
                open_error_prob: 0.0,
                metadata_error_prob: 0.0,
                max_short_io_divisor: 2,
                no_space_prob: 0.0,
                partial_failure_prob: 0.0,
            },
            // Realistic rare faults: ~1 in 1000 latency, ~1 in 10000 short I/O / errors.
            CommitlogFaultProfile::Light => Self {
                profile,
                latency_prob: 0.001,
                long_latency_prob: 0.0001,
                short_io_prob: 0.0001,
                read_error_prob: 0.0001,
                write_error_prob: 0.0001,
                flush_error_prob: 0.0001,
                fsync_error_prob: 0.0001,
                open_error_prob: 0.0001,
                metadata_error_prob: 0.0001,
                max_short_io_divisor: 2,
                no_space_prob: 0.0001,
                partial_failure_prob: 0.0001,
            },
            // Moderate rare faults: ~1 in 500 latency, ~1 in 5000 short I/O / errors.
            CommitlogFaultProfile::Default => Self {
                profile,
                latency_prob: 0.002,
                long_latency_prob: 0.0002,
                short_io_prob: 0.0002,
                read_error_prob: 0.0002,
                write_error_prob: 0.0002,
                flush_error_prob: 0.0002,
                fsync_error_prob: 0.0002,
                open_error_prob: 0.0002,
                metadata_error_prob: 0.0002,
                max_short_io_divisor: 2,
                no_space_prob: 0.0002,
                partial_failure_prob: 0.0002,
            },
            // Stress test: ~1 in 10 operations see a fault.
            CommitlogFaultProfile::Aggressive => Self {
                profile,
                latency_prob: 0.10,
                long_latency_prob: 0.02,
                short_io_prob: 0.02,
                read_error_prob: 0.01,
                write_error_prob: 0.01,
                flush_error_prob: 0.01,
                fsync_error_prob: 0.01,
                open_error_prob: 0.01,
                metadata_error_prob: 0.01,
                max_short_io_divisor: 2,
                no_space_prob: 0.01,
                partial_failure_prob: 0.01,
            },
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct StorageFaultSummary {
    pub(crate) profile: CommitlogFaultProfile,
    pub(crate) latency: usize,
    pub(crate) short_read: usize,
    pub(crate) short_write: usize,
    pub(crate) read_error: usize,
    pub(crate) write_error: usize,
    pub(crate) flush_error: usize,
    pub(crate) fsync_error: usize,
    pub(crate) open_error: usize,
    pub(crate) metadata_error: usize,
    pub(crate) no_space: usize,
    pub(crate) partial_failure: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum StorageFaultDomain {
    Disk,
    Snapshot,
}

impl StorageFaultDomain {
    fn label(self) -> &'static str {
        match self {
            Self::Disk => "disk",
            Self::Snapshot => "snapshot",
        }
    }
}

#[derive(Clone)]
pub(crate) struct StorageFaultController {
    config: StorageFaultConfig,
    domain: StorageFaultDomain,
    counters: Arc<FaultCounters>,
    handle: Option<spacetimedb_runtime::sim::Handle>,
    suspended: Arc<AtomicUsize>,
}

impl StorageFaultController {
    pub(crate) fn new(config: StorageFaultConfig, domain: StorageFaultDomain) -> Self {
        Self {
            config,
            domain,
            counters: Arc::default(),
            handle: crate::sim::current_handle(),
            suspended: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub(crate) fn enable(&self) {
        if let Some(handle) = &self.handle {
            handle.enable_buggify();
        }
    }

    pub(crate) fn with_suspended<T>(&self, f: impl FnOnce() -> T) -> T {
        self.suspended.fetch_add(1, Ordering::Relaxed);
        let _guard = SuspendFaultsGuard {
            suspended: self.suspended.clone(),
        };
        f()
    }

    pub(crate) fn maybe_latency(&self) {
        if self.sample_latency(self.config.latency_prob) {
            self.counters.latency.fetch_add(1, Ordering::Relaxed);
            let latency = if self.sample_latency(self.config.long_latency_prob) {
                Duration::from_millis(25)
            } else {
                Duration::from_millis(1)
            };
            if let Some(handle) = &self.handle {
                handle.advance(latency);
            }
        }
    }

    pub(crate) fn maybe_error(&self, kind: StorageFaultKind) -> io::Result<()> {
        let prob = kind.probability(&self.config);
        if self.sample(prob) {
            kind.counter(&self.counters).fetch_add(1, Ordering::Relaxed);
            return Err(io::Error::new(kind.error_kind(), kind.message(self.domain)));
        }
        Ok(())
    }

    pub(crate) fn check_pending_error(&self, kind: StorageFaultKind) -> io::Result<()> {
        if self.counters.pending_error.swap(false, Ordering::Relaxed) {
            kind.counter(&self.counters).fetch_add(1, Ordering::Relaxed);
            self.counters.partial_failure.fetch_add(1, Ordering::Relaxed);
            return Err(io::Error::new(kind.error_kind(), kind.message(self.domain)));
        }
        Ok(())
    }

    pub(crate) fn arm_pending_error(&self) {
        self.counters.pending_error.store(true, Ordering::Relaxed);
    }

    pub(crate) fn sample_partial_failure(&self) -> bool {
        if !self.active() || self.config.partial_failure_prob <= 0.0 {
            return false;
        }
        match &self.handle {
            Some(handle) => handle.buggify_with_prob(self.config.partial_failure_prob),
            None => false,
        }
    }

    pub(crate) fn maybe_short_len(&self, len: usize, kind: ShortIoKind) -> usize {
        if len <= 1 {
            return len;
        }
        if !self.sample(self.config.short_io_prob) {
            return len;
        }
        kind.counter(&self.counters).fetch_add(1, Ordering::Relaxed);
        let divisor = self.config.max_short_io_divisor.max(2);
        (len / divisor).max(1)
    }

    pub(crate) fn summary(&self) -> StorageFaultSummary {
        StorageFaultSummary {
            profile: self.config.profile,
            latency: self.counters.latency.load(Ordering::Relaxed) as usize,
            short_read: self.counters.short_read.load(Ordering::Relaxed) as usize,
            short_write: self.counters.short_write.load(Ordering::Relaxed) as usize,
            read_error: self.counters.read_error.load(Ordering::Relaxed) as usize,
            write_error: self.counters.write_error.load(Ordering::Relaxed) as usize,
            flush_error: self.counters.flush_error.load(Ordering::Relaxed) as usize,
            fsync_error: self.counters.fsync_error.load(Ordering::Relaxed) as usize,
            open_error: self.counters.open_error.load(Ordering::Relaxed) as usize,
            metadata_error: self.counters.metadata_error.load(Ordering::Relaxed) as usize,
            no_space: self.counters.no_space.load(Ordering::Relaxed) as usize,
            partial_failure: self.counters.partial_failure.load(Ordering::Relaxed) as usize,
        }
    }

    fn active(&self) -> bool {
        self.suspended.load(Ordering::Relaxed) == 0
    }

    fn sample(&self, probability: f64) -> bool {
        if probability <= 0.0 || !self.active() {
            return false;
        }
        match &self.handle {
            Some(handle) => handle.buggify_with_prob(probability),
            None => false,
        }
    }

    fn sample_latency(&self, probability: f64) -> bool {
        if probability <= 0.0 {
            return false;
        }
        match &self.handle {
            Some(handle) => handle.buggify_with_prob(probability),
            None => false,
        }
    }
}

struct SuspendFaultsGuard {
    suspended: Arc<AtomicUsize>,
}

impl Drop for SuspendFaultsGuard {
    fn drop(&mut self) {
        self.suspended.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Default)]
struct FaultCounters {
    latency: AtomicU64,
    short_read: AtomicU64,
    short_write: AtomicU64,
    read_error: AtomicU64,
    write_error: AtomicU64,
    flush_error: AtomicU64,
    fsync_error: AtomicU64,
    open_error: AtomicU64,
    metadata_error: AtomicU64,
    no_space: AtomicU64,
    partial_failure: AtomicU64,
    pending_error: AtomicBool,
}

#[derive(Clone, Copy)]
pub(crate) enum ShortIoKind {
    Read,
    Write,
}

impl ShortIoKind {
    fn counter(self, counters: &FaultCounters) -> &AtomicU64 {
        match self {
            Self::Read => &counters.short_read,
            Self::Write => &counters.short_write,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum StorageFaultKind {
    Read,
    Write,
    Flush,
    Fsync,
    Open,
    Metadata,
    NoSpace,
}

impl StorageFaultKind {
    fn probability(self, config: &StorageFaultConfig) -> f64 {
        match self {
            Self::Read => config.read_error_prob,
            Self::Write => config.write_error_prob,
            Self::Flush => config.flush_error_prob,
            Self::Fsync => config.fsync_error_prob,
            Self::Open => config.open_error_prob,
            Self::Metadata => config.metadata_error_prob,
            Self::NoSpace => config.no_space_prob,
        }
    }

    fn counter(self, counters: &FaultCounters) -> &AtomicU64 {
        match self {
            Self::Read => &counters.read_error,
            Self::Write => &counters.write_error,
            Self::Flush => &counters.flush_error,
            Self::Fsync => &counters.fsync_error,
            Self::Open => &counters.open_error,
            Self::Metadata => &counters.metadata_error,
            Self::NoSpace => &counters.no_space,
        }
    }

    fn error_kind(self) -> io::ErrorKind {
        match self {
            Self::NoSpace => io::ErrorKind::StorageFull,
            _ => io::ErrorKind::Other,
        }
    }

    fn message(self, domain: StorageFaultDomain) -> String {
        let label = domain.label();
        match self {
            Self::Read => format!("{INJECTED_ERROR_PREFIX}{label} input/output error"),
            Self::Write => format!("{INJECTED_ERROR_PREFIX}{label} input/output error"),
            Self::Flush => format!("{INJECTED_ERROR_PREFIX}{label} input/output error"),
            Self::Fsync => format!("{INJECTED_ERROR_PREFIX}{label} input/output error"),
            Self::Open => format!("{INJECTED_ERROR_PREFIX}{label} input/output error"),
            Self::Metadata => format!("{INJECTED_ERROR_PREFIX}{label} input/output error"),
            Self::NoSpace => format!("{INJECTED_ERROR_PREFIX}{label} no space left on device"),
        }
    }
}
