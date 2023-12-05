use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[cfg(target_family = "unix")]
use std::os::unix::fs::FileExt;
#[cfg(target_family = "windows")]
use std::os::windows::fs::FileExt;

use derive_more::Display;
use thiserror::Error;

use super::messages::commit::Commit;

const HEADER_SIZE: usize = 4;

/// Error returned by [`OpenOptions::open`] or [`MessageLog::open`].
#[derive(Debug, Error)]
#[error("Failed to open message log at `{path}`: {kind}")]
pub struct OpenError {
    kind: OpenErrorKind,
    path: String,
    #[source]
    source: Option<io::Error>,
}

#[derive(Debug, Display)]
pub enum OpenErrorKind {
    #[display(fmt = "Could not create root directory")]
    CreateRoot,
    #[display(fmt = "Could not read directory or -entry")]
    ReadDir,
    #[display(fmt = "Could not parse file name as offset")]
    ParseOffset,
    #[display(fmt = "Could not open segment file")]
    OpenSegment,
    #[display(fmt = "Failed to read segment at byte offset {offset}")]
    ReadSegment { offset: u64 },
}

/// Options for opening a [`MessageLog`], similar to [`fs::OpenOptions`].
#[derive(Clone, Copy, Debug)]
pub struct OpenOptions {
    max_segment_size: u64,
    // TODO(kim): Offset index options
}

impl OpenOptions {
    /// Set the maximum size in bytes of a single log segment.
    ///
    /// Default: 1GiB
    pub fn max_segment_size(&mut self, size: u64) -> &mut Self {
        self.max_segment_size = size;
        self
    }

    /// Open the [`MessageLog`] at `path` with the options in self.
    #[tracing::instrument(skip_all)]
    pub fn open(&self, path: impl AsRef<Path>) -> Result<MessageLog, OpenError> {
        use OpenErrorKind::*;

        fn err<S>(kind: OpenErrorKind, path: &Path) -> impl FnOnce(S) -> OpenError
        where
            S: Into<Option<io::Error>>,
        {
            let path = path.display().to_string();
            move |source| OpenError {
                kind,
                path,
                source: source.into(),
            }
        }

        let root = path.as_ref();
        fs::create_dir_all(root).map_err(err(CreateRoot, root))?;

        let mut segments = Vec::new();
        let mut total_size = 0;
        for file in fs::read_dir(root).map_err(err(ReadDir, root))? {
            let dir_entry = file.map_err(err(ReadDir, root))?;
            let path = dir_entry.path();
            if let Some(ext) = path.extension() {
                if ext != "log" {
                    continue;
                }
                let offset = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .parse::<u64>()
                    .map_err(|_| err(ParseOffset, &path)(None))?;
                let size = dir_entry.metadata().map_err(err(ReadDir, &path))?.len();

                total_size += size;
                segments.push(Segment {
                    min_offset: offset,
                    size,
                });
            }
        }

        segments.sort_unstable_by_key(|s| s.min_offset);

        if segments.is_empty() {
            segments.push(Segment { min_offset: 0, size: 0 });
        }

        let last_segment = segments.last().unwrap();
        let last_segment_path = root.join(last_segment.name() + ".log");
        let file = fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&last_segment_path)
            .map_err(err(OpenSegment, &last_segment_path))?;

        let mut max_offset = last_segment.min_offset;
        let mut cursor: u64 = 0;
        while cursor < last_segment.size {
            let mut buf = [0; HEADER_SIZE];
            #[cfg(target_family = "windows")]
            file.seek_read(&mut buf, cursor)
                .map_err(err(ReadSegment { offset: cursor }, &last_segment_path))?;
            #[cfg(target_family = "unix")]
            file.read_exact_at(&mut buf, cursor)
                .map_err(err(ReadSegment { offset: cursor }, &last_segment_path))?;
            let message_len = u32::from_le_bytes(buf);

            max_offset += 1;
            cursor += HEADER_SIZE as u64 + message_len as u64;
        }

        let file = BufWriter::new(file);

        log::debug!("Initialized with offset {}", max_offset);

        Ok(MessageLog {
            root: root.to_owned(),
            options: *self,
            segments,
            total_size,
            open_segment_file: file,
            open_segment_max_offset: max_offset,
        })
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            max_segment_size: 1_073_741_824, // 1GiB
        }
    }
}

