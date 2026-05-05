//! Commitlog storage fault-injection support for DST targets.

use std::{
    fmt,
    io::{self, BufRead, Read, Seek, Write},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use spacetimedb_commitlog::{
    repo::{Repo, RepoWithoutLockFile, SegmentLen, SegmentReader, TxOffset, TxOffsetIndex, TxOffsetIndexMut},
    segment::FileLike,
};

use crate::{config::CommitlogFaultProfile, seed::DstSeed, sim};

const INJECTED_DISK_ERROR_PREFIX: &str = "dst injected disk ";

/// Returns true if `text` contains an error created by this fault layer.
pub(crate) fn is_injected_disk_error_text(text: &str) -> bool {
    text.contains(INJECTED_DISK_ERROR_PREFIX)
}

/// Configurable fault profile for a DST-only commitlog repository wrapper.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CommitlogFaultConfig {
    profile: CommitlogFaultProfile,
    enabled: bool,
    latency_prob: f64,
    long_latency_prob: f64,
    short_io_prob: f64,
    read_error_prob: f64,
    write_error_prob: f64,
    flush_error_prob: f64,
    fsync_error_prob: f64,
    open_error_prob: f64,
    metadata_error_prob: f64,
    max_short_io_divisor: usize,
}

impl CommitlogFaultConfig {
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
                // The current local durability actor does not recover from I/O errors,
                // so profile-driven runs stay with latency and short I/O. The counters
                // and hooks stay here for targeted tests once the target can classify
                // those failures instead of treating them as harness errors.
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
pub(crate) struct CommitlogFaultSummary {
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

/// DST-only repo wrapper that makes the in-memory commitlog backend behave less like RAM.
///
/// Faults stay within normal file API semantics: calls may take deterministic simulated time,
/// reads/writes may complete partially, and configured calls may return transient I/O errors.
/// The wrapper deliberately avoids corruption or crash-style partial persistence; those need a
/// stronger durability model before we enable them.
#[derive(Clone, Debug)]
pub(crate) struct FaultableRepo<R> {
    inner: R,
    faults: FaultController,
}

impl<R> FaultableRepo<R> {
    pub(crate) fn new(inner: R, config: CommitlogFaultConfig, seed: DstSeed) -> Self {
        Self {
            inner,
            faults: FaultController::new(config, seed),
        }
    }

    pub(crate) fn enable_faults(&self) {
        self.faults.enable();
    }

    pub(crate) fn fault_summary(&self) -> CommitlogFaultSummary {
        self.faults.summary()
    }

    pub(crate) fn with_faults_suspended<T>(&self, f: impl FnOnce() -> T) -> T {
        self.faults.with_suspended(f)
    }
}

impl<R: fmt::Display> fmt::Display for FaultableRepo<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}+faultable({})", self.inner, self.faults.config.profile)
    }
}

impl<R: Repo> Repo for FaultableRepo<R> {
    type SegmentWriter = FaultableSegment<R::SegmentWriter>;
    type SegmentReader = FaultableReader<R::SegmentReader>;

    fn create_segment(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Open)?;
        self.inner
            .create_segment(offset)
            .map(|inner| FaultableSegment::new(inner, self.faults.clone()))
    }

    fn open_segment_reader(&self, offset: u64) -> io::Result<Self::SegmentReader> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Open)?;
        self.inner
            .open_segment_reader(offset)
            .map(|inner| FaultableReader::new(inner, self.faults.clone()))
    }

    fn open_segment_writer(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Open)?;
        self.inner
            .open_segment_writer(offset)
            .map(|inner| FaultableSegment::new(inner, self.faults.clone()))
    }

    fn segment_file_path(&self, offset: u64) -> Option<String> {
        self.inner.segment_file_path(offset)
    }

    fn remove_segment(&self, offset: u64) -> io::Result<()> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Metadata)?;
        self.inner.remove_segment(offset)
    }

    fn compress_segment(&self, offset: u64) -> io::Result<()> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Metadata)?;
        self.inner.compress_segment(offset)
    }

    fn existing_offsets(&self) -> io::Result<Vec<u64>> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Metadata)?;
        self.inner.existing_offsets()
    }

    fn create_offset_index(&self, offset: TxOffset, cap: u64) -> io::Result<TxOffsetIndexMut> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Metadata)?;
        self.inner.create_offset_index(offset, cap)
    }

    fn remove_offset_index(&self, offset: TxOffset) -> io::Result<()> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Metadata)?;
        self.inner.remove_offset_index(offset)
    }

    fn get_offset_index(&self, offset: TxOffset) -> io::Result<TxOffsetIndex> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Metadata)?;
        self.inner.get_offset_index(offset)
    }
}

impl<R: RepoWithoutLockFile> RepoWithoutLockFile for FaultableRepo<R> {}

pub(crate) struct FaultableSegment<S> {
    inner: S,
    faults: FaultController,
}

impl<S> FaultableSegment<S> {
    fn new(inner: S, faults: FaultController) -> Self {
        Self { inner, faults }
    }
}

impl<S: Read> Read for FaultableSegment<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Read)?;
        let len = self.faults.maybe_short_len(buf.len(), ShortIoKind::Read);
        self.inner.read(&mut buf[..len])
    }
}

impl<S: Write> Write for FaultableSegment<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Write)?;
        let len = self.faults.maybe_short_len(buf.len(), ShortIoKind::Write);
        self.inner.write(&buf[..len])
    }

    fn flush(&mut self) -> io::Result<()> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Flush)?;
        self.inner.flush()
    }
}

