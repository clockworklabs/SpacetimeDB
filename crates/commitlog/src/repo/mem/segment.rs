use std::{
    io,
    sync::{Arc, Mutex, RwLock},
};

use crate::{
    repo::{
        mem::{SpaceOnDevice, PAGE_SIZE},
        SegmentLen,
    },
    segment::FileLike,
};

pub type SharedLock<T> = Arc<RwLock<T>>;

/// Backing storage for a [Segment].
///
/// Morally, this consists of [PAGE_SIZE] chunks. Actually allocating the
/// memory is, however, prohibitively expensive (in particular in property
/// test). Thus, the underlying [Vec<u8>] buffer allocates as necessary, but
/// [Storage] tracks the logical amount of allocated space (in [PAGE_SIZE]
/// increments).
///
/// The data of a [Storage] is fully managed by its frontend [Segment].
/// The type is exported to allow sharing the storage between different
/// segments, each tracking a different read/write position.
#[derive(Debug)]
pub(super) struct Storage {
    alloc: u64,
    buf: Vec<u8>,
}

impl Storage {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            alloc: 0,
            buf: Vec::with_capacity(PAGE_SIZE),
        }
    }

    pub const fn len(&self) -> usize {
        self.buf.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

/// A log segment backed by a [Vec<u8>].
///
/// Writing to the segment behaves like a file opened with `O_APPEND`:
/// [`io::Write::write`] always appends to the segment, regardless of the
/// current position, and updates the position to the new length of the segment.
/// The initial position is zero.
///
/// Note that this is not a faithful model of a file, as safe Rust requires to
/// protect the buffer with a lock. This means that pathological situations
/// arising from concurrent read/write access of a file are impossible to occur.
#[derive(Clone, Debug)]
pub struct Segment {
    pos: u64,
    storage: SharedLock<Storage>,
    space: SpaceOnDevice,
}

impl Segment {
    pub fn new(space: u64) -> Self {
        Self::from_shared(Arc::new(Mutex::new(space)), Arc::new(RwLock::new(Storage::new())))
    }

    pub(super) fn from_shared(space: SpaceOnDevice, storage: SharedLock<Storage>) -> Self {
        Self { pos: 0, space, storage }
    }

    pub fn len(&self) -> usize {
        self.storage.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.storage.read().unwrap().is_empty()
    }

    pub fn modify_byte_at(&mut self, pos: usize, f: impl FnOnce(u8) -> u8) {
        let mut storage = self.storage.write().unwrap();
        storage.buf[pos] = f(storage.buf[pos])
    }

    pub fn allocated_space(&self) -> u64 {
        self.storage.read().unwrap().alloc
    }
}

impl io::Write for Segment {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut storage = self.storage.write().unwrap();

        let mut remaining = (storage.alloc - self.pos) as usize;
        // If we don't have enough space, allocate some.
        // If not enough space to write all of `buf` can be allocated,
        // just write as much as we can. The next `write` call will return
        // ENOSPC then.
        if remaining == 0 {
            let mut avail = self.space.lock().unwrap();
            if *avail == 0 {
                return Err(enospc());
            }

            let want = (buf.len() - remaining).next_multiple_of(PAGE_SIZE);
            let have = want.min(*avail as usize);

            storage.alloc += have as u64;
            *avail -= have as u64;
            remaining = (storage.alloc - self.pos) as usize;
        }

        let read = buf.len().min(remaining);
        storage.buf.extend(&buf[..read]);
        self.pos += read as u64;

        Ok(read)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Read for Segment {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let storage = self.storage.read().unwrap();

        let Some(remaining) = storage.len().checked_sub(self.pos as usize) else {
            return Ok(0);
        };
        let want = remaining.min(buf.len());
        let pos = self.pos as usize;
        buf[..want].copy_from_slice(&storage.buf[pos..pos + want]);
        self.pos += want as u64;

        Ok(want)
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

impl SegmentLen for Segment {
    fn segment_len(&mut self) -> io::Result<u64> {
        Ok(self.len() as _)
    }
}

impl FileLike for Segment {
    fn fsync(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn ftruncate(&mut self, _tx_offset: u64, size: u64) -> io::Result<()> {
        let mut storage = self.storage.write().unwrap();
        let mut avail = self.space.lock().unwrap();

        // NOTE: We don't modify `self.pos`, which is how `ftruncate(2)` behaves.
        // This means the position can be invalid after calling this.
        if size > storage.alloc {
            if *avail == 0 {
                return Err(enospc());
            }

            let want = size.next_multiple_of(PAGE_SIZE as u64) - storage.alloc;
            let have = want.min(*avail);

            storage.alloc += have;
            *avail -= have;
            storage.buf.resize(size as usize, 0);

            // NOTE: `ftruncate(2)` is a bit ambiguous as to what should happen
            // if the requested size exceeds the available space.
            //
            // [std::fs::File::set_len] will succeed, but all subsequent
            // operations return EBADF.
            //
            // That's not super helpful, so instead we zero out as much space as
            // possible, and return ENOSPC if more than that was requested.
            if want > have {
                return Err(enospc());
            }
        } else {
            let alloc = size.next_multiple_of(PAGE_SIZE as u64);
            *avail += storage.alloc - alloc;
            storage.alloc = alloc;
            storage.buf.resize(size as usize, 0);
        }

        Ok(())
    }

    #[cfg(feature = "fallocate")]
    fn fallocate(&mut self, size: u64) -> io::Result<()> {
        let mut storage = self.storage.write().unwrap();

        if size <= storage.alloc {
            return Ok(());
        }

        let mut avail = self.space.lock().unwrap();
        if *avail == 0 {
            return Err(enospc());
        }

        let want = size.next_multiple_of(PAGE_SIZE as u64) - storage.alloc;
        let have = want.min(*avail);
        storage.alloc += have;
        *avail -= have;

        if want > have {
            return Err(enospc());
        }

        Ok(())
    }
}

fn enospc() -> io::Error {
    io::Error::new(io::ErrorKind::StorageFull, "no space left on device")
}

#[cfg(feature = "streaming")]
mod async_impls {
    use super::*;

    use std::{
        io::{Read as _, Seek as _, Write as _},
        pin::Pin,
        task::{Context, Poll},
    };

    use tokio::io::{self, AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};

    use crate::stream::{AsyncFsync, AsyncLen, IntoAsyncWriter};

    impl IntoAsyncWriter for Segment {
        type AsyncWriter = tokio::io::BufWriter<Self>;

        fn into_async_writer(self) -> Self::AsyncWriter {
            tokio::io::BufWriter::new(self)
        }
    }

    impl AsyncRead for Segment {
        fn poll_read(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            let res = self.get_mut().read(buf.initialize_unfilled());
            if let Ok(read) = &res {
                buf.advance(*read);
            }
            Poll::Ready(res.map(drop))
        }
    }

    impl AsyncWrite for Segment {
        fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
            Poll::Ready(self.get_mut().write(buf))
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncSeek for Segment {
        fn start_seek(self: Pin<&mut Self>, position: io::SeekFrom) -> io::Result<()> {
            self.get_mut().seek(position).map(drop)
        }

        fn poll_complete(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<u64>> {
            Poll::Ready(self.get_mut().stream_position())
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