/// A segment of a [`MessageLog`].
///
/// Represents a physical file holding a subset of the log.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Segment {
    /// Message offset this segment starts with (zero-based).
    pub min_offset: u64,
    /// Size in bytes of this segment.
    pub size: u64,
}

impl Segment {
    /// Name of this segment, as it appears in its file name.
    pub fn name(&self) -> String {
        format!("{:0>20}", self.min_offset)
    }
}

pub struct MessageLog {
    root: PathBuf,
    options: OpenOptions,
    segments: Vec<Segment>,
    total_size: u64,
    open_segment_file: BufWriter<File>,
    pub open_segment_max_offset: u64,
}

impl std::fmt::Debug for MessageLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageLog")
            .field("root", &self.root)
            .field("segments", &self.segments)
            .field("total_size", &self.total_size)
            .field("open_segment_file", &self.open_segment_file)
            .field("open_segment_max_offset", &self.open_segment_max_offset)
            .field("open_segment_size", &self.open_segment().size)
            .finish()
    }
}

impl Drop for MessageLog {
    fn drop(&mut self) {
        if let Err(e) = self.sync_all() {
            log::warn!("Unable to fsync on drop: {e}");
        }
    }
}

// TODO: do we build the concept of batches into the message log?
impl MessageLog {
    #[tracing::instrument(skip(path))]
    pub fn open(path: impl AsRef<Path>) -> Result<Self, OpenError> {
        OpenOptions::default().open(path)
    }

    pub fn options() -> OpenOptions {
        OpenOptions::default()
    }

    #[tracing::instrument(skip_all)]
    pub fn append(&mut self, message: impl AsRef<[u8]>) -> io::Result<()> {
        let message = message.as_ref();
        let mess_size = message.len() as u32;
        let size: u32 = mess_size + HEADER_SIZE as u32;

        let end_size = self.open_segment().size + size as u64;
        if end_size > self.options.max_segment_size {
            self.flush()?;
            self.segments.push(Segment {
                min_offset: self.open_segment_max_offset,
                size: 0,
            });

            let last_segment = self.segments.last().unwrap();
            let last_segment_path = self.root.join(last_segment.name() + ".log");

            let file = fs::OpenOptions::new()
                .append(true)
                .create_new(true)
                .open(last_segment_path)?;
            let file = BufWriter::new(file);

            self.open_segment_file = file;
        }

        self.open_segment_file.write_all(&mess_size.to_le_bytes())?;
        self.open_segment_file.write_all(message)?;

        self.open_segment_mut().size += size as u64;
        self.open_segment_max_offset += 1;
        self.total_size += size as u64;

        Ok(())
    }

    // NOTE: Flushing a `File` does nothing (just returns Ok(())), but flushing a BufWriter will
    // write the current buffer to the `File` by calling write. All `File` writes are atomic
    // so if you want to do an atomic action, make sure it all fits within the BufWriter buffer.
    // https://www.evanjones.ca/durability-filesystem.html
    // https://stackoverflow.com/questions/42442387/is-write-safe-to-be-called-from-multiple-threads-simultaneously/42442926#42442926
    // https://github.com/facebook/rocksdb/wiki/WAL-Performance
    #[tracing::instrument]
    pub fn flush(&mut self) -> io::Result<()> {
        self.open_segment_file.flush()?;
        Ok(())
    }

    // This will not return until the data is physically written to disk, as opposed to having
    // been pushed to the OS. You probably don't need to call this function, unless you need it
    // to be for sure durably written.
    // SEE: https://stackoverflow.com/questions/69819990/whats-the-difference-between-flush-and-sync-all
    #[tracing::instrument]
    pub fn sync_all(&mut self) -> io::Result<()> {
        log::trace!("fsync log file");
        self.flush()?;
        let file = self.open_segment_file.get_ref();
        file.sync_all()?;
        Ok(())
    }

    pub fn size(&self) -> u64 {
        self.total_size
    }

    pub fn max_segment_size(&self) -> u64 {
        self.options.max_segment_size
    }

    /// Total number of segments currently comprising the log.
    ///
    /// Equivalent to `self.segments().count()`, but more efficient.
    pub fn total_segments(&self) -> usize {
        self.segments.len()
    }

