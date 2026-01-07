use std::{
    cmp,
    fmt::{self, Debug},
    io::{self, Seek as _, SeekFrom},
    iter,
};

use log::debug;
use pretty_assertions::assert_matches;

use crate::{
    commitlog,
    repo::{self, Repo, SegmentLen},
    segment::{self, FileLike},
    tests::helpers::{enable_logging, fill_log_with},
    Options,
};

#[test]
#[should_panic]
fn panics_on_enospc() {
    enable_logging();

    let mut log = open_log::<[u8; 32]>(ShortMem::new(800));
    for i in 0..100 {
        log.commit([(i, [b'z'; 32])]).unwrap();
    }
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

    let repo = repo::Memory::unlimited();
    let options = Options {
        max_segment_size: 512,
        ..<_>::default()
    };
    {
        let mut log = commitlog::Generic::open(repo.clone(), options).unwrap();
        fill_log_with(&mut log, iter::once([b'x'; 64]).cycle().take(9));
    }
    let segments = repo.existing_offsets().unwrap();
    assert_eq!(2, segments.len(), "repo should contain 2 segments");

    {
        let mut last_segment = repo.open_segment_writer(*segments.last().unwrap()).unwrap();
        last_segment.modify_bytes_at(segment::Header::LEN + 1.., |data| data.fill(0));
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

    #[cfg(feature = "fallocate")]
    fn fallocate(&mut self, size: u64) -> io::Result<()> {
        self.inner.fallocate(size)
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
            inner: repo::Memory::new(max_len * 4096),
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
