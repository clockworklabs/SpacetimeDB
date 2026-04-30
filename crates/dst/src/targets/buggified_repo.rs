use std::{
    fmt,
    io::{self, BufRead, Read, Seek, Write},
    time::Duration,
};

use spacetimedb_commitlog::{
    repo::{Repo, RepoWithoutLockFile, SegmentLen, SegmentReader, TxOffset, TxOffsetIndex, TxOffsetIndexMut},
    segment::FileLike,
};

const LATENCY_PROBABILITY: f64 = 0.35;
const LONG_LATENCY_PROBABILITY: f64 = 0.08;
const SHORT_IO_PROBABILITY: f64 = 0.08;

/// DST-only repo wrapper that makes the in-memory commitlog backend behave less like RAM.
///
/// Faults stay within normal file API semantics: calls may take deterministic simulated time
/// and `Read` / `Write` may complete partially. The wrapper deliberately avoids corruption or
/// crash-style partial persistence; those need a stronger durability model before we enable them.
#[derive(Clone, Debug)]
pub(crate) struct BuggifiedRepo<R> {
    inner: R,
}

impl<R> BuggifiedRepo<R> {
    pub(crate) fn new(inner: R) -> Self {
        Self { inner }
    }
}

impl<R: fmt::Display> fmt::Display for BuggifiedRepo<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}+buggified", self.inner)
    }
}

impl<R: Repo> Repo for BuggifiedRepo<R> {
    type SegmentWriter = BuggifiedSegment<R::SegmentWriter>;
    type SegmentReader = BuggifiedReader<R::SegmentReader>;

    fn create_segment(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        maybe_disk_latency();
        self.inner.create_segment(offset).map(BuggifiedSegment::new)
    }

    fn open_segment_reader(&self, offset: u64) -> io::Result<Self::SegmentReader> {
        maybe_disk_latency();
        self.inner.open_segment_reader(offset).map(BuggifiedReader::new)
    }

    fn open_segment_writer(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        maybe_disk_latency();
        self.inner.open_segment_writer(offset).map(BuggifiedSegment::new)
    }

    fn segment_file_path(&self, offset: u64) -> Option<String> {
        self.inner.segment_file_path(offset)
    }

    fn remove_segment(&self, offset: u64) -> io::Result<()> {
        maybe_disk_latency();
        self.inner.remove_segment(offset)
    }

    fn compress_segment(&self, offset: u64) -> io::Result<()> {
        maybe_disk_latency();
        self.inner.compress_segment(offset)
    }

    fn existing_offsets(&self) -> io::Result<Vec<u64>> {
        maybe_disk_latency();
        self.inner.existing_offsets()
    }

    fn create_offset_index(&self, offset: TxOffset, cap: u64) -> io::Result<TxOffsetIndexMut> {
        maybe_disk_latency();
        self.inner.create_offset_index(offset, cap)
    }

    fn remove_offset_index(&self, offset: TxOffset) -> io::Result<()> {
        maybe_disk_latency();
        self.inner.remove_offset_index(offset)
    }

    fn get_offset_index(&self, offset: TxOffset) -> io::Result<TxOffsetIndex> {
        maybe_disk_latency();
        self.inner.get_offset_index(offset)
    }
}

impl<R: RepoWithoutLockFile> RepoWithoutLockFile for BuggifiedRepo<R> {}

pub(crate) struct BuggifiedSegment<S> {
    inner: S,
}

impl<S> BuggifiedSegment<S> {
    fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S: Read> Read for BuggifiedSegment<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        maybe_disk_latency();
        let len = maybe_short_len(buf.len());
        self.inner.read(&mut buf[..len])
    }
}

impl<S: Write> Write for BuggifiedSegment<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        maybe_disk_latency();
        let len = maybe_short_len(buf.len());
        self.inner.write(&buf[..len])
    }

    fn flush(&mut self) -> io::Result<()> {
        maybe_disk_latency();
        self.inner.flush()
    }
}

impl<S: Seek> Seek for BuggifiedSegment<S> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        maybe_disk_latency();
        self.inner.seek(pos)
    }
}

impl<S: SegmentLen> SegmentLen for BuggifiedSegment<S> {
    fn segment_len(&mut self) -> io::Result<u64> {
        maybe_disk_latency();
        self.inner.segment_len()
    }
}

impl<S: FileLike> FileLike for BuggifiedSegment<S> {
    fn fsync(&mut self) -> io::Result<()> {
        maybe_disk_latency();
        self.inner.fsync()
    }

    fn ftruncate(&mut self, tx_offset: u64, size: u64) -> io::Result<()> {
        maybe_disk_latency();
        self.inner.ftruncate(tx_offset, size)
    }
}

pub(crate) struct BuggifiedReader<S> {
    inner: S,
}

impl<S> BuggifiedReader<S> {
    fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S: Read> Read for BuggifiedReader<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        maybe_disk_latency();
        let len = maybe_short_len(buf.len());
        self.inner.read(&mut buf[..len])
    }
}

impl<S: BufRead> BufRead for BuggifiedReader<S> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        maybe_disk_latency();
        self.inner.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.inner.consume(amount);
    }
}

impl<S: Seek> Seek for BuggifiedReader<S> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        maybe_disk_latency();
        self.inner.seek(pos)
    }
}

impl<S: SegmentLen> SegmentLen for BuggifiedReader<S> {
    fn segment_len(&mut self) -> io::Result<u64> {
        maybe_disk_latency();
        self.inner.segment_len()
    }
}

impl<S: SegmentReader> SegmentReader for BuggifiedReader<S> {
    fn sealed(&self) -> bool {
        self.inner.sealed()
    }
}

fn maybe_disk_latency() {
    #[cfg(madsim)]
    {
        if madsim::buggify::buggify_with_prob(LATENCY_PROBABILITY) {
            let latency = if madsim::buggify::buggify_with_prob(LONG_LATENCY_PROBABILITY) {
                Duration::from_millis(25)
            } else {
                Duration::from_millis(1)
            };
            madsim::time::advance(latency);
        }
    }

    #[cfg(not(madsim))]
    {
        let _ = (LATENCY_PROBABILITY, LONG_LATENCY_PROBABILITY, Duration::ZERO);
    }
}

fn maybe_short_len(len: usize) -> usize {
    if len <= 1 {
        return len;
    }

    #[cfg(madsim)]
    {
        if madsim::buggify::buggify_with_prob(SHORT_IO_PROBABILITY) {
            return (len / 2).max(1);
        }
    }

    #[cfg(not(madsim))]
    {
        let _ = SHORT_IO_PROBABILITY;
    }

    len
}