    pub fn get_root(&self) -> PathBuf {
        self.root.clone()
    }

    /// Obtains an iterator over all segments in the log, in the order they were
    /// created.
    ///
    /// The iterator represents a _snapshot_ of the log at the time this method
    /// is called. That is, segments created after the method returns will not
    /// appear in the iteration. The last segment yielded by the iterator may be
    /// incomplete (i.e. still be appended to).
    ///
    /// See also: [`MessageLog::segments_from`]
    pub fn segments(&self) -> Segments {
        self.segments_from(0)
    }

    /// Obtains an iterator over all segments containing messages equal to or
    /// newer than `offset`.
    ///
    /// `offset` counts all _messages_ (not: bytes) in the log, starting from
    /// zero.
    ///
    /// Note that the first segment yielded by the iterator may contain messages
    /// with an offset _smaller_ than the argument, as segments do not currently
    /// support slicing.
    ///
    /// If `offset` is larger than the offset of any message already written to
    /// the log, an empty iterator is returned.
    ///
    /// The iterator represents a _snapshot_ of the log at the time this method
    /// is called. That is, segments created after the method returns will not
    /// appear in the iteration. The last segment yielded by the iterator may be
    /// incomplete (i.e. still be appended to).
    pub fn segments_from(&self, offset: u64) -> Segments {
        if offset > self.open_segment_max_offset {
            return Segments::empty();
        }

        let root = self.get_root();
        let pos = self
            .segments
            .iter()
            .rposition(|s| s.min_offset <= offset)
            .expect("a segment with offset 0 must exist");

        Segments {
            root,
            inner: Vec::from(&self.segments[pos..]).into_iter(),
        }
    }

    /// Truncate the log to message offset `offset`.
    ///
    /// **This method destructively modifies the on-disk log!**
    ///
    /// After `reset_to` returns successfully, the message `offset` will be the
    /// last message in the log. That is:
    ///
    ///   * `reset_to(0)` will leave exactly zero messages in the log
    ///   * `reset_to(1)` will leave exactly  one message  in the log
    ///   * `reset_to(n)` will leave `min(n, open_segment_max_offset)`
    ///     messages in the log
    ///
    /// Segments with an offset range greater than `offset` will be removed.
    /// Note that this may interfere with readers which operate on a snapshot
    /// of the internal state of [`MessageLog`] (i.e. the [`Segments`] iterator).
    ///
    /// Setting the new offset (i.e. `self.open_segment_max_offset`) is
    /// **not atomic**, because [`MessageLog`] operates on multiple segment
    /// files internally.
    ///
    /// For example, the given `offset` may require some number of segment files
    /// at the end of the log to be deleted. Deleting a file could fail, in
    /// which case this method returns an error. The new offset in this case
    /// will be the max offset of the segment which could not be deleted, but
    /// potentially be greater than `offset`.
    ///
    /// However, file operations (`unlink`, `ftruncate`) are guaranteed to be
    /// atomic, to the extent required by [POSIX].
    ///
    /// [POSIX]: https://pubs.opengroup.org/onlinepubs/9699919799/functions/V2_chap02.html#tag_15_09_07
    pub fn reset_to(&mut self, offset: u64) -> io::Result<()> {
        log::info!("Resetting message log to offset {offset}");
        if offset == 0 {
            fs::remove_dir_all(&self.root)?;
            *self = self
                .options
                .open(&self.root)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

            return Ok(());
        }
        if offset >= self.open_segment_max_offset {
            return Ok(());
        }

        while let Some(segment) = self.segments.pop() {
            let path = self.root.join(segment.name()).with_extension("log");
            if segment.min_offset > offset {
                // Segment is outside the offset, so remove it wholesale.
                fs::remove_file(path)?;
                self.total_size -= segment.size;
                self.open_segment_max_offset = segment.min_offset - 1;
            } else {
                // Read record-wise until we find the byte offset.
                // TODO(kim): Use an offset index to seek closer to `offset`.
                let new_segment_size = {
                    let file = File::open(&path)?;
                    let mut iter = IterSegment {
                        segment: segment.min_offset,
                        read: 0,
                        file: BufReader::new(file),
                    };

                    log::debug!("segment min_offset={}", segment.min_offset);
                    for i in segment.min_offset..offset {
                        let message = iter.by_ref().next().ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                format!("Open segment shorter than expected: {i}"),
                            )
                        })?;
                        // Give up on I/O errors while reading the next message.
                        let _ = message?;
                    }
                    iter.bytes_read()
                };

