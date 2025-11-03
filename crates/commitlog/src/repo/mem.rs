use std::{
    collections::{btree_map, BTreeMap},
    fmt, io,
    sync::{Arc, RwLock, RwLockWriteGuard},
};

use crate::segment::FileLike;

use super::Repo;

type SharedLock<T> = Arc<RwLock<T>>;
type SharedBytes = SharedLock<Vec<u8>>;

/// A log segment backed by a `Vec<u8>`.
///
/// Writing to the segment behaves like a file opened with `O_APPEND`:
/// [`io::Write::write`] always appends to the segment, regardless of the
/// current position, and updates the position to the new length of the segment.
/// The initial position is zero.
///
/// Note that this is not a faithful model of a file, as safe Rust requires to
/// protect the buffer with a lock. This means that pathological situations
/// arising from concurrent read/write access of a file are impossible to occur.
#[derive(Clone, Debug, Default)]
pub struct Segment {
    pos: u64,
    buf: SharedBytes,
}

impl Segment {
    pub fn len(&self) -> usize {
        self.buf.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Obtain mutable access to the underlying buffer.
    ///
    /// This is intended for tests which deliberately corrupt the segment data.
    pub fn buf_mut(&self) -> RwLockWriteGuard<'_, Vec<u8>> {
        self.buf.write().unwrap()
    }
}

impl From<SharedBytes> for Segment {
    fn from(buf: SharedBytes) -> Self {
        Self { pos: 0, buf }
    }
}

impl super::SegmentLen for Segment {
    fn segment_len(&mut self) -> io::Result<u64> {
        Ok(self.len() as u64)
    }
}

impl FileLike for Segment {
    fn fsync(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn ftruncate(&mut self, _tx_offset: u64, size: u64) -> io::Result<()> {
        let mut inner = self.buf.write().unwrap();
        inner.resize(size as usize, 0);
        // NOTE: As per `ftruncate(2)`, the offset is not changed.
        Ok(())
    }
}

impl io::Write for Segment {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self.buf.write().unwrap();
        inner.extend(buf);
        self.pos += buf.len() as u64;

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Read for Segment {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let inner = self.buf.read().unwrap();
        let pos = self.pos as usize;
        if pos > inner.len() {
            // Bad file descriptor
            return Err(io::Error::from_raw_os_error(9));
        }
        let n = io::Read::read(&mut &inner[pos..], buf)?;
        self.pos += n as u64;

        Ok(n)
    }
}

impl io::Seek for Segment {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let (base_pos, offset) = match pos {
            io::SeekFrom::Start(n) => {
                self.pos = n;
                return Ok(n);
            }
            io::SeekFrom::End(n) => (self.len() as u64, n),
            io::SeekFrom::Current(n) => (self.pos, n),
        };
        match base_pos.checked_add_signed(offset) {
            Some(n) => {
                self.pos = n;
                Ok(n)
            }
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek to a negative or overflowing position",
            )),
        }
    }
}

#[cfg(feature = "streaming")]
mod async_impls {
    use super::*;

    use std::{
        io::{Seek as _, Write as _},
        pin::Pin,
        task::{Context, Poll},
    };

    use tokio::io::{self, AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};

    use crate::stream::{AsyncFsync, AsyncLen, AsyncRepo, IntoAsyncWriter};

    impl AsyncRepo for Memory {
        type AsyncSegmentWriter = io::BufWriter<Segment>;
        type AsyncSegmentReader = io::BufReader<Segment>;

        async fn open_segment_reader_async(&self, offset: u64) -> io::Result<Self::AsyncSegmentReader> {
            self.open_segment_writer(offset).map(io::BufReader::new)
        }
    }

    impl IntoAsyncWriter for Segment {
        type AsyncWriter = tokio::io::BufWriter<Self>;

        fn into_async_writer(self) -> Self::AsyncWriter {
            tokio::io::BufWriter::new(self)
        }
    }

    impl AsyncRead for Segment {
        fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            let inner = this.buf.read().unwrap();
            let pos = this.pos as usize;
            if pos > inner.len() {
                // Bad file descriptor
                return Poll::Ready(Err(io::Error::from_raw_os_error(9)));
            }
            let filled = buf.filled().len();
            AsyncRead::poll_read(Pin::new(&mut &inner[pos..]), cx, buf).map_ok(|()| {
                this.pos += (buf.filled().len() - filled) as u64;
            })
        }
    }

    impl AsyncSeek for Segment {
        fn start_seek(self: Pin<&mut Self>, position: io::SeekFrom) -> io::Result<()> {
            self.get_mut().seek(position).map(drop)
        }

        fn poll_complete(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
            Poll::Ready(self.get_mut().stream_position())
        }
    }

    impl AsyncWrite for Segment {
        fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, io::Error>> {
            Poll::Ready(self.get_mut().write(buf))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncFsync for Segment {
        async fn fsync(&self) {}
    }

    impl AsyncLen for Segment {
        async fn segment_len(&mut self) -> io::Result<u64> {
            Ok(self.len() as u64)
        }
    }
}

/// In-memory implementation of [`Repo`].
#[derive(Clone, Debug, Default)]
pub struct Memory(SharedLock<BTreeMap<u64, SharedBytes>>);

impl Memory {
    pub fn new() -> Self {
        Self::default()
    }
}

impl fmt::Display for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<memory>")
    }
}

impl Repo for Memory {
    type SegmentWriter = Segment;
    type SegmentReader = io::BufReader<Segment>;

    fn create_segment(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        let mut inner = self.0.write().unwrap();
        match inner.entry(offset) {
            btree_map::Entry::Occupied(entry) => {
                let entry = entry.get();
                let read_guard = entry.read().unwrap();
                if read_guard.is_empty() {
                    Ok(Segment::from(Arc::clone(entry)))
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("segment {offset} already exists"),
                    ))
                }
            }
            btree_map::Entry::Vacant(entry) => {
                let segment = entry.insert(Default::default());
                Ok(Segment::from(Arc::clone(segment)))
            }
        }
    }

    fn open_segment_writer(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        let inner = self.0.read().unwrap();
        let Some(buf) = inner.get(&offset) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("segment {offset} does not exist"),
            ));
        };
        Ok(Segment::from(Arc::clone(buf)))
    }

    fn open_segment_reader(&self, offset: u64) -> io::Result<Self::SegmentReader> {
        self.open_segment_writer(offset).map(io::BufReader::new)
    }

    fn remove_segment(&self, offset: u64) -> io::Result<()> {
        let mut inner = self.0.write().unwrap();
        if inner.remove(&offset).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("segment {offset} does not exist"),
            ));
        }

        Ok(())
    }

    fn compress_segment(&self, _offset: u64) -> io::Result<()> {
        Ok(())
    }

    fn existing_offsets(&self) -> io::Result<Vec<u64>> {
        Ok(self.0.read().unwrap().keys().copied().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, Write};

    #[test]
    fn segment_read_write_seek() {
        let mut segment = Segment::default();
        segment.write_all(b"alonso").unwrap();

        segment.seek(io::SeekFrom::Start(0)).unwrap();
        let mut buf = [0; 6];
        segment.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"alonso");

        segment.seek(io::SeekFrom::Start(2)).unwrap();
        let n = segment.read(&mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], b"onso");

        segment.seek(io::SeekFrom::Current(-4)).unwrap();
        let n = segment.read(&mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], b"onso");

        segment.seek(io::SeekFrom::End(-3)).unwrap();
        let n = segment.read(&mut buf).unwrap();
        assert_eq!(n, 3);
        assert_eq!(&buf[0..3], b"nso");
    }
}
