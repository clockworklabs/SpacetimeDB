use std::fmt;
use std::fs::{self, File};
use std::io;
use std::sync::Arc;

use log::{debug, warn};
use spacetimedb_fs_utils::compression::{CompressReader, CompressionStats};
use spacetimedb_fs_utils::lockfile;
use spacetimedb_paths::server::{CommitLogDir, SegmentFile};
use tempfile::NamedTempFile;

use super::{Repo, SegmentLen, SegmentReader, TxOffset, TxOffsetIndex, TxOffsetIndexMut};
use crate::repo::CompressOnce;
use crate::segment::{self, FileLike};

const SEGMENT_FILE_EXT: &str = ".stdb.log";

// TODO
//
// - should use advisory locks?
//
// Experiment:
//
// - O_DIRECT | O_DSYNC
// - io_uring
//

pub type OnNewSegmentFn = dyn Fn() + Send + Sync + 'static;

/// Size on disk of a [Fs] repo.
///
/// Created by [Fs::size_on_disk].
#[derive(Clone, Copy, Default)]
pub struct SizeOnDisk {
    /// The total size in bytes of all segments and offset indexes in the repo.
    pub total_bytes: u64,
    /// The total number of 512-bytes blocks allocated by all segments and
    /// offset indexes in the repo.
    ///
    /// Only available on unix platforms.
    ///
    /// For other platforms, the number computed from the number of 4096-bytes
    /// pages that would be needed to store `total_bytes`. This may or may not
    /// reflect that actual storage allocation.
    ///
    /// The number of allocated blocks is typically larger than the number of
    /// actually written bytes.
    ///
    /// When the `fallocate` feature is enabled, the number can diverge
    /// substantially. Use `total_blocks` in this case to monitor disk space.
    pub total_blocks: u64,
}

impl SizeOnDisk {
    #[cfg(unix)]
    fn add(&mut self, stat: std::fs::Metadata) {
        self.total_bytes += stat.len();
        self.total_blocks += std::os::unix::fs::MetadataExt::blocks(&stat);
    }

    #[cfg(not(unix))]
    fn add(&mut self, _stat: std::fs::Metadata) {
        let imaginary_blocks = if self.total_bytes > 0 {
            8 * self.total_bytes.div_ceil(4096)
        } else {
            0
        };
        self.total_blocks = imaginary_blocks;
    }
}

/// A commitlog repository [`Repo`] which stores commits in ordinary files on
/// disk.
#[derive(Clone)]
pub struct Fs {
    /// The base directory within which segment files will be stored.
    root: CommitLogDir,

    /// Channel through which to send a message whenever we create a new segment.
    ///
    /// The other end of this channel will be a `SnapshotWorker`,
    /// which will capture a snapshot each time we rotate segments.
    on_new_segment: Option<Arc<OnNewSegmentFn>>,
}

impl std::fmt::Debug for Fs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fs").field("root", &self.root).finish_non_exhaustive()
    }
}

impl Fs {
    /// Create a commitlog repository which stores segments in the directory `root`.
    ///
    /// `root` must name an extant, accessible, writeable directory.
    pub fn new(root: CommitLogDir, on_new_segment: Option<Arc<OnNewSegmentFn>>) -> io::Result<Self> {
        root.create()?;
        Ok(Self { root, on_new_segment })
    }

    /// Get the filename for a segment starting with `offset` within this
    /// repository.
    pub fn segment_path(&self, offset: u64) -> SegmentFile {
        self.root.segment(offset)
    }

    /// Determine the size on disk as the sum of the sizes of all segments, as
    /// well as offset indexes.
    ///
    /// Note that the actively written-to segment (if any) is included.
    pub fn size_on_disk(&self) -> io::Result<SizeOnDisk> {
        let mut size = SizeOnDisk::default();

        for offset in self.existing_offsets()? {
            let segment = self.segment_path(offset);
            let stat = segment.metadata()?;
            size.add(stat);

            // Add the size of the offset index file if present.
            let index = self.root.index(offset);
            let Some(stat) = index.metadata().map(Some).or_else(|e| match e.kind() {
                io::ErrorKind::NotFound => Ok(None),
                _ => Err(e),
            })?
            else {
                continue;
            };
            size.add(stat);
        }

        Ok(size)
    }
}

impl fmt::Display for Fs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.root.display())
    }
}

impl SegmentLen for File {}

impl FileLike for NamedTempFile {
    fn fsync(&mut self) -> io::Result<()> {
        self.as_file_mut().fsync()
    }

    fn ftruncate(&mut self, tx_offset: u64, size: u64) -> io::Result<()> {
        self.as_file_mut().ftruncate(tx_offset, size)
    }

    #[cfg(feature = "fallocate")]
    fn fallocate(&mut self, size: u64) -> io::Result<()> {
        self.as_file_mut().fallocate(size)
    }
}

/// A file-backed, read-only segment.
///
/// Transparently handles reading compressed segments.
/// [Self::sealed] returns `true` if the segment is compressed.
pub struct ReadOnlySegment {
    inner: CompressReader,
    len: u64,
}

impl SegmentReader for ReadOnlySegment {
    #[inline]
    fn sealed(&self) -> bool {
        self.inner.is_compressed()
    }
}

impl io::Read for ReadOnlySegment {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl io::BufRead for ReadOnlySegment {
    #[inline]
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner.fill_buf()
    }

    #[inline]
    fn consume(&mut self, amount: usize) {
        self.inner.consume(amount);
    }
}

