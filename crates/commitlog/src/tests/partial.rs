use std::{
    cmp,
    fmt::Debug,
    io::{self, Seek as _, SeekFrom, Write},
    iter::repeat,
};

use log::debug;

use crate::{
    commitlog, error, payload,
    repo::{self, Repo},
    segment::FileLike,
    tests::helpers::enable_logging,
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
    let mut last_segment = repo.open_segment(last_segment_offset).unwrap();
    last_segment
        .seek(SeekFrom::End(-((last_commit.encoded_len() - 1) as i64)))
        .unwrap();
    last_segment.write_all(&[255; 1]).unwrap();

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
                "expected checksum error with offset={}: {:?}",
                last_good_offset,
                commit
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

impl FileLike for ShortSegment {
    fn fsync(&self) -> std::io::Result<()> {
        self.inner.fsync()
    }

    fn ftruncate(&self, size: u64) -> std::io::Result<()> {
        self.inner.ftruncate(size)
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

impl Repo for ShortMem {
    type Segment = ShortSegment;

    fn create_segment(&self, offset: u64) -> io::Result<Self::Segment> {
        self.inner.create_segment(offset).map(|inner| ShortSegment {
            inner,
            max_len: self.max_len,
        })
    }

    fn open_segment(&self, offset: u64) -> io::Result<Self::Segment> {
        self.inner.open_segment(offset).map(|inner| ShortSegment {
            inner,
            max_len: self.max_len,
        })
    }

    fn remove_segment(&self, offset: u64) -> io::Result<()> {
        self.inner.remove_segment(offset)
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
