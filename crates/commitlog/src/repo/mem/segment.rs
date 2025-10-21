use std::{
    io,
    sync::{atomic::Ordering, Arc, RwLock, RwLockWriteGuard},
};

use log::{debug, trace};

use crate::{
    repo::{
        mem::{Page, SpaceOnDevice, PAGE_SIZE},
        SegmentLen,
    },
    segment::FileLike,
};

type SharedLock<T> = Arc<RwLock<T>>;
type SharedPages = SharedLock<Vec<Page>>;

/// A log segment backed by a [Vec<Page>].
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
    pages: SharedPages,
    device: SpaceOnDevice,
}

impl Segment {
    pub fn new(device: SpaceOnDevice) -> Self {
        Self::with_pages(device, <_>::default())
    }

    pub(super) fn with_pages(device: SpaceOnDevice, pages: SharedPages) -> Self {
        Self { pos: 0, pages, device }
    }

    pub fn len(&self) -> usize {
        self.pages
            .read()
            .unwrap()
            .iter()
            .fold(0, |size, page| size + page.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn page_count(&self) -> usize {
        self.pages.read().unwrap().len()
    }

    pub fn modify_byte_at(&mut self, pos: usize, f: impl FnOnce(u8) -> u8) {
        let mut pages = self.pages.write().unwrap();

        let page_idx = pos / PAGE_SIZE;
        let page = pages.get_mut(page_idx).expect("pos out of bounds");
        let page_ofs = pos % PAGE_SIZE;
        page.modify_byte_at(page_ofs, f);
    }

    fn allocate(&self, pages: &mut RwLockWriteGuard<'_, Vec<Page>>, n: usize) -> io::Result<()> {
        assert!(n > pages.len());
        let page_size = PAGE_SIZE as i64;
        for _ in pages.len()..n {
            if self.device.load(Ordering::Relaxed) - page_size < 0 {
                return Err(io::Error::new(
                    io::ErrorKind::StorageFull,
                    "not enough space left on device",
                ));
            }
            pages.push(Page::new());
            if self.device.fetch_sub(page_size, Ordering::Relaxed) < 0 {
                return Err(io::Error::new(io::ErrorKind::StorageFull, "no space left on device"));
            }
        }

        Ok(())
    }
}

impl SegmentLen for Segment {
    fn segment_len(&mut self) -> io::Result<u64> {
        Ok(self.len() as u64)
    }
}

impl io::Write for Segment {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written = 0;
        while written < buf.len() {
            let mut pages = self.pages.write().unwrap();
            let page = {
                let page_idx = self.pos as usize / PAGE_SIZE;
                if page_idx >= pages.len() {
                    self.allocate(&mut pages, page_idx + 1)?;
                }
                &mut pages[page_idx]
            };
            let remaining = buf.len() - written;
            let to_copy = page.remaining().min(remaining);

            page.copy_from_slice(&buf[written..written + to_copy]);
            drop(pages);

            written += to_copy;
            self.pos += to_copy as u64;
        }

        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Read for Segment {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read = 0;
        while read < buf.len() {
            trace!("read {} from {}", buf.len(), self.pos);
            let pages = self.pages.read().unwrap();
            let Some(page) = pages.get(self.pos as usize / PAGE_SIZE) else {
                trace!("no page at pos");
                break;
            };
            let offset_in_page = (self.pos % PAGE_SIZE as u64) as usize;
            if offset_in_page >= page.len() {
                trace!("offset after initialized bytes in page");
                break;
            }
            let available_in_page = page.len() - offset_in_page;
            let to_copy = (buf.len() - read).min(available_in_page);
            trace!("available_in_page={available_in_page} to_copy={to_copy}");

            buf[read..read + to_copy].copy_from_slice(page.slice(offset_in_page..offset_in_page + to_copy));
            trace!("buf={buf:?}");

            read += to_copy;
            self.pos += to_copy as u64;
        }

        Ok(read)
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

impl FileLike for Segment {
    fn fsync(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn ftruncate(&mut self, _tx_offset: u64, size: u64) -> io::Result<()> {
        use std::cmp::Ordering::*;

        let mut pages = self.pages.write().unwrap();
        let old_page_count = pages.len() as u64;
        let new_page_count = size.next_multiple_of(PAGE_SIZE as u64) / PAGE_SIZE as u64;

        let zero_tail = |maybe_last_page: Option<&mut Page>| {
            if let Some(last_page) = maybe_last_page {
                let tail_start = (size as usize) % PAGE_SIZE;
                last_page.zeroize(tail_start);
            }
        };
        match new_page_count.cmp(&old_page_count) {
            Greater => self.allocate(&mut pages, new_page_count as usize)?,
            ordering => {
                if matches!(ordering, Less) {
                    pages.truncate(new_page_count as usize);
                }
                zero_tail(pages.last_mut());
            }
        };

        if self.pos > size {
            self.pos = size;
        }

        Ok(())
    }

    fn fallocate(&mut self, size: u64) -> io::Result<()> {
        let mut pages = self.pages.write().unwrap();
        let old_page_count = pages.len() as u64;
        let new_page_count = size.next_multiple_of(PAGE_SIZE as u64) / PAGE_SIZE as u64;

        debug!(
            "fallocate {}: old_page_count={} new_page_count={}",
            size, old_page_count, new_page_count
        );

        if new_page_count > old_page_count {
            self.allocate(&mut pages, new_page_count as usize)?;
        }

        Ok(())
    }
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
