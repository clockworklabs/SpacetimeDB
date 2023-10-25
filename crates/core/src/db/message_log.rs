use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

#[cfg(target_family = "unix")]
use std::os::unix::fs::FileExt;

use anyhow::{anyhow, Context};

use crate::error::DBError;
#[cfg(target_family = "windows")]
use std::os::windows::fs::FileExt;

const HEADER_SIZE: usize = 4;

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
    pub fn open(&self, path: impl AsRef<Path>) -> Result<MessageLog, DBError> {
        let root = path.as_ref();
        fs::create_dir_all(root).with_context(|| format!("could not create root directory: {}", root.display()))?;

        let mut segments = Vec::new();
        let mut total_size = 0;
        for file in fs::read_dir(root).with_context(|| format!("unable to read root directory: {}", root.display()))? {
            let dir_entry = file?;
            let path = dir_entry.path();
            if let Some(ext) = path.extension() {
                if ext != "log" {
                    continue;
                }
                let file_stem = path
                    .file_stem()
                    .map(|os| os.to_string_lossy())
                    .ok_or_else(|| anyhow!("unexpected .log file: {}", path.display()))?;
                let offset = file_stem
                    .parse::<u64>()
                    .with_context(|| format!("could not parse log offset from: {}", path.display()))?;
                let size = dir_entry.metadata()?.len();

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
            .open(&last_segment_path)?;

        let mut max_offset = last_segment.min_offset;
        let mut cursor: u64 = 0;
        while cursor < last_segment.size {
            let mut buf = [0; HEADER_SIZE];
            #[cfg(target_family = "windows")]
            file.seek_read(&mut buf, cursor)?;
            #[cfg(target_family = "unix")]
            file.read_exact_at(&mut buf, cursor)?;
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

#[derive(Clone, Copy, Debug)]
struct Segment {
    min_offset: u64,
    size: u64,
}

impl Segment {
    fn name(&self) -> String {
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

// TODO: do we build the concept of batches into the message log?
impl MessageLog {
    #[tracing::instrument(skip(path))]
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DBError> {
        OpenOptions::default().open(path)
    }

    pub fn options() -> OpenOptions {
        OpenOptions::default()
    }

    #[tracing::instrument]
    pub fn reset_hard(&mut self) -> Result<(), DBError> {
        fs::remove_dir_all(&self.root)?;
        *self = Self::open(&self.root)?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub fn append(&mut self, message: impl AsRef<[u8]>) -> Result<(), DBError> {
        let message = message.as_ref();
        let mess_size = message.len() as u32;
        let size: u32 = mess_size + HEADER_SIZE as u32;

        let end_size = self.open_segment().size + size as u64;
        if end_size > self.options.max_segment_size {
            self.flush()?;
            self.segments.push(Segment {
                min_offset: self.open_segment_max_offset + 1,
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
    pub fn flush(&mut self) -> Result<(), DBError> {
        self.open_segment_file.flush()?;
        Ok(())
    }

    // This will not return until the data is physically written to disk, as opposed to having
    // been pushed to the OS. You probably don't need to call this function, unless you need it
    // to be for sure durably written.
    // SEE: https://stackoverflow.com/questions/69819990/whats-the-difference-between-flush-and-sync-all
    #[tracing::instrument]
    pub fn sync_all(&mut self) -> Result<(), DBError> {
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

    /// Turn this [`SegmentView`] into a [`Read`]able [`File`].
    pub fn try_into_file(self) -> io::Result<File> {
        self.try_into()
    }
}

impl TryFrom<SegmentView> for IterSegment {
    type Error = io::Error;

    fn try_from(view: SegmentView) -> Result<Self, Self::Error> {
        File::try_from(view)
            .map(BufReader::new)
            .map(|file| IterSegment { file })
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
    file: BufReader<File>,
}

impl IterSegment {
    fn read_exact_or_none(&mut self, buf: &mut [u8]) -> Option<io::Result<()>> {
        match self.file.read_exact(buf) {
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => None,
            Err(e) => Some(Err(e)),
            Ok(()) => Some(Ok(())),
        }
    }
}

impl Iterator for IterSegment {
    type Item = io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = [0; HEADER_SIZE];
        if let Err(e) = self.read_exact_or_none(&mut buf)? {
            return Some(Err(e));
        }

        let message_len = u32::from_le_bytes(buf);
        let mut buf = vec![0; message_len as usize];
        if let Err(e) = self.read_exact_or_none(&mut buf)? {
            return Some(Err(e));
        }

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

    use super::MessageLog;
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
        let path = tmp.path();

        const MESSAGE: &[u8] = b"fee fi fo fum";
        const MESSAGES_PER_SEGMENT: usize = 10_000;
        const SEGMENT_SIZE: usize = MESSAGES_PER_SEGMENT * (MESSAGE.len() + super::HEADER_SIZE);
        const TOTAL_MESSAGES: usize = (MESSAGES_PER_SEGMENT * 3) - 1;

        let mut message_log = MessageLog::options().max_segment_size(SEGMENT_SIZE as u64).open(path)?;
        for _ in 0..TOTAL_MESSAGES {
            message_log.append(MESSAGE)?;
        }
        message_log.sync_all()?;

        let segments = message_log.segments().count();
        assert_eq!(3, segments);

        let segments = message_log.segments_from(1_000_000).count();
        assert_eq!(0, segments);

        let segments = message_log.segments_from(20_001).count();
        assert_eq!(1, segments);

        let segments = message_log.segments_from(10_001).count();
        assert_eq!(2, segments);

        let segments = message_log.segments_from(10_000).count();
        assert_eq!(3, segments);

        Ok(())
    }

    #[test]
    fn test_segment_iter() -> ResultTest<()> {
        let tmp = TempDir::with_prefix("message_log_test")?;
        let path = tmp.path();

        const MESSAGE: &[u8] = b"fee fi fo fum";
        const MESSAGES_PER_SEGMENT: usize = 10_000;
        const SEGMENT_SIZE: usize = MESSAGES_PER_SEGMENT * (MESSAGE.len() + super::HEADER_SIZE);
        const TOTAL_MESSAGES: usize = (MESSAGES_PER_SEGMENT * 3) - 1;

        let mut message_log = MessageLog::options().max_segment_size(SEGMENT_SIZE as u64).open(path)?;
        for _ in 0..TOTAL_MESSAGES {
            message_log.append(MESSAGE)?;
        }
        message_log.sync_all()?;

        let mut count = 0;
        for segment in message_log.segments() {
            for message in segment.try_into_iter()? {
                assert_eq!(message?, MESSAGE);
                count += 1;
            }
        }
        assert_eq!(count, TOTAL_MESSAGES);

        Ok(())
    }
}