impl<S: Seek> Seek for FaultableSegment<S> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.faults.maybe_disk_latency();
        self.inner.seek(pos)
    }
}

impl<S: SegmentLen> SegmentLen for FaultableSegment<S> {
    fn segment_len(&mut self) -> io::Result<u64> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Metadata)?;
        self.inner.segment_len()
    }
}

impl<S: FileLike> FileLike for FaultableSegment<S> {
    fn fsync(&mut self) -> io::Result<()> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Fsync)?;
        self.inner.fsync()
    }

    fn ftruncate(&mut self, tx_offset: u64, size: u64) -> io::Result<()> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Metadata)?;
        self.inner.ftruncate(tx_offset, size)
    }
}

pub(crate) struct FaultableReader<S> {
    inner: S,
    faults: FaultController,
}

impl<S> FaultableReader<S> {
    fn new(inner: S, faults: FaultController) -> Self {
        Self { inner, faults }
    }
}

impl<S: Read> Read for FaultableReader<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Read)?;
        let len = self.faults.maybe_short_len(buf.len(), ShortIoKind::Read);
        self.inner.read(&mut buf[..len])
    }
}

impl<S: BufRead> BufRead for FaultableReader<S> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Read)?;
        let buf = self.inner.fill_buf()?;
        let len = self.faults.maybe_short_len(buf.len(), ShortIoKind::Read);
        Ok(&buf[..len])
    }

    fn consume(&mut self, amount: usize) {
        self.inner.consume(amount);
    }
}

impl<S: Seek> Seek for FaultableReader<S> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.faults.maybe_disk_latency();
        self.inner.seek(pos)
    }
}

impl<S: SegmentLen> SegmentLen for FaultableReader<S> {
    fn segment_len(&mut self) -> io::Result<u64> {
        self.faults.maybe_disk_latency();
        self.faults.maybe_error(FaultKind::Metadata)?;
        self.inner.segment_len()
    }
}

impl<S: SegmentReader> SegmentReader for FaultableReader<S> {
    fn sealed(&self) -> bool {
        self.inner.sealed()
    }
}

#[derive(Clone, Debug)]
struct FaultController {
    config: CommitlogFaultConfig,
    counters: Arc<FaultCounters>,
    decisions: Arc<sim::DecisionSource>,
    time: Option<sim::time::TimeHandle>,
    armed: Arc<AtomicBool>,
    suspended: Arc<AtomicUsize>,
}

impl FaultController {
    fn new(config: CommitlogFaultConfig, seed: DstSeed) -> Self {
        Self {
            config,
            counters: Arc::default(),
            decisions: Arc::new(sim::decision_source(seed)),
            time: sim::time::try_current_handle(),
            armed: Arc::new(AtomicBool::new(false)),
            suspended: Arc::default(),
        }
    }

    fn enable(&self) {
        self.armed.store(true, Ordering::Relaxed);
    }

    fn active(&self) -> bool {
        self.config.enabled() && self.armed.load(Ordering::Relaxed) && self.suspended.load(Ordering::Relaxed) == 0
    }

    fn with_suspended<T>(&self, f: impl FnOnce() -> T) -> T {
        self.suspended.fetch_add(1, Ordering::Relaxed);
        let _guard = SuspendFaultsGuard {
            suspended: self.suspended.clone(),
        };
        f()
    }

    fn maybe_disk_latency(&self) {
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

    fn maybe_error(&self, kind: FaultKind) -> io::Result<()> {
        if self.sample(kind.probability(&self.config)) {
            kind.counter(&self.counters).fetch_add(1, Ordering::Relaxed);
            return Err(io::Error::other(kind.message()));
        }
        Ok(())
    }

    fn maybe_short_len(&self, len: usize, kind: ShortIoKind) -> usize {
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

    fn sample(&self, probability: f64) -> bool {
        if !self.active() || probability <= 0.0 {
            return false;
        }

        self.decisions.sample_probability(probability)
    }

    fn summary(&self) -> CommitlogFaultSummary {
        CommitlogFaultSummary {
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
enum ShortIoKind {
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
enum FaultKind {
    Read,
    Write,
    Flush,
    Fsync,
    Open,
    Metadata,
}

impl FaultKind {
    fn probability(self, config: &CommitlogFaultConfig) -> f64 {
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

    fn message(self) -> &'static str {
        match self {
            Self::Read => "dst injected disk read error",
            Self::Write => "dst injected disk write error",
            Self::Flush => "dst injected disk flush error",
            Self::Fsync => "dst injected disk fsync error",
            Self::Open => "dst injected disk open error",
            Self::Metadata => "dst injected disk metadata error",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, Cursor};

    use super::*;

    fn always_short_read_config() -> CommitlogFaultConfig {
        CommitlogFaultConfig {
            profile: CommitlogFaultProfile::Default,
            enabled: true,
            latency_prob: 0.0,
            long_latency_prob: 0.0,
            short_io_prob: 1.0,
            read_error_prob: 0.0,
            write_error_prob: 0.0,
            flush_error_prob: 0.0,
            fsync_error_prob: 0.0,
            open_error_prob: 0.0,
            metadata_error_prob: 0.0,
            max_short_io_divisor: 2,
        }
    }

    #[test]
    fn buf_read_path_applies_short_read_faults() {
        let faults = FaultController::new(always_short_read_config(), DstSeed(55));
        faults.enable();
        let mut reader = FaultableReader::new(Cursor::new(vec![1, 2, 3, 4]), faults.clone());

        assert_eq!(reader.fill_buf().unwrap(), &[1, 2]);
        assert_eq!(faults.summary().short_read, 1);
    }
}
