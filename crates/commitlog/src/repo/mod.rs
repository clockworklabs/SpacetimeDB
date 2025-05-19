use std::io;

use log::{debug, warn};

use crate::{
    commit::Commit,
    error,
    index::{IndexFile, IndexFileMut},
    segment::{FileLike, Header, Metadata, OffsetIndexWriter, Reader, Writer},
    Options,
};

pub(crate) mod fs;
#[cfg(any(test, feature = "test"))]
pub mod mem;

pub use fs::Fs;
#[cfg(any(test, feature = "test"))]
pub use mem::Memory;

pub type TxOffset = u64;
pub type TxOffsetIndexMut = IndexFileMut<TxOffset>;
pub type TxOffsetIndex = IndexFile<TxOffset>;

pub trait SegmentLen: io::Seek {
    /// Determine the length in bytes of the segment.
    ///
    /// This method does not rely on metadata `fsync`, and may use up to three
    /// `seek` operations.
    ///
    /// If the method returns successfully, the seek position before the call is
    /// restored. However, if it returns an error, the seek position is
    /// unspecified.
    //
    // TODO: Remove trait and replace with `Seek::stream_len` if / when stabilized:
    // https://github.com/rust-lang/rust/issues/59359
    fn segment_len(&mut self) -> io::Result<u64> {
        let old_pos = self.stream_position()?;
        let len = self.seek(io::SeekFrom::End(0))?;

        // Avoid seeking a third time when we were already at the end of the
        // stream. The branch is usually way cheaper than a seek operation.
        if old_pos != len {
            self.seek(io::SeekFrom::Start(old_pos))?;
        }

        Ok(len)
    }
}

pub trait SegmentReader: io::BufRead + SegmentLen + Send + Sync {}
impl<T: io::BufRead + SegmentLen + Send + Sync> SegmentReader for T {}

pub trait SegmentWriter: FileLike + io::Read + io::Write + SegmentLen + Send + Sync {}
impl<T: FileLike + io::Read + io::Write + SegmentLen + Send + Sync> SegmentWriter for T {}

/// A repository of log segments.
///
/// This is mainly an internal trait to allow testing against an in-memory
/// representation.
pub trait Repo: Clone {
    /// The type of log segments managed by this repo, which must behave like a file.
    type SegmentWriter: SegmentWriter + 'static;
    type SegmentReader: SegmentReader + 'static;

    /// Create a new segment with the minimum transaction offset `offset`.
    ///
    /// This **must** create the segment atomically, and return
    /// [`io::ErrorKind::AlreadyExists`] if the segment already exists.
    ///
    /// It is permissible, however, to successfully return the new segment if
    /// it is completely empty (i.e. [`create_segment_writer`] did not previously
    /// succeed in writing the segment header).
    fn create_segment(&self, offset: u64) -> io::Result<Self::SegmentWriter>;

    /// Open an existing segment at the minimum transaction offset `offset`.
    ///
    /// Must return [`io::ErrorKind::NotFound`] if a segment with the given
    /// `offset` does not exist.
    ///
    /// The method does not guarantee that the segment is non-empty -- this case
    /// will be caught by [`open_segment_reader`].
    fn open_segment_reader(&self, offset: u64) -> io::Result<Self::SegmentReader>;

    /// Open an existing segment at the minimum transaction offset `offset`.
    ///
    /// Must return [`io::ErrorKind::NotFound`] if a segment with the given
    /// `offset` does not exist.
    ///
    /// The method does not guarantee that the segment is non-empty -- this case
    /// will be caught by [`resume_segment_writer`].
    fn open_segment_writer(&self, offset: u64) -> io::Result<Self::SegmentWriter>;

    /// Remove the segment at the minimum transaction offset `offset`.
    ///
    /// Return [`io::ErrorKind::NotFound`] if no such segment exists.
    fn remove_segment(&self, offset: u64) -> io::Result<()>;

    /// Compress a segment in storage, marking it as immutable.
    fn compress_segment(&self, offset: u64) -> io::Result<()>;

    /// Traverse all segments in this repository and return list of their
    /// offsets, sorted in ascending order.
    fn existing_offsets(&self) -> io::Result<Vec<u64>>;

    /// Create [`TxOffsetIndexMut`] for the given `offset` or open it if already exist.
    /// The `cap` parameter is the maximum number of entries in the index.
    fn create_offset_index(&self, _offset: TxOffset, _cap: u64) -> io::Result<TxOffsetIndexMut> {
        Err(io::Error::new(io::ErrorKind::Other, "not implemented"))
    }

    /// Remove [`TxOffsetIndexMut`] named with `offset`.
    fn remove_offset_index(&self, _offset: TxOffset) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "not implemented"))
    }

    /// Get [`TxOffsetIndex`] for the given `offset`.
    fn get_offset_index(&self, _offset: TxOffset) -> io::Result<TxOffsetIndex> {
        Err(io::Error::new(io::ErrorKind::Other, "not implemented"))
    }
}

impl<T: Repo> Repo for &T {
    type SegmentWriter = T::SegmentWriter;
    type SegmentReader = T::SegmentReader;

    fn create_segment(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        T::create_segment(self, offset)
    }

    fn open_segment_reader(&self, offset: u64) -> io::Result<Self::SegmentReader> {
        T::open_segment_reader(self, offset)
    }

    fn open_segment_writer(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        T::open_segment_writer(self, offset)
    }

    fn remove_segment(&self, offset: u64) -> io::Result<()> {
        T::remove_segment(self, offset)
    }