                log::debug!("new segment size {new_segment_size}");
                // Truncate file to byte offset.
                let mut file = File::options().read(true).write(true).open(path)?;
                file.set_len(new_segment_size)?;
                file.sync_all()?;
                file.seek(SeekFrom::End(0))?;

                self.total_size -= segment.size;
                self.total_size += new_segment_size;
                self.segments.push(Segment {
                    size: new_segment_size,
                    ..segment
                });
                self.open_segment_max_offset = offset;
                self.open_segment_file = BufWriter::new(file);

                return Ok(());
            }
        }

        // TODO(kim): Consider using `NonEmpty` for the segment list.
        unreachable!("The segment with min offset 0 did not exist")
    }

    fn open_segment(&self) -> &Segment {
        self.segments.last().expect("at least one segment must exist")
    }

    fn open_segment_mut(&mut self) -> &mut Segment {
        self.segments.last_mut().expect("at least one segment must exist")
    }
}

/// A read-only view of an on-disk [`Segment`] of the [`MessageLog`].
///
/// The underlying file is opened lazily when calling [`SegmentView::try_into_iter`]
/// or [`SegmentView::try_into_file`].
#[derive(Clone, Debug)]
pub struct SegmentView {
    info: Segment,
    path: PathBuf,
}

impl SegmentView {
    /// The offset of the first message in the segment, relative to all segments
    /// in the log.
    pub fn offset(&self) -> u64 {
        self.info.min_offset
    }

    /// The size in bytes of the segment.
    pub fn size(&self) -> u64 {
        self.info.size
    }

    /// Obtain an iterator over the _messages_ the segment contains.
    ///
    /// Opens a new handle to the underlying file.
    pub fn try_into_iter(self) -> io::Result<IterSegment> {
        self.try_into()
    }

    /// Obtain an iterator over the [`Commit`]s the segment contains.
    ///
    /// Convenience for `self.try_into_iter().map(|i| i.commits())`.
    pub fn try_into_iter_commits(self) -> io::Result<impl Iterator<Item = io::Result<Commit>>> {
        self.try_into_iter().map(|i| i.commits())
    }

    /// Turn this [`SegmentView`] into a [`Read`]able [`File`].
    pub fn try_into_file(self) -> io::Result<File> {
        self.try_into()
    }
}

impl From<SegmentView> for Segment {
    fn from(SegmentView { info, .. }: SegmentView) -> Self {
        info
    }
}

impl TryFrom<SegmentView> for IterSegment {
    type Error = io::Error;

    fn try_from(view: SegmentView) -> Result<Self, Self::Error> {
        let segment = view.offset();
        File::try_from(view).map(|file| IterSegment {
            segment,
            read: 0,
            file: BufReader::new(file),
        })
    }
}

impl TryFrom<SegmentView> for File {
    type Error = io::Error;

    fn try_from(view: SegmentView) -> Result<Self, Self::Error> {
        File::open(view.path)
    }
}

/// Iterator over a [`SegmentView`], yielding individual messages.
///
/// Created by [`SegmentView::try_iter`].
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct IterSegment {
    segment: u64,
    read: u64,
    file: BufReader<File>,
}

impl IterSegment {
    /// Return the id of the segment being iterated over.
    ///
    /// The segment id is the `min_offset`, but that information is not
    /// meaningful here -- the value returned should be treated  as opaque.
    pub fn segment(&self) -> u64 {
        self.segment
    }

    /// Return the number of bytes read from the segment file so far.
    pub fn bytes_read(&self) -> u64 {
        self.read
    }

    /// Turn this iterator into an iterator yielding [`Commit`]s
    /// instead of raw bytes.
    pub fn commits(self) -> impl Iterator<Item = io::Result<Commit>> {
        super::commit_log::IterSegment::from(self)
    }
}

