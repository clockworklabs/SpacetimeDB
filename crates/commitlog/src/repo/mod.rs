use std::{
    fmt,
    io::{self, Seek},
};

use log::{debug, warn};

use crate::{
    commit::Commit,
    error,
    index::{IndexFile, IndexFileMut},
    segment::{self, FileLike, Header, Metadata, OffsetIndexWriter, Reader, Writer},
    Options,
};

pub(crate) mod fs;
#[cfg(any(test, feature = "test"))]
pub mod mem;

pub use fs::{Fs, OnNewSegmentFn, SizeOnDisk};
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
    ///
    /// The returned length will be the bytes actually written to the segment,
    /// not the allocated size of the segment (if the `fallocate` feature is
    /// enabled).
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

pub trait SegmentReader: io::BufRead + SegmentLen + Send + Sync {
    /// Whether the segment is considered immutable.
    ///
    /// Currently, this is true when the segment is compressed.
    /// [resume_segment_writer] uses this method to indicate that a new segment
    /// should be created when opening a commitlog.
    fn sealed(&self) -> bool;
}

pub trait SegmentWriter: FileLike + io::Read + io::Write + SegmentLen + Send + Sync {}
impl<T: FileLike + io::Read + io::Write + SegmentLen + Send + Sync> SegmentWriter for T {}

/// A repository of log segments.
///
/// This is mainly an internal trait to allow testing against an in-memory
/// representation.
///
/// The [fmt::Display] should provide context about the location of the repo,
/// e.g. the root directory for a filesystem-based implementation.
pub trait Repo: Clone + fmt::Display {
    /// The type of log segments managed by this repo, which must behave like a file.
    type SegmentWriter: SegmentWriter + 'static;
    type SegmentReader: SegmentReader + 'static;

    /// Create a new segment with the minimum transaction offset `offset`.
    ///
    /// This **must** create the segment atomically, and return
    /// [`io::ErrorKind::AlreadyExists`] if the segment already exists (it is
    /// permissible to overwrite an existing segment if it is zero-length).
    ///
    /// If the method returns successfully, the `header` **must** have been
    /// durably written to the segment.
    fn create_segment(&self, offset: u64, header: segment::Header) -> io::Result<Self::SegmentWriter>;

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

    /// Return a path-like identifier for debugging logs.
    ///
    /// This is optional and only used to enrich error messages when segment
    /// operations fail.
    fn segment_file_path(&self, _offset: u64) -> Option<String> {
        None
    }

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
        Err(io::Error::other("not implemented"))
    }

    /// Remove [`TxOffsetIndexMut`] named with `offset`.
    fn remove_offset_index(&self, _offset: TxOffset) -> io::Result<()> {
        Err(io::Error::other("not implemented"))
    }

    /// Get [`TxOffsetIndex`] for the given `offset`.
    fn get_offset_index(&self, _offset: TxOffset) -> io::Result<TxOffsetIndex> {
        Err(io::Error::other("not implemented"))
    }
}

/// Capability trait for repos that can report storage usage.
pub trait RepoWithSizeOnDisk: Repo {
    fn size_on_disk(&self) -> io::Result<SizeOnDisk>;
}

/// Marker for repos that do not require an external lock file.
///
/// Durability implementations can use this to expose repo-backed opening
/// only for storage backends where skipping the filesystem `db.lock` cannot
/// violate single-writer safety.
pub trait RepoWithoutLockFile: Repo {}

impl<T> RepoWithoutLockFile for &T where T: RepoWithoutLockFile {}

impl<T> RepoWithSizeOnDisk for &T
where
    T: RepoWithSizeOnDisk,
{
    fn size_on_disk(&self) -> io::Result<SizeOnDisk> {
        T::size_on_disk(self)
    }
}

#[cfg(any(test, feature = "test"))]
impl RepoWithoutLockFile for Memory {}

impl<T: Repo> Repo for &T {
    type SegmentWriter = T::SegmentWriter;
    type SegmentReader = T::SegmentReader;