    fn compress_segment(&self, offset: u64) -> io::Result<()> {
        T::compress_segment(self, offset)
    }

    fn existing_offsets(&self) -> io::Result<Vec<u64>> {
        T::existing_offsets(self)
    }

    fn create_offset_index(&self, offset: TxOffset, cap: u64) -> io::Result<TxOffsetIndexMut> {
        T::create_offset_index(self, offset, cap)
    }

    /// Remove [`TxOffsetIndexMut`] named with `offset`.
    fn remove_offset_index(&self, offset: TxOffset) -> io::Result<()> {
        T::remove_offset_index(self, offset)
    }

    /// Get [`TxOffsetIndex`] for the given `offset`.
    fn get_offset_index(&self, offset: TxOffset) -> io::Result<TxOffsetIndex> {
        T::get_offset_index(self, offset)
    }
}

impl<T: SegmentLen> SegmentLen for io::BufReader<T> {
    fn segment_len(&mut self) -> io::Result<u64> {
        SegmentLen::segment_len(self.get_mut())
    }
}

pub(crate) fn create_offset_index_writer<R: Repo>(repo: &R, offset: u64, opts: Options) -> Option<OffsetIndexWriter> {
    repo.create_offset_index(offset, opts.offset_index_len())
        .map(|index| OffsetIndexWriter::new(index, opts))
        .map_err(|e| {
            warn!("failed to get offset index for segment {offset}: {e}");
        })
        .ok()
}

/// Create a new segment [`Writer`] with `offset`.
///
/// Immediately attempts to write the segment header with the supplied
/// `log_format_version`.
///
/// If the segment already exists, [`io::ErrorKind::AlreadyExists`] is returned.
pub fn create_segment_writer<R: Repo>(
    repo: &R,
    opts: Options,
    epoch: u64,
    offset: u64,
) -> io::Result<Writer<R::SegmentWriter>> {
    let mut storage = repo.create_segment(offset)?;
    Header {
        log_format_version: opts.log_format_version,
        checksum_algorithm: Commit::CHECKSUM_ALGORITHM,
    }
    .write(&mut storage)?;
    storage.fsync()?;

    Ok(Writer {
        commit: Commit {
            min_tx_offset: offset,
            n: 0,
            records: Vec::new(),
            epoch,
        },
        inner: io::BufWriter::new(storage),

        min_tx_offset: offset,
        bytes_written: Header::LEN as u64,

        max_records_in_commit: opts.max_records_in_commit,

        offset_index_head: create_offset_index_writer(repo, offset, opts),
    })
}

/// Open the existing segment at `offset` for writing.
///
/// This will traverse the segment in order to find the offset of the next
/// commit to write to it, which may fail for various reasons.
///
/// If the traversal is successful, the segment header is checked against the
/// `max_log_format_version`, and [`io::ErrorKind::InvalidData`] is returned if
/// the segment's log format version is greater than the given value. Likewise
/// if the checksum algorithm stored in the segment header cannot be handled
/// by this crate.
///
/// If only a (non-empty) prefix of the segment could be read due to a failure
/// to decode a [`Commit`], the segment [`Metadata`] read up to the faulty
/// commit is returned in an `Err`. In this case, a new segment should be
/// created for writing.
pub fn resume_segment_writer<R: Repo>(
    repo: &R,
    opts: Options,
    offset: u64,
) -> io::Result<Result<Writer<R::SegmentWriter>, Metadata>> {
    let mut storage = repo.open_segment_writer(offset)?;
    let offset_index = repo.get_offset_index(offset).ok();
    let Metadata {
        header,
        tx_range,
        size_in_bytes,
        max_epoch,
        max_commit_offset: _,
    } = match Metadata::extract(offset, &mut storage, offset_index.as_ref()) {
        Err(error::SegmentMetadata::InvalidCommit { sofar, source }) => {
            warn!("invalid commit in segment {offset}: {source}");
            debug!("sofar={sofar:?}");
            return Ok(Err(sofar));
        }
        Err(error::SegmentMetadata::Io(e)) => return Err(e),
        Ok(meta) => meta,
    };
    header
        .ensure_compatible(opts.log_format_version, Commit::CHECKSUM_ALGORITHM)
        .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;
    // When resuming, the log format version must be equal.
    if header.log_format_version != opts.log_format_version {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "log format version mismatch: current={} segment={}",
                opts.log_format_version, header.log_format_version
            ),
        ));
    }

    Ok(Ok(Writer {
        commit: Commit {
            min_tx_offset: tx_range.end,
            n: 0,
            records: Vec::new(),
            epoch: max_epoch,
        },
        inner: io::BufWriter::new(storage),

        min_tx_offset: tx_range.start,
        bytes_written: size_in_bytes,

        max_records_in_commit: opts.max_records_in_commit,

        offset_index_head: create_offset_index_writer(repo, offset, opts),
    }))
}

/// Open the existing segment at `offset` for reading.
///
/// Unlike [`resume_segment_writer`], this does not traverse the segment. It
/// does, however, attempt to read the segment header and checks that the log
/// format version and checksum algorithm are compatible.
pub fn open_segment_reader<R: Repo>(
    repo: &R,
    max_log_format_version: u8,
    offset: u64,
) -> io::Result<Reader<R::SegmentReader>> {
    debug!("open segment reader at {offset}");
    let storage = repo.open_segment_reader(offset)?;
    Reader::new(max_log_format_version, offset, storage)
}
