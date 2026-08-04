use std::{
    collections::{btree_map, BTreeMap},
    fmt, io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
};

use spacetimedb_commitlog::{
    repo::{CompressOnce, CompressionStats, Repo, RepoWithoutLockFile, SegmentLen, SegmentReader, TxOffset},
    segment::{FileLike, Header},
    Commitlog, Decoder, Options, Transaction,
};
use spacetimedb_durability::{Close, Durability, DurableOffset, History, PreparedTx};
use spacetimedb_engine::relational_db::Txdata;
use spacetimedb_runtime::sync::watch;

#[derive(Clone, Debug)]
pub struct InMemoryCommitlog {
    repo: Memory,
    options: Options,
}

impl Default for InMemoryCommitlog {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryCommitlog {
    pub fn new() -> Self {
        Self {
            repo: Memory::unlimited(),
            options: Options::default(),
        }
    }

    pub fn open_handle(&self) -> io::Result<InMemoryCommitlogHandle> {
        InMemoryCommitlogHandle::open(self.repo.clone(), self.options)
    }
}

#[derive(Clone)]
pub struct InMemoryCommitlogHandle {
    inner: Arc<HandleInner>,
}

struct HandleInner {
    log: Commitlog<Txdata, Memory>,
    durable_tx: watch::Sender<Option<TxOffset>>,
    closed: AtomicBool,
}

impl InMemoryCommitlogHandle {
    fn open(repo: Memory, options: Options) -> io::Result<Self> {
        let log = Commitlog::open_with_repo(repo, options)?;
        let (durable_tx, _) = watch::channel(log.max_committed_offset());
        Ok(Self {
            inner: Arc::new(HandleInner {
                log,
                durable_tx,
                closed: AtomicBool::new(false),
            }),
        })
    }
}

impl Durability for InMemoryCommitlogHandle {
    type TxData = Txdata;

    fn append_tx(&self, tx: PreparedTx<Self::TxData>) {
        assert!(
            !self.inner.closed.load(Ordering::Acquire),
            "in-memory commitlog durability is closed"
        );

        let tx = tx.into_transaction();
        self.inner.log.commit([tx]).expect("in-memory commitlog append failed");
        let durable_offset = self
            .inner
            .log
            .flush_and_sync()
            .expect("in-memory commitlog flush failed");
        let _ = self.inner.durable_tx.send(durable_offset);
    }

    fn durable_tx_offset(&self) -> DurableOffset {
        self.inner.durable_tx.subscribe().into()
    }

    fn close(&self) -> Close {
        self.inner.closed.store(true, Ordering::Release);
        let durable_offset = self.inner.log.max_committed_offset();
        let _ = self.inner.durable_tx.send(durable_offset);
        Box::pin(async move { durable_offset })
    }
}

impl History for InMemoryCommitlogHandle {
    type TxData = Txdata;

    fn fold_transactions_from<D>(&self, offset: TxOffset, decoder: D) -> Result<(), D::Error>
    where
        D: Decoder,
        D::Error: From<spacetimedb_commitlog::error::Traversal>,
    {
        self.inner.log.fold_transactions_from(offset, decoder)
    }

    fn transactions_from<'a, D>(
        &self,
        offset: TxOffset,
        decoder: &'a D,
    ) -> impl Iterator<Item = Result<Transaction<Self::TxData>, D::Error>>
    where
        D: Decoder<Record = Self::TxData>,
        D::Error: From<spacetimedb_commitlog::error::Traversal>,
        Self::TxData: 'a,
    {
        self.inner.log.transactions_from(offset, decoder)
    }

    fn tx_range_hint(&self) -> (TxOffset, Option<TxOffset>) {
        let min = self.inner.log.min_committed_offset().unwrap_or_default();
        let max = self.inner.log.max_committed_offset();

        (min, max)
    }
}

const PAGE_SIZE: usize = 4096;

type SharedLock<T> = Arc<RwLock<T>>;
type SpaceOnDevice = Arc<Mutex<u64>>;

#[derive(Clone, Debug)]
pub struct Memory {
    space: SpaceOnDevice,
    segments: SharedLock<BTreeMap<u64, SharedLock<Storage>>>,
}

impl Memory {
    pub fn new(total_space: u64) -> Self {
        Self {
            space: Arc::new(Mutex::new(total_space)),
            segments: <_>::default(),
        }
    }

    pub fn unlimited() -> Self {
        Self::new(u64::MAX)
    }
}

impl fmt::Display for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<dst-memory>")
    }
}

impl Repo for Memory {
    type SegmentWriter = Segment;
    type SegmentReader = ReadOnlySegment;

    fn create_segment(&self, offset: u64, header: Header) -> io::Result<Self::SegmentWriter> {
        let mut inner = self.segments.write().unwrap();
        let mut segment = match inner.entry(offset) {
            btree_map::Entry::Occupied(entry) => {
                let entry = entry.get();
                if entry.read().unwrap().is_empty() {
                    Segment::from_shared(self.space.clone(), entry.clone())
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("segment {offset} already exists"),
                    ));
                }
            }
            btree_map::Entry::Vacant(entry) => {
                let storage = entry.insert(Arc::new(RwLock::new(Storage::new())));
                Segment::from_shared(self.space.clone(), storage.clone())
            }
        };
        header.write(&mut segment)?;

        Ok(segment)
    }

    fn open_segment_reader(&self, offset: u64) -> io::Result<Self::SegmentReader> {
        self.open_segment_writer(offset).map(Into::into)
    }

    fn open_segment_writer(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        let inner = self.segments.read().unwrap();
        let Some(buf) = inner.get(&offset) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("segment {offset} does not exist"),
            ));
        };
        Ok(Segment::from_shared(self.space.clone(), buf.clone()))
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

    fn compress_segment_with(&self, _: u64, _: impl CompressOnce) -> io::Result<CompressionStats> {
        Ok(<_>::default())
    }

    fn existing_offsets(&self) -> io::Result<Vec<u64>> {
        Ok(self.segments.read().unwrap().keys().copied().collect())
    }
}