    fn create_segment(&self, offset: u64, header: segment::Header) -> io::Result<Self::SegmentWriter> {
        T::create_segment(self, offset, header)
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
    let mut storage = repo.create_segment(
        offset,
        Header {
            log_format_version: opts.log_format_version,
            checksum_algorithm: Commit::CHECKSUM_ALGORITHM,
        },
    )?;
    // Ensure we have enough space for this segment.
    fallocate(&mut storage, &opts)?;

    Ok(Writer {
        commit: Commit {
            min_tx_offset: offset,
            n: 0,
            records: Vec::new(),
            epoch,
        },
        inner: io::BufWriter::with_capacity(opts.write_buffer_size, storage),

        min_tx_offset: offset,
        bytes_written: Header::LEN as u64,

        offset_index_head: create_offset_index_writer(repo, offset, opts),
    })
}

/// Outcome of [resume_segment_writer].
pub enum ResumedSegment<W: io::Write> {
    /// The segment contains at most the header bytes.
    ///
    /// It is not safe to resume without first checking integrity of
    /// the preceeding segment. The empty segment should be removed.
    Empty,
    /// The successfully resumed segment writer.
    Resumed(Writer<W>),
    /// The segment is valid, but should not be resumed as it is already sealed.
    ///
    /// The [Metadata] is guaranteed to contain at least one valid commit.
    /// A new segment should be created at `Metadata::tx_range.end()`.
    Sealed(Metadata),
    /// The segment contains corrupted data and should not be resumed.
    ///
    /// The [Metadata] is guaranteed to contain at least one valid commit.
    /// A new segment should be created at `Metadata::tx_range.end()`.
    Corrupted(Metadata),
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
/// commit is returned. In this case, a new segment should be created for
/// writing. Similarly if the segment is sealed.
pub fn resume_segment_writer<R: Repo>(
    repo: &R,
    opts: Options,
    offset: u64,
) -> io::Result<ResumedSegment<R::SegmentWriter>> {
    let mut reader = repo
        .open_segment_reader(offset)
        .map_err(|source| with_segment_context("opening segment for resume", repo, offset, source))?;

    // If the segment at `offset` is empty, remove it and try the previous.
    // Return an error if no previous segment is found.
    let len = reader
        .segment_len()
        .map_err(|source| with_segment_context("determining segment file size for resume", repo, offset, source))?;
    if len <= segment::Header::LEN as u64 {
        debug!("repo {}: segment {} is empty", repo, offset);
        return Ok(ResumedSegment::Empty);
    }

    let guard_non_empty = |meta: &Metadata| match meta.tx_range.is_empty() {
        true => Err(with_segment_context(
            "checking metadata",
            repo,
            offset,
            io::Error::new(io::ErrorKind::InvalidData, "no valid commits in segment"),
        )),
        false => Ok(()),
    };

    // The segment is now guaranteed to be non-empty, i.e. contain more bytes
    // than the segment header.
    //
    // Traverse it to gather the `Metadata` and ensure that the segment is safe
    // to resume, which is the case if:
    //
    // - it contains at least one commit
    // - it does not contain corrupted commits
    // - the existing segment passes the compatibility check
    // - the existing segment's version is the same as
    //   the one requested in `opts`
    let offset_index = repo.get_offset_index(offset).ok();
    let meta = match Metadata::extract(offset, &mut reader, offset_index.as_ref()) {
        Err(error::SegmentMetadata::InvalidCommit { sofar, source }) => {
            warn!("{repo}: invalid commit in segment {offset}: {source}");
            debug!("sofar={sofar:?}");
            guard_non_empty(&sofar)?;
            return Ok(ResumedSegment::Corrupted(sofar));
        }
        Err(error::SegmentMetadata::Io(e)) => {
            return Err(with_segment_context("extracting segment metadata", repo, offset, e));
        }
        Ok(meta) => meta,
    };
    meta.header
        .ensure_compatible(opts.log_format_version, Commit::CHECKSUM_ALGORITHM)
        .map_err(|msg| {
            with_segment_context(
                "checking segment compatibility",
                repo,
                offset,
                io::Error::new(io::ErrorKind::InvalidData, msg),
            )
        })?;
    if meta.header.log_format_version != opts.log_format_version {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{}: log format version mismatch: current={} segment={}",
                segment_label(repo, offset),
                opts.log_format_version,
                meta.header.log_format_version
            ),
        ));
    }
    guard_non_empty(&meta)?;

    if reader.sealed() {
        Ok(ResumedSegment::Sealed(meta))
    } else {
        let Metadata {
            header: _,
            tx_range,
            size_in_bytes,
            max_epoch,
            max_commit_offset: _,
            max_commit: _,
        } = meta;
        let mut writer = repo.open_segment_writer(offset)?;
        // Ensure we have enough space for this segment.
        // The segment could have been created without the `fallocate` feature
        // enabled, so we call this here again to ensure writes can't fail due
        // to ENOSPC.
        fallocate(&mut writer, &opts)?;
        // We use `O_APPEND`, but make the file offset consistent regardless.
        writer.seek(io::SeekFrom::End(0))?;

        Ok(ResumedSegment::Resumed(Writer {
            commit: Commit {
                min_tx_offset: tx_range.end,
                n: 0,
                records: Vec::new(),
                epoch: max_epoch,
            },
            inner: io::BufWriter::new(writer),

            min_tx_offset: tx_range.start,
            bytes_written: size_in_bytes,

            offset_index_head: create_offset_index_writer(repo, offset, opts),
        }))
    }
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
    let segment = segment_label(repo, offset);
    debug!("open segment reader for {segment}");
    let storage = repo
        .open_segment_reader(offset)
        .map_err(|source| with_segment_context("opening segment for read", repo, offset, source))?;
    Reader::new(max_log_format_version, offset, storage)
        .map_err(|source| with_segment_context("reading segment header", repo, offset, source))
}

fn segment_label<R: Repo>(repo: &R, offset: u64) -> String {
    repo.segment_file_path(offset)
        .unwrap_or_else(|| format!("offset {offset}"))
}

fn with_segment_context<R: Repo>(context: &'static str, repo: &R, offset: u64, source: io::Error) -> io::Error {
    io::Error::new(
        source.kind(),
        format!("{} [{}]: {}", segment_label(repo, offset), context, source),
    )
}

/// Allocate [Options::max_segment_size] of space for [FileLike]
/// if the `fallocate` feature is enabled,
/// and [Options::preallocate_segments] is `true`.
///
/// No-op otherwise.
#[inline]
pub(crate) fn fallocate(_f: &mut impl FileLike, _opts: &Options) -> io::Result<()> {
    #[cfg(feature = "fallocate")]
    if _opts.preallocate_segments {
        _f.fallocate(_opts.max_segment_size)?;
    }

    Ok(())
}
