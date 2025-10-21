use std::{
    collections::{btree_map, BTreeMap},
    io,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, RwLock,
    },
};

use log::debug;

use super::Repo;

mod page;
pub use page::{Page, PAGE_SIZE};

mod segment;
pub use segment::Segment;

type SharedLock<T> = Arc<RwLock<T>>;
type SharedPages = SharedLock<Vec<Page>>;

/// The total capacity of the imaginary storage device.
///
/// [Segment]s are allocated from [Memory], which tracks the total space it
/// has available. [SpaceOnDevice] is shared by each [Segment]. When a [Segment]
/// allocates a [Page], it deducts the page's size from the space, returning
/// an error if [SpaceOnDevice] goes below zero.
pub type SpaceOnDevice = Arc<AtomicI64>;

#[cfg(feature = "streaming")]
mod async_impls {
    use super::*;

    use crate::stream::AsyncRepo;

    impl AsyncRepo for Memory {
        type AsyncSegmentWriter = tokio::io::BufWriter<Segment>;
        type AsyncSegmentReader = tokio::io::BufReader<Segment>;

        async fn open_segment_reader_async(&self, offset: u64) -> io::Result<Self::AsyncSegmentReader> {
            self.open_segment_writer(offset).map(tokio::io::BufReader::new)
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
            space: Arc::new(AtomicI64::new(total_space.min(i64::MAX as u64) as i64)),
            segments: <_>::default(),
        }
    }
}

impl Repo for Memory {
    type SegmentWriter = Segment;
    type SegmentReader = io::BufReader<Segment>;

    fn create_segment(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        debug!("create_segment: space={}", self.space.load(Ordering::Relaxed));
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
    use std::io::{Read, Seek, Write};

    use pretty_assertions::assert_matches;
    use tempfile::tempfile;
    use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite};

    use super::*;
    use crate::{segment::FileLike as _, tests::helpers::enable_logging};

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

    async fn async_read_write_seek(f: &mut (impl AsyncRead + AsyncSeek + AsyncWrite + Unpin)) {
        use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _, AsyncWriteExt as _};

        enable_logging();

        f.write_all(b"alonso").await.unwrap();

        f.seek(io::SeekFrom::Start(0)).await.unwrap();
        let mut buf = [0; 6];
        f.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"alonso");

        f.seek(io::SeekFrom::Start(2)).await.unwrap();
        let n = f.read(&mut buf).await.unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], b"onso");

        f.seek(io::SeekFrom::Current(-4)).await.unwrap();
        let n = f.read(&mut buf).await.unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], b"onso");

        f.seek(io::SeekFrom::End(-3)).await.unwrap();
        let n = f.read(&mut buf).await.unwrap();
        assert_eq!(n, 3);
        assert_eq!(&buf[0..3], b"nso");

        f.seek(io::SeekFrom::End(4096)).await.unwrap();
        let n = f.read(&mut buf).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn std_file_async_read_write_seek() {
        let tmp = tempfile().unwrap();
        async_read_write_seek(&mut tokio::fs::File::from_std(tmp)).await
    }

    #[tokio::test]
    async fn segment_async_read_write_seek() {
        let space_on_device = Arc::new(AtomicI64::new(4096));
        async_read_write_seek(&mut Segment::new(space_on_device)).await
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
        assert_eq!(segment.page_count(), 2);

        // Extend beyond available space returns `StorageFull`.
        assert_matches!(
            segment.ftruncate(42, 9216),
            Err(e) if e.kind() == io::ErrorKind::StorageFull
        );

        // Shrink deallocates pages.
        segment.ftruncate(42, 512).unwrap();
        buf.clear();
        read_from_start_to_end(&mut segment, &mut buf).unwrap();
        assert_eq!(buf, data);
        assert_eq!(segment.page_count(), 1);

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
        assert_eq!(segment.page_count(), 1);

        // Extend beyond page allocates new page.
        segment.fallocate(5120).unwrap();
        buf.clear();
        read_from_start_to_end(&mut segment, &mut buf).unwrap();
        assert_eq!(buf, data);
        assert_eq!(segment.page_count(), 2);

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
        assert_eq!(segment.page_count(), 2);
    }

    #[test]
    fn write_many_pages() {
        enable_logging();

        let space_on_device = Arc::new(AtomicI64::new(4096 * 4));
        let mut segment = Segment::new(space_on_device);

        let data = [b'y'; 4096];
        for _ in 0..4 {
            segment.write_all(&data[..2048]).unwrap();
            segment.write_all(&data[2048..]).unwrap();
        }
        assert_matches!(
            segment.write_all(&data[..2048]),
            Err(e) if e.kind() == io::ErrorKind::StorageFull
        );
        segment.rewind().unwrap();

        let mut buf = [0; 4096];
        for _ in 0..4 {
            segment.read_exact(&mut buf).unwrap();
            assert!(buf.iter().all(|&x| x == b'y'));
        }
        assert_matches!(
            segment.read_exact(&mut buf),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof
        );
    }

    fn read_from_start_to_end(f: &mut (impl Read + Seek), buf: &mut Vec<u8>) -> io::Result<usize> {
        f.rewind()?;
        f.read_to_end(buf)
    }
}