impl RepoWithoutLockFile for Memory {}

#[derive(Debug)]
struct Storage {
    alloc: u64,
    buf: Vec<u8>,
}

impl Storage {
    fn new() -> Self {
        Self {
            alloc: 0,
            buf: Vec::with_capacity(PAGE_SIZE),
        }
    }

    const fn len(&self) -> usize {
        self.buf.len()
    }

    const fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct Segment {
    pos: u64,
    storage: SharedLock<Storage>,
    space: SpaceOnDevice,
}

impl Segment {
    fn from_shared(space: SpaceOnDevice, storage: SharedLock<Storage>) -> Self {
        Self { pos: 0, storage, space }
    }

    fn len(&self) -> usize {
        self.storage.read().unwrap().len()
    }
}

impl io::Write for Segment {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut storage = self.storage.write().unwrap();
        let requested_end = self
            .pos
            .checked_add(buf.len() as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "write position overflow"))?;

        if requested_end > storage.alloc {
            let mut avail = self.space.lock().unwrap();

            if self.pos >= storage.alloc {
                let minimum_alloc = next_page_multiple(
                    self.pos
                        .checked_add(1)
                        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "write position overflow"))?,
                )?;
                let needed = minimum_alloc - storage.alloc;
                if *avail < needed {
                    return Err(enospc());
                }
            }

            let target_alloc = next_page_multiple(requested_end)?;
            let wanted = target_alloc - storage.alloc;
            let available = wanted.min(*avail);

            storage.alloc += available;
            *avail -= available;
        }

        debug_assert!(self.pos < storage.alloc);
        let writable = buf.len().min((storage.alloc - self.pos) as usize);
        let start = self.pos as usize;
        let end = start + writable;

        if storage.buf.len() < start {
            storage.buf.resize(start, 0);
        }
        if storage.buf.len() < end {
            storage.buf.resize(end, 0);
        }
        storage.buf[start..end].copy_from_slice(&buf[..writable]);
        self.pos += writable as u64;

        Ok(writable)
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
        Ok(self.len() as u64)
    }
}

impl FileLike for Segment {
    fn fsync(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn ftruncate(&mut self, _tx_offset: u64, size: u64) -> io::Result<()> {
        let mut storage = self.storage.write().unwrap();
        let mut avail = self.space.lock().unwrap();

        if size > storage.alloc {
            if *avail == 0 {
                return Err(enospc());
            }

            let want = size.next_multiple_of(PAGE_SIZE as u64) - storage.alloc;
            let have = want.min(*avail);

            storage.alloc += have;
            *avail -= have;
            storage.buf.resize(size as usize, 0);

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

    fn fallocate(&mut self, _size: u64) -> io::Result<()> {
        Ok(())
    }
}

pub struct ReadOnlySegment {
    inner: io::BufReader<Segment>,
}

impl From<Segment> for ReadOnlySegment {
    fn from(inner: Segment) -> Self {
        Self {
            inner: io::BufReader::new(inner),
        }
    }
}

impl SegmentReader for ReadOnlySegment {
    fn sealed(&self) -> bool {
        false
    }
}

impl io::Read for ReadOnlySegment {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl io::BufRead for ReadOnlySegment {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.inner.consume(amount);
    }
}

impl io::Seek for ReadOnlySegment {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl SegmentLen for ReadOnlySegment {}

fn next_page_multiple(size: u64) -> io::Result<u64> {
    let page = PAGE_SIZE as u64;
    let remainder = size % page;
    if remainder == 0 {
        return Ok(size);
    }

    size.checked_add(page - remainder)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "allocation size overflow"))
}

fn enospc() -> io::Error {
    io::Error::new(io::ErrorKind::StorageFull, "no space left on device")
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Seek, Write};

    use super::*;

    fn segment() -> Segment {
        Segment::from_shared(Arc::new(Mutex::new(u64::MAX)), Arc::new(RwLock::new(Storage::new())))
    }

    #[test]
    fn write_overwrites_at_seek_position() {
        let mut segment = segment();

        segment.write_all(b"abcdef").unwrap();
        segment.seek(io::SeekFrom::Start(2)).unwrap();
        segment.write_all(b"XY").unwrap();

        let mut bytes = Vec::new();
        segment.seek(io::SeekFrom::Start(0)).unwrap();
        segment.read_to_end(&mut bytes).unwrap();

        assert_eq!(bytes, b"abXYef");
    }

    #[test]
    fn write_after_end_fills_gap_with_zeroes() {
        let mut segment = segment();

        segment.seek(io::SeekFrom::Start(4)).unwrap();
        segment.write_all(&[1, 2]).unwrap();

        let mut bytes = Vec::new();
        segment.seek(io::SeekFrom::Start(0)).unwrap();
        segment.read_to_end(&mut bytes).unwrap();

        assert_eq!(bytes, &[0, 0, 0, 0, 1, 2]);
    }
}
