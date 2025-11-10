use std::fmt;
use std::fs::{self, File};
use std::io;
use std::sync::Arc;

use log::{debug, warn};
use spacetimedb_fs_utils::compression::{compress_with_zstd, CompressReader};
use spacetimedb_paths::server::{CommitLogDir, SegmentFile};
use tempfile::NamedTempFile;

use crate::segment::FileLike;

use super::{Repo, SegmentLen, TxOffset, TxOffsetIndex, TxOffsetIndexMut};

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
    fn add(&mut self, stat: std::fs::Metadata) {
        let imaginary_blocks = (self.total_bytes > 0)
            .then(|| 8 * self.total_bytes.div_ceil(4096))
            .unwrap_or_default();
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

impl Repo for Fs {
    type SegmentWriter = File;
    type SegmentReader = CompressReader;

    fn create_segment(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        File::options()
            .read(true)
            .append(true)
            .create_new(true)
            .open(self.segment_path(offset))
            .or_else(|e| {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    debug!("segment {offset} already exists");
                    // If the segment is completely empty, we can resume writing.
                    let file = self.open_segment_writer(offset)?;
                    if file.metadata()?.len() == 0 {
                        debug!("segment {offset} is empty");
                        return Ok(file);
                    }

                    // Otherwise, provide some context.
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("repo {}: segment {} already exists and is non-empty", self, offset),
                    ));
                }

                Err(e)
            })
            .inspect(|_| {
                // We're rotating commitlog segments, so we should also take a snapshot at the earliest opportunity.
                if let Some(on_new_segment) = self.on_new_segment.as_ref() {
                    // No need to handle the error here: if the snapshot worker is closed we'll eventually close too,
                    // and we don't want to die prematurely if there are still TXes to write.
                    on_new_segment();
                }
            })
    }

    fn open_segment_writer(&self, offset: u64) -> io::Result<Self::SegmentWriter> {
        File::options().read(true).append(true).open(self.segment_path(offset))
    }

    fn open_segment_reader(&self, offset: u64) -> io::Result<Self::SegmentReader> {
        let file = File::open(self.segment_path(offset))?;
        CompressReader::new(file)
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

    fn compress_segment(&self, offset: u64) -> io::Result<()> {
        let src = self.open_segment_reader(offset)?;
        // if it's already compressed, leave it be
        let CompressReader::None(mut src) = src else {
            return Ok(());
        };

        let mut dst = NamedTempFile::new_in(&self.root)?;
        // bytes per frame. in the future, it might be worth looking into putting
        // every commit into its own frame, to make seeking more efficient.
        let max_frame_size = 0x1000;
        compress_with_zstd(&mut src, &mut dst, Some(max_frame_size))?;
        dst.persist(self.segment_path(offset))?;

        Ok(())
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

impl SegmentLen for CompressReader {}

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