impl io::Seek for ReadOnlySegment {
    #[inline]
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl SegmentLen for ReadOnlySegment {
    fn segment_len(&mut self) -> io::Result<u64> {
        // If the segment is compressed, we guarantee that it is immutable,
        // so use the file length as determined when opening the reader.
        // Seeking would be somewhat expensive in this case, as the zstd reader
        // translates to uncompressed offsets and thus must decompress at least
        // some frames.
        //
        // If the segment is not compressed, we may be reading the active
        // segment, so immutability is not guaranteed. Use the default seek
        // strategy thus.
        if self.inner.is_compressed() {
            Ok(self.len)
        } else {
            use io::Seek as _;

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
}

impl Repo for Fs {
    type SegmentWriter = File;
    type SegmentReader = ReadOnlySegment;

    fn create_segment(&self, offset: u64, header: segment::Header) -> io::Result<Self::SegmentWriter> {
        let path = self.segment_path(offset);

        // We need to check if the segment already exists,
        // so use file locking to prevent a TOCTOU race.
        // Using `flock` means we don't need to worry about stale lockfiles.
        let lock_path = path.0.with_extension("lock");
        let _lock = scopeguard::guard(
            lockfile::advisory::LockedFile::lock(&lock_path)
                .map_err(|e| io::Error::new(e.source.kind(), format!("repo {}: {}: {}", self, e, e.source)))?,
            |lockfile| {
                if let Err(e) = lockfile.release(true) {
                    // It's ok if removing the file fails, but print a warning
                    // anyways.
                    warn!("repo {}: failed to remove {}: {}", self, lock_path.display(), e);
                }
            },
        );

        // Check whether the segment already exists.
        // Overwrite it if its length is zero.
        match fs::metadata(&path) {
            Ok(stat) => {
                if stat.len() > 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("repo {}: segment {} already exists and is non-empty", self, offset),
                    ));
                }
            }
            Err(e) => {
                if e.kind() != io::ErrorKind::NotFound {
                    return Err(io::Error::new(
                        e.kind(),
                        format!(
                            "repo {}: error getting file metadata for segment {}: {}",
                            self, offset, e
                        ),
                    ));
                }
            }
        }

        // The segment file either does not exist, or is of length zero.
        // Write the header to a temporary file and atomically move it into place.
        let mut tmp = tempfile::Builder::new().make_in(&self.root.0, |tmp_path| {
            File::options().read(true).append(true).create_new(true).open(tmp_path)
        })?;
        header.write(&mut tmp)?;
        tmp.as_file_mut().sync_all()?;
        let segment = tmp.persist(path)?;

        // Notify subscribers.
        if let Some(on_new_segment) = self.on_new_segment.as_ref() {
            on_new_segment();
        }

        Ok(segment)
    }

    fn open_segment_writer(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        File::options().read(true).append(true).open(self.segment_path(offset))
    }

    fn segment_file_path(&self, offset: u64) -> Option<String> {
        Some(self.segment_path(offset).0.to_string_lossy().into_owned())
    }

    fn open_segment_reader(&self, offset: u64) -> io::Result<Self::SegmentReader> {
        let path = self.segment_path(offset);
        debug!("fs: open segment at {}", path.display());
        let file = File::open(&path)?;
        let len = file.metadata()?.len();
        CompressReader::new(file).map(|inner| ReadOnlySegment { inner, len })
    }

    fn remove_segment(&self, offset: u64) -> io::Result<()> {
        let _ = self.remove_offset_index(offset).map_err(|e| {
            warn!(
                "repo {}: failed to remove offset index for segment {}: {}",
                self, offset, e
            );
        });
        fs::remove_file(self.segment_path(offset))
    }

    fn compress_segment_with(&self, offset: u64, f: impl CompressOnce) -> io::Result<CompressionStats> {
        let src = self.open_segment_reader(offset)?;
        // if it's already compressed, leave it be
        let CompressReader::None(mut src) = src.inner else {
            return Ok(<_>::default());
        };

        let mut dst = NamedTempFile::new_in(&self.root)?;
        let stats = f.compress(&mut src, &mut dst)?;
        dst.persist(self.segment_path(offset))?;

        Ok(stats)
    }

    fn existing_offsets(&self) -> io::Result<Vec<u64>> {
        let mut segments = Vec::new();

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let path = entry.path();
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let Some(file_name) = name.strip_suffix(SEGMENT_FILE_EXT) else {
                    continue;
                };
                let Ok(offset) = file_name.parse::<u64>() else {
                    continue;
                };

                segments.push(offset);
            }
        }

        segments.sort_unstable();

        Ok(segments)
    }

    fn create_offset_index(&self, offset: TxOffset, cap: u64) -> io::Result<TxOffsetIndexMut> {
        TxOffsetIndexMut::create_index_file(&self.root.index(offset), cap)
    }

    fn remove_offset_index(&self, offset: TxOffset) -> io::Result<()> {
        TxOffsetIndexMut::delete_index_file(&self.root.index(offset))
    }

    fn get_offset_index(&self, offset: TxOffset) -> io::Result<TxOffsetIndex> {
        TxOffsetIndex::open_index_file(&self.root.index(offset))
    }
}

#[cfg(feature = "streaming")]
impl crate::stream::AsyncRepo for Fs {
    type AsyncSegmentWriter = tokio::io::BufWriter<tokio::fs::File>;
    type AsyncSegmentReader = spacetimedb_fs_utils::compression::AsyncCompressReader<tokio::fs::File>;

    async fn open_segment_reader_async(&self, offset: u64) -> io::Result<Self::AsyncSegmentReader> {
        let file = tokio::fs::File::open(self.segment_path(offset)).await?;
        spacetimedb_fs_utils::compression::AsyncCompressReader::new(file).await
    }
}

#[cfg(feature = "streaming")]
impl<T> crate::stream::AsyncLen for spacetimedb_fs_utils::compression::AsyncCompressReader<T> where
    T: tokio::io::AsyncSeek + tokio::io::AsyncRead + Unpin + Send
{
}
