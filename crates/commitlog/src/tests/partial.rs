use std::{
    cmp,
    fmt::{self, Debug},
    io::{self, Seek as _, SeekFrom},
    iter::{self, repeat},
    num::NonZeroU16,
    sync::RwLockWriteGuard,
};

use log::debug;
use pretty_assertions::assert_matches;

use crate::{
    commitlog, error, payload,
    repo::{self, Repo, SegmentLen},
    segment::{self, FileLike},
    tests::helpers::{enable_logging, fill_log_with},
    Commit, Encode, Options, DEFAULT_LOG_FORMAT_VERSION,
};

#[test]
fn traversal() {
    enable_logging();

    let mut log = open_log::<[u8; 32]>(ShortMem::new(800));
    let total_commits = 100;
    let total_txs = fill_log_enospc(&mut log, total_commits, (1..=10).cycle());

    assert_eq!(
        total_txs,
        log.transactions_from(0, &payload::ArrayDecoder)
            .map(Result::unwrap)
            .count()
    );
    assert_eq!(total_commits, log.commits_from(0).map(Result::unwrap).count());
}

// Note: Write errors cause the in-flight commit to be written to a fresh
// segment. So as long as we write through the public API, partial writes
// never surface (i.e. the log is contiguous).
#[test]
fn reopen() {
    enable_logging();

    let repo = ShortMem::new(800);
    let num_commits = 10;

    let mut total_txs = 0;
    for i in 0..2 {
        let mut log = open_log::<[u8; 32]>(repo.clone());
        total_txs += fill_log_enospc(&mut log, num_commits, (1..=10).cycle());

        debug!("fill {} done", i + 1);
    }

    assert_eq!(
        total_txs,
        open_log::<[u8; 32]>(repo.clone())
            .transactions_from(0, &payload::ArrayDecoder)
            .map(Result::unwrap)
            .count()
    );

    // Let's see if we hit a funny case in any of the segments.
    for offset in repo.existing_offsets().unwrap().into_iter().rev() {
        let meta = repo::open_segment_reader(&repo, DEFAULT_LOG_FORMAT_VERSION, offset)
            .unwrap()
            .metadata()
            .unwrap();
        debug!("dropping segment: segment::{meta:?}");
        repo.remove_segment(offset).unwrap();
        assert_eq!(
            meta.tx_range.start,
            open_log::<[u8; 32]>(repo.clone())
                .transactions_from(0, &payload::ArrayDecoder)
                .map(Result::unwrap)
                .count() as u64
        );
    }
}

#[test]
fn overwrite_reopen() {
    enable_logging();

    let repo = ShortMem::new(800);
    let num_commits = 10;
    let txs_per_commit = 5;

    let mut log = open_log::<[u8; 32]>(repo.clone());
    let mut total_txs = fill_log_enospc(&mut log, num_commits, repeat(txs_per_commit));

    let last_segment_offset = repo.existing_offsets().unwrap().last().copied().unwrap();
    let last_commit: Commit = repo::open_segment_reader(&repo, DEFAULT_LOG_FORMAT_VERSION, last_segment_offset)
        .unwrap()
        .commits()
        .map(Result::unwrap)
        .last()
        .unwrap()
        .into();
    debug!("last commit: {last_commit:?}");

    {
        let mut last_segment = repo.open_segment_writer(last_segment_offset).unwrap();
        let mut data = last_segment.buf_mut();
        let pos = data.len() - last_commit.encoded_len() + 1;
        data[pos] = 255;
    }

    let mut log = open_log::<[u8; 32]>(repo.clone());
    for (i, commit) in log.commits_from(0).enumerate() {
        if i < num_commits - 1 {
            commit.expect("all but last commit should be good");
        } else {
            let last_good_offset = txs_per_commit * (num_commits - 1);
            assert!(
                matches!(
                    commit,
                    Err(error::Traversal::Checksum { offset, .. }) if offset == last_good_offset as u64,
                ),
                "expected checksum error with offset={last_good_offset}: {commit:?}"
            );
        }
    }

    // Write some more data.
    total_txs += fill_log_enospc(&mut log, num_commits, repeat(txs_per_commit));
    // Log should be contiguous, but missing one corrupted commit.
    assert_eq!(
        total_txs - txs_per_commit,
        log.transactions_from(0, &payload::ArrayDecoder)
            .map(Result::unwrap)
            .count()
    );
    // Check that this is true if we reopen the log.
    assert_eq!(
        total_txs - txs_per_commit,
        open_log::<[u8; 32]>(repo)
            .transactions_from(0, &payload::ArrayDecoder)
            .map(Result::unwrap)
            .count()
    );
}

