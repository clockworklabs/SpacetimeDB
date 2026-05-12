//! Shared storage fault-injection primitives for DST simulation helpers.

use std::{
    io,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use crate::{config::CommitlogFaultProfile, seed::DstSeed, sim};

const INJECTED_ERROR_PREFIX: &str = "dst injected ";

pub(crate) fn is_injected_fault_text(domain: StorageFaultDomain, text: &str) -> bool {
    text.contains(&format!("{INJECTED_ERROR_PREFIX}{} ", domain.label()))
}

/// API-level storage fault profile for DST-only storage wrappers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StorageFaultConfig {
    pub(crate) profile: CommitlogFaultProfile,
    pub(crate) enabled: bool,
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
}

impl StorageFaultConfig {
    pub(crate) fn for_profile(profile: CommitlogFaultProfile) -> Self {
        match profile {
            CommitlogFaultProfile::Off => Self {
                profile,
                enabled: false,
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
            },
            CommitlogFaultProfile::Light => Self {
                profile,
                enabled: true,
                latency_prob: 0.20,
                long_latency_prob: 0.04,
                short_io_prob: 0.03,
                read_error_prob: 0.0,
                write_error_prob: 0.0,
                flush_error_prob: 0.0,
                fsync_error_prob: 0.0,
                open_error_prob: 0.0,
                metadata_error_prob: 0.0,
                max_short_io_divisor: 2,
            },
            CommitlogFaultProfile::Default => Self {
                profile,
                enabled: true,
                latency_prob: 0.35,
                long_latency_prob: 0.08,
                short_io_prob: 0.08,
                read_error_prob: 0.0,
                write_error_prob: 0.0,
                flush_error_prob: 0.0,
                fsync_error_prob: 0.0,
                open_error_prob: 0.0,
                metadata_error_prob: 0.0,
                max_short_io_divisor: 2,
            },
            CommitlogFaultProfile::Aggressive => Self {
                profile,
                enabled: true,
                latency_prob: 0.65,
                long_latency_prob: 0.18,
                short_io_prob: 0.20,
                // Current profile-driven runs stay with latency and short I/O.
                // Error hooks are available for targeted tests once targets can
                // classify transient storage failures instead of treating them
                // as harness errors.
                read_error_prob: 0.0,
                write_error_prob: 0.0,
                flush_error_prob: 0.0,
                fsync_error_prob: 0.0,
                open_error_prob: 0.0,
                metadata_error_prob: 0.0,
                max_short_io_divisor: 4,
            },
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
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
    decisions: Arc<sim::Rng>,
    time: Option<sim::time::TimeHandle>,
    armed: Arc<AtomicBool>,
    suspended: Arc<AtomicUsize>,
}

impl StorageFaultController {
    pub(crate) fn new(config: StorageFaultConfig, domain: StorageFaultDomain, seed: DstSeed) -> Self {
        Self {
            config,
            domain,
            counters: Arc::default(),
            decisions: Arc::new(sim::decision_source(seed)),
            time: sim::time::try_current_handle(),
            armed: Arc::new(AtomicBool::new(false)),
            suspended: Arc::default(),
        }
    }

    pub(crate) fn enable(&self) {
        self.armed.store(true, Ordering::Relaxed);
    }

    pub(crate) fn with_suspended<T>(&self, f: impl FnOnce() -> T) -> T {
        self.suspended.fetch_add(1, Ordering::Relaxed);
        let _guard = SuspendFaultsGuard {
            suspended: self.suspended.clone(),
        };
        f()
    }

    pub(crate) fn maybe_latency(&self) {
        if self.sample(self.config.latency_prob) {
            self.counters.latency.fetch_add(1, Ordering::Relaxed);
            let latency = if self.sample(self.config.long_latency_prob) {
                Duration::from_millis(25)
            } else {
                Duration::from_millis(1)
            };
            if let Some(time) = &self.time {
                time.advance(latency);
            } else {
                sim::advance_time(latency);
            }
        }
    }

    pub(crate) fn maybe_error(&self, kind: StorageFaultKind) -> io::Result<()> {
        if self.sample(kind.probability(&self.config)) {
            kind.counter(&self.counters).fetch_add(1, Ordering::Relaxed);
            return Err(io::Error::other(kind.message(self.domain)));
        }
        Ok(())
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
        }
    }

    fn active(&self) -> bool {
        self.config.enabled() && self.armed.load(Ordering::Relaxed) && self.suspended.load(Ordering::Relaxed) == 0
    }

    fn sample(&self, probability: f64) -> bool {
        if !self.active() || probability <= 0.0 {
            return false;
        }

        self.decisions.sample_probability(probability)
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
        }
    }

    fn message(self, domain: StorageFaultDomain) -> String {
        let action = match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Flush => "flush",
            Self::Fsync => "fsync",
            Self::Open => "open",
            Self::Metadata => "metadata",
        };
        format!("{INJECTED_ERROR_PREFIX}{} {action} error", domain.label())
    }
}
