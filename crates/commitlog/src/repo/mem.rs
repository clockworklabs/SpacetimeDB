use std::{
    collections::{btree_map, BTreeMap},
    io,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, RwLock, RwLockWriteGuard,
    },
};

use super::Repo;
use crate::{repo::SegmentLen, segment::FileLike};

type SharedLock<T> = Arc<RwLock<T>>;
type SharedPages = SharedLock<Vec<Page>>;

const PAGE_SIZE: usize = 4096;

#[derive(Debug)]
pub struct Page {
    filled: usize,
    buf: [u8; PAGE_SIZE],
}

impl Page {
    pub fn remaining(&self) -> usize {
        PAGE_SIZE - self.filled
    }
}

impl Default for Page {
    fn default() -> Self {
        Self {
            filled: 0,
            buf: [0; PAGE_SIZE],
        }
    }
}

/// The total capacity of the imaginary storage device.
///
/// [Segment]s are allocated from [Memory], which tracks the total space it
/// has available. [SpaceOnDevice] is shared by each [Segment]. When a [Segment]
/// allocates a [Page], it deducts the page's size from the space, returning
/// an error if [SpaceOnDevice] goes below zero.
pub type SpaceOnDevice = Arc<AtomicI64>;

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

    pub fn with_pages(device: SpaceOnDevice, pages: SharedPages) -> Self {
        Self { pos: 0, pages, device }
    }

    pub fn len(&self) -> usize {
        self.pages
            .read()
            .unwrap()
            .iter()
            .fold(0, |size, page| size + page.filled)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn modify_byte_at(&mut self, pos: usize, f: impl FnOnce(u8) -> u8) {
        let mut pages = self.pages.write().unwrap();

        let page_idx = pos / PAGE_SIZE;
        let page = pages.get_mut(page_idx).expect("pos out of bounds");
        let page_ofs = pos % PAGE_SIZE;
        page.buf[page_ofs] = f(page.buf[page_ofs]);
    }

    fn allocate(&self, pages: &mut RwLockWriteGuard<'_, Vec<Page>>, n: usize) -> io::Result<()> {
        let mut allocated = 0;
        pages.resize_with(n, || {
            allocated += 1;
            Page::default()
        });
        if self.device.fetch_sub((allocated * PAGE_SIZE) as i64, Ordering::Relaxed) <= 0 {
            return Err(io::Error::new(io::ErrorKind::StorageFull, "no space left on device"));
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

            let range_in_page = page.filled..page.filled + to_copy;
            let range_in_buf = written..written + to_copy;

            page.buf[range_in_page].copy_from_slice(&buf[range_in_buf]);
            page.filled += to_copy;
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
            let pages = self.pages.read().unwrap();
            let Some(page) = pages.get(self.pos as usize / PAGE_SIZE) else {
                break;
            };
            let offset_in_page = (self.pos % PAGE_SIZE as u64) as usize;
            if offset_in_page >= page.filled {
                break;
            }
            let available_in_page = page.filled - offset_in_page;
            let to_copy = (buf.len() - read).min(available_in_page);

            buf[read..read + to_copy].copy_from_slice(&page.buf[offset_in_page..offset_in_page + to_copy]);

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
                last_page.filled = tail_start;
                last_page.buf[tail_start..].fill(0);
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
        fn poll_read(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(self.get_mut().read(buf.initialize_unfilled()).map(drop))
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

/// In-memory implementation of [`Repo`].
#[derive(Clone, Debug)]
pub struct Memory {
    space: SpaceOnDevice,
    segments: SharedLock<BTreeMap<u64, SharedPages>>,
}

impl Memory {
    pub fn new(total_space: u64) -> Self {
        Self {
            space: Arc::new(AtomicI64::new(total_space as _)),
            segments: <_>::default(),
        }
    }
}

impl Repo for Memory {
    type SegmentWriter = Segment;
    type SegmentReader = io::BufReader<Segment>;

    fn create_segment(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        let mut inner = self.segments.write().unwrap();
        match inner.entry(offset) {
            btree_map::Entry::Occupied(entry) => {
                let entry = entry.get();
                let read_guard = entry.read().unwrap();
                if read_guard.is_empty() {
                    Ok(Segment::with_pages(self.space.clone(), entry.clone()))
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("segment {offset} already exists"),
                    ))
                }
            }
            btree_map::Entry::Vacant(entry) => {
                let segment = entry.insert(SharedPages::default());
                Ok(Segment::with_pages(self.space.clone(), segment.clone()))
            }
        }
    }

    fn open_segment_writer(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        let inner = self.segments.read().unwrap();
        let Some(buf) = inner.get(&offset) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("segment {offset} does not exist"),
            ));
        };
        Ok(Segment::with_pages(self.space.clone(), buf.clone()))
    }

    fn open_segment_reader(&self, offset: u64) -> io::Result<Self::SegmentReader> {
        self.open_segment_writer(offset).map(io::BufReader::new)
    }

    fn remove_segment(&self, offset: u64) -> io::Result<()> {
        let mut inner = self.segments.write().unwrap();
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
        Ok(self.segments.read().unwrap().keys().copied().collect())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_matches;
    use tempfile::tempfile;

    use super::*;
    use std::io::{Read, Seek, Write};

    fn read_write_seek(f: &mut (impl Read + Seek + Write)) {
        f.write_all(b"alonso").unwrap();

        f.seek(io::SeekFrom::Start(0)).unwrap();
        let mut buf = [0; 6];
        f.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"alonso");

        f.seek(io::SeekFrom::Start(2)).unwrap();
        let n = f.read(&mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], b"onso");

        f.seek(io::SeekFrom::Current(-4)).unwrap();
        let n = f.read(&mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], b"onso");

        f.seek(io::SeekFrom::End(-3)).unwrap();
        let n = f.read(&mut buf).unwrap();
        assert_eq!(n, 3);
        assert_eq!(&buf[0..3], b"nso");

        f.seek(io::SeekFrom::End(4096)).unwrap();
        let n = f.read(&mut buf).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn segment_read_write_seek() {
        let space_on_device = Arc::new(AtomicI64::new(4096));
        read_write_seek(&mut Segment::new(space_on_device));
    }

    #[test]
    fn std_file_read_write_seek() {
        read_write_seek(&mut tempfile().unwrap());
    }

    #[test]
    fn ftruncate() {
        let space_on_device = Arc::new(AtomicI64::new(8192));
        let mut segment = Segment::new(space_on_device);

        let data = [b'z'; 512];
        let mut buf = Vec::with_capacity(4096);

        segment.write_all(&data).unwrap();
        read_from_start_to_end(&mut segment, &mut buf).unwrap();
        assert_eq!(&buf, &data);

        // Extend adds zeroes.
        segment.ftruncate(42, 1024).unwrap();
        buf.clear();
        read_from_start_to_end(&mut segment, &mut buf).unwrap();
        assert_eq!(&buf[..512], &data);
        assert_eq!(&buf[512..], &[0; 512]);

        // Extend beyond existing page allocates zeroed page.
        segment.ftruncate(42, 5120).unwrap();
        buf.clear();
        read_from_start_to_end(&mut segment, &mut buf).unwrap();
        assert_eq!(&buf[..512], &data);
        assert_eq!(&buf[512..], &[0; 512]);
        assert_eq!(segment.pages.read().unwrap().len(), 2);

        // Extends beyond available space returns `StorageFull`.
        assert_matches!(
            segment.ftruncate(42, 9216),
            Err(e) if e.kind() == io::ErrorKind::StorageFull
        );

        // Shrink deallocates pages.
        segment.ftruncate(42, 512).unwrap();
        buf.clear();
        read_from_start_to_end(&mut segment, &mut buf).unwrap();
        assert_eq!(buf, data);
        assert_eq!(segment.pages.read().unwrap().len(), 1);

        segment.ftruncate(42, 256).unwrap();
        buf.clear();
        read_from_start_to_end(&mut segment, &mut buf).unwrap();
        assert_eq!(buf, &data[..256]);
    }

    #[test]
    fn fallocate() {
        let space_on_device = Arc::new(AtomicI64::new(8192));
        let mut segment = Segment::new(space_on_device);

        let data = [b'z'; 512];
        let mut buf = Vec::with_capacity(4096);

        segment.write_all(&data).unwrap();
        read_from_start_to_end(&mut segment, &mut buf).unwrap();
        assert_eq!(buf, data);

        // Extend within existing page doesn't allocate.
        segment.fallocate(1024).unwrap();
        buf.clear();
        read_from_start_to_end(&mut segment, &mut buf).unwrap();
        assert_eq!(buf, data);
        assert_eq!(segment.pages.read().unwrap().len(), 1);

        // Extend beyond page allocates new page.
        segment.fallocate(5120).unwrap();
        buf.clear();
        read_from_start_to_end(&mut segment, &mut buf).unwrap();
        assert_eq!(buf, data);
        assert_eq!(segment.pages.read().unwrap().len(), 2);

        // Extend beyond available space returns `StorageFull`.
        assert_matches!(
            segment.fallocate(9216),
            Err(e) if e.kind() == io::ErrorKind::StorageFull
        );

        // Shrink does nothing.
        segment.fallocate(256).unwrap();
        buf.clear();
        read_from_start_to_end(&mut segment, &mut buf).unwrap();
        assert_eq!(buf, data);
        assert_eq!(segment.pages.read().unwrap().len(), 3);
    }

    fn read_from_start_to_end(f: &mut (impl Read + Seek), buf: &mut Vec<u8>) -> io::Result<usize> {
        f.rewind()?;
        f.read_to_end(buf)
    }
}