/// Edge case surfaced in production:
///
/// If the first commit in the last segment is corrupt, creating a new segment
/// would fail because the `tx_range` is the same as the corrupt segment.
///
/// We don't automatically recover from that, but test that `open` returns an
/// error providing some context.
#[test]
fn first_commit_in_last_segment_corrupt() {
    enable_logging();

    let repo = repo::Memory::new();
    let options = Options {
        max_segment_size: 512,
        max_records_in_commit: NonZeroU16::new(1).unwrap(),
        ..<_>::default()
    };
    {
        let mut log = commitlog::Generic::open(repo.clone(), options).unwrap();
        fill_log_with(&mut log, iter::once([b'x'; 64]).cycle().take(9));
    }
    let segments = repo.existing_offsets().unwrap();
    assert_eq!(2, segments.len(), "repo should contain 2 segments");

    {
        let last_segment = repo.open_segment_writer(*segments.last().unwrap()).unwrap();
        let mut data = last_segment.buf_mut();
        data[segment::Header::LEN + 1..].fill(0);
    }

    assert_matches!(
        commitlog::Generic::<_, [u8; 64]>::open(repo, options),
        Err(e) if e.kind() == io::ErrorKind::InvalidData,
    );
}

fn open_log<T>(repo: ShortMem) -> commitlog::Generic<ShortMem, T> {
    commitlog::Generic::open(
        repo,
        Options {
            max_segment_size: 1024,
            ..Options::default()
        },
    )
    .unwrap()
}

const ENOSPC: i32 = 28;

/// Wrapper around [`mem::Segment`] which causes a partial [`io::Write::write`]
/// if and when the size of the underlying buffer exceeds a max length.
#[derive(Debug)]
struct ShortSegment {
    inner: repo::mem::Segment,
    max_len: u64,
}

impl ShortSegment {
    fn buf_mut(&mut self) -> RwLockWriteGuard<'_, Vec<u8>> {
        self.inner.buf_mut()
    }
}

impl SegmentLen for ShortSegment {
    fn segment_len(&mut self) -> io::Result<u64> {
        self.inner.segment_len()
    }
}

impl FileLike for ShortSegment {
    fn fsync(&mut self) -> std::io::Result<()> {
        self.inner.fsync()
    }

    fn ftruncate(&mut self, tx_offset: u64, size: u64) -> std::io::Result<()> {
        self.inner.ftruncate(tx_offset, size)
    }
}

impl io::Write for ShortSegment {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let pos = self.inner.stream_position()?;
        debug!("pos={} max_len={} buf-len={}", pos, self.max_len, buf.len());
        if pos + buf.len() as u64 > self.max_len {
            let max = cmp::min(1, (self.max_len - pos) as usize);
            let n = self.inner.write(&buf[..max])?;
            debug!("partial write {}/{}", n, buf.len());
            return Err(io::Error::from_raw_os_error(ENOSPC));
        }
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl io::Read for ShortSegment {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl io::Seek for ShortSegment {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// Wrapper around [`repo::Memory`] which causes partial (or: short) writes.
#[derive(Debug, Clone)]
struct ShortMem {
    inner: repo::Memory,
    max_len: u64,
}

impl ShortMem {
    pub fn new(max_len: u64) -> Self {
        Self {
            inner: repo::Memory::new(),
            max_len,
        }
    }
}

impl fmt::Display for ShortMem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl Repo for ShortMem {
    type SegmentWriter = ShortSegment;
    type SegmentReader = io::BufReader<repo::mem::Segment>;

    fn create_segment(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        self.inner.create_segment(offset).map(|inner| ShortSegment {
            inner,
            max_len: self.max_len,
        })
    }

    fn open_segment_writer(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        self.inner.open_segment_writer(offset).map(|inner| ShortSegment {
            inner,
            max_len: self.max_len,
        })
    }

    fn open_segment_reader(&self, offset: u64) -> io::Result<Self::SegmentReader> {
        self.inner.open_segment_reader(offset)
    }

    fn remove_segment(&self, offset: u64) -> io::Result<()> {
        self.inner.remove_segment(offset)
    }

    fn compress_segment(&self, offset: u64) -> io::Result<()> {
        self.inner.compress_segment(offset)
    }

    fn existing_offsets(&self) -> io::Result<Vec<u64>> {
        self.inner.existing_offsets()
    }
}

/// Like [`crate::tests::helpers::fill_log`], but expect that ENOSPC happens at
/// least once.
fn fill_log_enospc<T>(
    log: &mut commitlog::Generic<ShortMem, T>,
    num_commits: usize,
    txs_per_commit: impl Iterator<Item = usize>,
) -> usize
where
    T: Debug + Default + Encode,
{
    let mut seen_enospc = false;

    let mut total_txs = 0;
    for (_, n) in (0..num_commits).zip(txs_per_commit) {
        for _ in 0..n {
            log.append(T::default()).unwrap();
            total_txs += 1;
        }
        let res = log.commit();
        if let Err(Some(os)) = res.as_ref().map_err(|e| e.raw_os_error()) {
            if os == ENOSPC {
                debug!("fill: ignoring ENOSPC");
                seen_enospc = true;
                log.commit().unwrap();
                continue;
            }
        }
        res.unwrap();
    }

    assert!(seen_enospc, "expected to see ENOSPC");

    total_txs
}
