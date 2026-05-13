//! Commitlog storage fault-injection support for DST targets.

use std::{
    fmt,
    io::{self, BufRead, Read, Seek, Write},
};

use spacetimedb_commitlog::{
    repo::{
        CompressOnce, CompressionStats, Repo, RepoWithoutLockFile, SegmentLen, SegmentReader, TxOffset, TxOffsetIndex, TxOffsetIndexMut,
    },
    segment::{FileLike, Header},
};

use crate::{
    seed::DstSeed,
    sim::storage_faults::{
        is_injected_fault_text, ShortIoKind, StorageFaultConfig, StorageFaultController, StorageFaultDomain,
        StorageFaultKind, StorageFaultSummary,
    },
};

pub(crate) type CommitlogFaultConfig = StorageFaultConfig;
pub(crate) type CommitlogFaultSummary = StorageFaultSummary;

/// Returns true if `text` contains an error created by this fault layer.
pub(crate) fn is_injected_disk_error_text(text: &str) -> bool {
    is_injected_fault_text(StorageFaultDomain::Disk, text)
}

/// DST-only repo wrapper that makes the in-memory commitlog backend behave less like RAM.
///
/// Faults stay within normal file API semantics: calls may take deterministic simulated time,
/// reads/writes may complete partially, and configured calls may return transient I/O errors.
/// The wrapper deliberately avoids corruption or crash-style partial persistence; those need a
/// stronger durability model before we enable them.
#[derive(Clone)]
pub(crate) struct FaultableRepo<R> {
    inner: R,
    faults: StorageFaultController,
}

impl<R> FaultableRepo<R> {
    pub(crate) fn new(inner: R, config: CommitlogFaultConfig, seed: DstSeed) -> Self {
        Self {
            inner,
            faults: StorageFaultController::new(config, StorageFaultDomain::Disk, seed),
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
        write!(f, "{}+faultable({})", self.inner, self.faults.summary().profile)
    }
}

impl<R: Repo> Repo for FaultableRepo<R> {
    type SegmentWriter = FaultableSegment<R::SegmentWriter>;
    type SegmentReader = FaultableReader<R::SegmentReader>;

    fn create_segment(&self, offset: u64, header: Header) -> io::Result<Self::SegmentWriter> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Open)?;
        self.inner
            .create_segment(offset, header)
            .map(|inner| FaultableSegment::new(inner, self.faults.clone()))
    }

    fn open_segment_reader(&self, offset: u64) -> io::Result<Self::SegmentReader> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Open)?;
        self.inner
            .open_segment_reader(offset)
            .map(|inner| FaultableReader::new(inner, self.faults.clone()))
    }

    fn open_segment_writer(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Open)?;
        self.inner
            .open_segment_writer(offset)
            .map(|inner| FaultableSegment::new(inner, self.faults.clone()))
    }

    fn segment_file_path(&self, offset: u64) -> Option<String> {
        self.inner.segment_file_path(offset)
    }

    fn remove_segment(&self, offset: u64) -> io::Result<()> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Metadata)?;
        self.inner.remove_segment(offset)
    }

    fn compress_segment_with(&self, offset: u64, f: impl CompressOnce) -> io::Result<CompressionStats> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Metadata)?;
        self.inner.compress_segment_with(offset, f)
    }

    fn existing_offsets(&self) -> io::Result<Vec<u64>> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Metadata)?;
        self.inner.existing_offsets()
    }

    fn create_offset_index(&self, offset: TxOffset, cap: u64) -> io::Result<TxOffsetIndexMut> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Metadata)?;
        self.inner.create_offset_index(offset, cap)
    }

    fn remove_offset_index(&self, offset: TxOffset) -> io::Result<()> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Metadata)?;
        self.inner.remove_offset_index(offset)
    }

    fn get_offset_index(&self, offset: TxOffset) -> io::Result<TxOffsetIndex> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Metadata)?;
        self.inner.get_offset_index(offset)
    }
}

impl<R: RepoWithoutLockFile> RepoWithoutLockFile for FaultableRepo<R> {}

pub(crate) struct FaultableSegment<S> {
    inner: S,
    faults: StorageFaultController,
}

impl<S> FaultableSegment<S> {
    fn new(inner: S, faults: StorageFaultController) -> Self {
        Self { inner, faults }
    }
}

impl<S: Read> Read for FaultableSegment<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Read)?;
        let len = self.faults.maybe_short_len(buf.len(), ShortIoKind::Read);
        self.inner.read(&mut buf[..len])
    }
}

impl<S: Write> Write for FaultableSegment<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Write)?;
        let len = self.faults.maybe_short_len(buf.len(), ShortIoKind::Write);
        self.inner.write(&buf[..len])
    }

    fn flush(&mut self) -> io::Result<()> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Flush)?;
        self.inner.flush()
    }
}

impl<S: Seek> Seek for FaultableSegment<S> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.faults.maybe_latency();
        self.inner.seek(pos)
    }
}

impl<S: SegmentLen> SegmentLen for FaultableSegment<S> {
    fn segment_len(&mut self) -> io::Result<u64> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Metadata)?;
        self.inner.segment_len()
    }
}

impl<S: FileLike> FileLike for FaultableSegment<S> {
    fn fsync(&mut self) -> io::Result<()> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Fsync)?;
        self.inner.fsync()
    }

    fn ftruncate(&mut self, tx_offset: u64, size: u64) -> io::Result<()> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Metadata)?;
        self.inner.ftruncate(tx_offset, size)
    }
}

pub(crate) struct FaultableReader<S> {
    inner: S,
    faults: StorageFaultController,
}

impl<S> FaultableReader<S> {
    fn new(inner: S, faults: StorageFaultController) -> Self {
        Self { inner, faults }
    }
}

impl<S: Read> Read for FaultableReader<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Read)?;
        let len = self.faults.maybe_short_len(buf.len(), ShortIoKind::Read);
        self.inner.read(&mut buf[..len])
    }
}

impl<S: BufRead> BufRead for FaultableReader<S> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Read)?;
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
        self.faults.maybe_latency();
        self.inner.seek(pos)
    }
}

impl<S: SegmentLen> SegmentLen for FaultableReader<S> {
    fn segment_len(&mut self) -> io::Result<u64> {
        self.faults.maybe_latency();
        self.faults.maybe_error(StorageFaultKind::Metadata)?;
        self.inner.segment_len()
    }
}

impl<S: SegmentReader> SegmentReader for FaultableReader<S> {
    fn sealed(&self) -> bool {
        self.inner.sealed()
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, Cursor};

    use crate::config::CommitlogFaultProfile;

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
        let faults = StorageFaultController::new(always_short_read_config(), StorageFaultDomain::Disk, DstSeed(55));
        faults.enable();
        let mut reader = FaultableReader::new(Cursor::new(vec![1, 2, 3, 4]), faults.clone());

        assert_eq!(reader.fill_buf().unwrap(), &[1, 2]);
        assert_eq!(faults.summary().short_read, 1);
    }
}
