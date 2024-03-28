use std::io::{self, Read, Seek as _};

use log::{debug, warn};

use crate::{
    commit::Commit,
    dio::{PagedReader, PagedWriter, WriteAt, BLOCK_SIZE},
    error,
    segment::{self, FileLike},
    Options,
};

pub(crate) mod fs;
#[cfg(test)]
pub mod mem;

pub use fs::Fs;
#[cfg(test)]
pub use mem::Memory;

/// A repository of log segments.
///
/// This is mainly an internal trait to allow testing against an in-memory
/// representation.
pub trait Repo: Clone {
    /// The type of log segments managed by this repo, which must behave like a file.
    type Segment: io::Read + io::Write + io::Seek + WriteAt + FileLike;

    /// Create a new segment with the minimum transaction offset `offset`.
    ///
    /// This **must** create the segment atomically, and return
    /// [`io::ErrorKind::AlreadyExists`] if the segment already exists.
    ///
    /// It is permissible, however, to successfully return the new segment if
    /// it is completely empty (i.e. [`create_segment_writer`] did not previously
    /// succeed in writing the segment header).
    fn create_segment(&self, offset: u64) -> io::Result<Self::Segment>;

    /// Open an existing segment at the minimum transaction offset `offset`.
    ///
    /// Must return [`io::ErrorKind::NotFound`] if a segment with the given
    /// `offset` does not exist.
    ///
    /// The method does not guarantee that the segment is non-empty -- this case
    /// will be caught by [`open_segment_writer`] and [`open_segment_reader`]
    /// respectively.
    fn open_segment(&self, offset: u64) -> io::Result<Self::Segment>;

    /// Remove the segment at the minimum transaction offset `offset`.
    ///
    /// Return [`io::ErrorKind::NotFound`] if no such segment exists.
    fn remove_segment(&self, offset: u64) -> io::Result<()>;

    /// Traverse all segments in this repository and return a list of their
    /// offsets, sorted in ascending order.
    fn existing_offsets(&self) -> io::Result<Vec<u64>>;
}

pub type SegmentWriter<T> = segment::Writer<PagedWriter<T>>;
pub type SegmentReader<T> = segment::Reader<PagedReader<T>>;

/// Create a new segment [`Writer`] with `offset`.
///
/// Immediately attempts to write the segment header with the supplied
/// `log_format_version`.
///
/// If the segment already exists, [`io::ErrorKind::AlreadyExists`] is returned.
pub fn create_segment_writer<R: Repo>(repo: &R, opts: Options, offset: u64) -> io::Result<SegmentWriter<R::Segment>> {
    let mut storage = repo.create_segment(offset).map(PagedWriter::new)?;

    // Eagerly write the header.
    segment::Header {
        log_format_version: opts.log_format_version,
        checksum_algorithm: Commit::CHECKSUM_ALGORITHM,
    }
    .write(&mut storage)?;
    storage.sync_all()?;

    Ok(segment::Writer {
        commit: Commit {
            min_tx_offset: offset,
            n: 0,
            records: Vec::new(),
        },
        inner: storage,

        min_tx_offset: offset,
        bytes_written: segment::Header::LEN as u64,

        max_records_in_commit: opts.max_records_in_commit,
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
) -> io::Result<Result<SegmentWriter<R::Segment>, segment::Metadata>> {
    let mut storage = repo.open_segment(offset).map(PagedReader::new)?;
    let meta = match segment::Metadata::extract(offset, &mut storage) {
        Err(error::SegmentMetadata::InvalidCommit { sofar, source }) => {
            warn!("invalid commit in segment {offset}: {source}");
            debug!("sofar={sofar:?}");
            return Ok(Err(sofar));
        }
        Err(error::SegmentMetadata::Io(e)) => {
            warn!("metadata I/O error: {e}");
            return Err(e);
        }
        Ok(meta) => meta,
    };
    // Force creation of a fresh segment if the version is outdated.
    if meta.header.log_format_version < opts.log_format_version {
        return Ok(Err(meta));
    }
    // Error if the version is newer than what we know.
    meta.header
        .ensure_compatible(opts.log_format_version, Commit::CHECKSUM_ALGORITHM)
        .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;

    debug!("resume segment {offset} from: segment::{meta:?}");

    let segment::Metadata {
        header: _,
        tx_range,
        size_in_bytes,
    } = meta;

    // To avoid padding in the middle of a segment, we may need to rewrite
    // the last block on the next flush. Below adjusts the file position and
    // page buffer contents for that.
    let (mut file, mut page, filled) = storage.into_raw_parts();
    let mut file_pos = file.stream_position()?;
    debug!("read: file_pos={} pos={} filled={}", file_pos, page.pos(), filled);
    // `filled` is zero if we hit EOF after consuming a full commit.
    //
    // This does not mean `file_pos` is on a block boundary, because the file
    // may not contain padding due to truncation or because it was written
    // without O_DIRECT.
    if filled > 0 {
        // Set the file position to the start of the last block.
        file_pos = size_in_bytes
            .next_multiple_of(BLOCK_SIZE as u64)
            .saturating_sub(BLOCK_SIZE as u64);
        // Move the last block (with padding) to the start of the buffer.
        let last_block = page.pos().next_multiple_of(BLOCK_SIZE).saturating_sub(BLOCK_SIZE)..filled;
        page.buf_mut().copy_within(last_block, 0);
        // Set the buffer position to the bytes consumed without padding.
        let data_bytes = (size_in_bytes - file_pos) as usize;
        page.set_pos(data_bytes);
    } else {
        debug_assert_eq!(page.pos(), 0);
        // If we didn't read a full block, rewind to the previous block
        // and re-read the remaining data into the page buffer.
        if file_pos % BLOCK_SIZE as u64 != 0 {
            file_pos = size_in_bytes
                .next_multiple_of(BLOCK_SIZE as u64)
                .saturating_sub(BLOCK_SIZE as u64);
            let n = file.read(&mut page.buf_mut()[..BLOCK_SIZE])?;
            page.set_pos(n);
        }
    }
    debug!("adjusted: file_pos={file_pos} pos={}", page.pos());

    let writer = PagedWriter::from_raw_parts(file, file_pos, page);

    Ok(Ok(segment::Writer {
        commit: Commit {
            min_tx_offset: tx_range.end,
            n: 0,
            records: Vec::new(),
        },
        inner: writer,

        min_tx_offset: tx_range.start,
        bytes_written: size_in_bytes,

        max_records_in_commit: opts.max_records_in_commit,
    }))
}

/// Open the existing segment at `offset` for reading.
///
/// Unlike [`open_segment_writer`], this does not traverse the segment. It does,
/// however, attempt to read the segment header and checks that the log format
/// version and checksum algorithm are compatible.
pub fn open_segment_reader<R: Repo>(
    repo: &R,
    max_log_format_version: u8,
    offset: u64,
) -> io::Result<SegmentReader<R::Segment>> {
    debug!("open segment reader at {offset}");
    let storage = repo.open_segment(offset).map(PagedReader::new)?;
    segment::Reader::new(max_log_format_version, offset, storage)
}