impl Iterator for IterSegment {
    type Item = io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = [0; HEADER_SIZE];
        if let Err(e) = self.file.read_exact(&mut buf) {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                return None;
            } else {
                return Some(Err(e));
            }
        }
        self.read += HEADER_SIZE as u64;

        let message_len = u32::from_le_bytes(buf);
        let mut buf = vec![0; message_len as usize];
        if let Err(e) = self.file.read_exact(&mut buf) {
            return Some(Err(e));
        }
        self.read += message_len as u64;

        Some(Ok(buf))
    }
}

/// Iterator yielding [`SegmentView`]s, created by [`MessageLog::segments`] and
/// [`MessageLog::segments_from`] respectively.
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct Segments {
    root: PathBuf,
    inner: std::vec::IntoIter<Segment>,
}

impl Segments {
    pub fn empty() -> Self {
        Self {
            root: PathBuf::default(),
            inner: vec![].into_iter(),
        }
    }
}

impl Iterator for Segments {
    type Item = SegmentView;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|segment| SegmentView {
            info: segment,
            path: self.root.join(segment.name()).with_extension("log"),
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_macros)]

    use std::path::Path;

    use super::{MessageLog, Segment, HEADER_SIZE};
    use spacetimedb_lib::error::ResultTest;
    use tempfile::{self, TempDir};

    #[test]
    fn test_message_log() -> ResultTest<()> {
        let tmp_dir = TempDir::with_prefix("message_log_test")?;
        let path = tmp_dir.path();
        let mut message_log = MessageLog::open(path)?;

        const MESSAGE_COUNT: i32 = 100_000;
        let start = std::time::Instant::now();
        for _i in 0..MESSAGE_COUNT {
            let s = b"yo this is tyler";
            message_log.append(s)?;
        }
        let duration = start.elapsed();
        println!(
            "{} us ({} ns / message)",
            duration.as_micros(),
            duration.as_nanos() / MESSAGE_COUNT as u128
        );
        message_log.flush()?;
        println!("total_size: {}", message_log.size());
        Ok(())
    }

    #[test]
    fn test_message_log_reopen() -> ResultTest<()> {
        let tmp_dir = TempDir::with_prefix("message_log_test")?;
        let path = tmp_dir.path();
        let mut message_log = MessageLog::open(path)?;

        const MESSAGE_COUNT: i32 = 100_000;
        let start = std::time::Instant::now();
        for _i in 0..MESSAGE_COUNT {
            let s = b"yo this is tyler";
            //let message = s.as_bytes();
            message_log.append(s)?;
        }
        let duration = start.elapsed();
        println!(
            "{} us ({} ns / message)",
            duration.as_micros(),
            duration.as_nanos() / MESSAGE_COUNT as u128
        );
        message_log.sync_all()?;
        println!("total_size: {}", message_log.size());
        drop(message_log);

        let message_log = MessageLog::open(path)?;
        assert!(message_log.size() == 2_000_000);

        Ok(())
    }

    #[test]
    fn test_segments_iter() -> ResultTest<()> {
        let tmp = TempDir::with_prefix("message_log_test")?;

        const SEGMENTS: usize = 3;
        const MESSAGES_PER_SEGMENT: usize = 10_000;

        let message_log = fill_log(tmp.path(), SEGMENTS, MESSAGES_PER_SEGMENT, b"foo fi fo fum")?;

        let segments = message_log.segments().count();
        assert_eq!(3, segments);

        let segments = message_log.segments_from(1_000_000).count();
        assert_eq!(0, segments);

        let segments = message_log.segments_from(20_000).count();
        assert_eq!(1, segments);

        let segments = message_log.segments_from(10_000).count();
        assert_eq!(2, segments);

        let segments = message_log.segments_from(9_999).count();
        assert_eq!(3, segments);

        let segments = message_log.segments_from(0).count();
        assert_eq!(3, segments);

        Ok(())
    }

    #[test]
    fn test_segment_iter() -> ResultTest<()> {
        let tmp = TempDir::with_prefix("message_log_test")?;

        const MESSAGE: &[u8] = b"fee fi fo fum";
        const SEGMENTS: usize = 3;
        const MESSAGES_PER_SEGMENT: usize = 10_000;

        let mlog = fill_log(tmp.path(), SEGMENTS, MESSAGES_PER_SEGMENT, MESSAGE)?;
        let mut count = 0;
        for segment in mlog.segments() {
            for message in segment.try_into_iter()? {
                assert_eq!(message?, MESSAGE);
                count += 1;
            }
        }
        assert_eq!(count, MESSAGES_PER_SEGMENT * SEGMENTS);

        Ok(())
    }

    #[test]
    fn test_truncate() -> ResultTest<()> {
        let tmp = TempDir::with_prefix("message_log_test")?;

        const MESSAGE: &[u8] = b"bleep bloop bleep";
        const SEGMENTS: usize = 3;
        const MESSAGES_PER_SEGMENT: usize = 10_000;

        #[derive(Debug)]
        struct Snapshot {
            segments: Vec<Segment>,
            total_size: u64,
            max_offset: u64,
        }

        impl From<&MessageLog> for Snapshot {
            fn from(mlog: &MessageLog) -> Self {
                Self {
                    segments: mlog.segments.clone(),
                    total_size: mlog.total_size,
                    max_offset: mlog.open_segment_max_offset,
                }
            }
        }

        fn go(mlog: &mut MessageLog, offset: u64) {
            let snapshot = Snapshot::from(&*mlog);

            mlog.reset_to(offset).unwrap();
            assert_eq!(
                offset, mlog.open_segment_max_offset,
                "offset must be reset to argument\n{:#?}",
                mlog
            );
            assert_eq!(
                mlog.total_size,
                mlog.segments.iter().map(|s| s.size).sum::<u64>(),
                "reset to {}: total size must be the sum of segment sizes\n{:#?}",
                offset,
                mlog
            );
            let entries_delta = snapshot.max_offset - offset;
            let size_delta = entries_delta * (MESSAGE.len() + HEADER_SIZE) as u64;
            assert_eq!(
                mlog.total_size,
                snapshot.total_size - size_delta,
                "reset to {}: size should have been reduced by {} entries / {} bytes\n{:#?}\n{:#?}",
                offset,
                entries_delta,
                size_delta,
                snapshot,
                mlog
            );

            if offset == 0 {
                assert_eq!(1, mlog.segments.len(), "one segment must exist");
                assert_eq!(0, mlog.open_segment().min_offset);
                assert_eq!(0, mlog.open_segment().size)
            }

            let retained_segments = snapshot.segments.iter().filter(|segment| segment.min_offset <= offset);
            let retained_segments_count = retained_segments.clone().count();
            assert_eq!(
                mlog.segments.len(),
                retained_segments_count,
                "reset to {}: {} segments should have been retained\n{:#?}\n{:#?}",
                offset,
                retained_segments_count,
                snapshot,
                mlog
            );

            let last_segment_size = retained_segments
                .take(retained_segments_count - 1)
                .fold(mlog.total_size, |acc, segment| acc - segment.size);
            assert_eq!(
                mlog.segments.last().unwrap().size,
                last_segment_size,
                "reset to {}: last segment should have size {}\n{:#?}\n{:#?}",
                offset,
                last_segment_size,
                snapshot,
                mlog,
            );
        }

        let mut mlog = fill_log(tmp.path(), SEGMENTS, MESSAGES_PER_SEGMENT, MESSAGE)?;
        for offset in [29_999, 22_000, 20_000, 15_000, 10_000, 0] {
            go(&mut mlog, offset)
        }

        // The log is now empty.
        // As a sanity check, assert that we're not off by one on the offset.
        mlog.append(b"retain me")?;
        mlog.append(MESSAGE)?;
        mlog.sync_all()?;
        mlog.reset_to(1)?;
        assert_eq!(
            b"retain me",
            mlog.segments()
                .next()
                .unwrap()
                .try_into_iter()
                .unwrap()
                .map(Result::unwrap)
                .last()
                .unwrap()
                .as_slice(),
            "last message in log should be 'retain me'\n{:#?}",
            mlog
        );

        Ok(())
    }

    fn fill_log(path: &Path, segments: usize, messages_per_segment: usize, message: &[u8]) -> ResultTest<MessageLog> {
        let segment_size = messages_per_segment * (message.len() + super::HEADER_SIZE);
        let total_messages = messages_per_segment * segments;

        let mut mlog = MessageLog::options().max_segment_size(segment_size as u64).open(path)?;
        for _ in 0..total_messages {
            mlog.append(message)?;
        }
        mlog.sync_all()?;

        Ok(mlog)
    }
}
