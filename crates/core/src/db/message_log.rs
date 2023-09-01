use std::{
    fs::{self, read_dir, File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

#[cfg(target_family = "unix")]
use std::os::unix::fs::FileExt;

use crate::error::DBError;
#[cfg(target_family = "windows")]
use std::os::windows::fs::FileExt;

const HEADER_SIZE: usize = 4;

/// Maximum size in bytes of a single log segment.
pub const MAX_SEGMENT_SIZE: u64 = 1_073_741_824;

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
    segments: Vec<Segment>,
    total_size: u64,
    open_segment_file: BufWriter<File>,
    open_segment_max_offset: u64,
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
        let root = path.as_ref();
        fs::create_dir_all(root).unwrap();

        let mut segments = Vec::new();
        let mut total_size = 0;
        for file in read_dir(root)? {
            let dir_entry = file?;
            let path = dir_entry.path();
            if path.extension().unwrap() == "log" {
                let file_stem = path.file_stem().unwrap();
                let offset = file_stem.to_os_string().into_string().unwrap();
                let offset = offset.parse::<u64>()?;
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
        let file = OpenOptions::new()
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

        Ok(Self {
            root: root.to_owned(),
            segments,
            total_size,
            open_segment_file: file,
            open_segment_max_offset: max_offset,
        })
    }

    #[tracing::instrument]
    pub fn reset_hard(&mut self) -> Result<(), DBError> {
        fs::remove_dir_all(&self.root)?;
        *self = Self::open(&self.root)?;
        Ok(())
    }

    #[tracing::instrument(skip(message))]
    pub fn append(&mut self, message: impl AsRef<[u8]>) -> Result<(), DBError> {
        let message = message.as_ref();
        let mess_size = message.len() as u32;
        let size: u32 = mess_size + HEADER_SIZE as u32;

        let end_size = self.open_segment().size + size as u64;
        if end_size > MAX_SEGMENT_SIZE {
            self.flush()?;
            self.segments.push(Segment {
                min_offset: self.open_segment_max_offset + 1,
                size: 0,
            });

            let last_segment = self.segments.last().unwrap();
            let last_segment_path = self.root.join(last_segment.name() + ".log");

            let file = OpenOptions::new()
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

    pub fn get_root(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn iter(&self) -> MessageLogIter {
        self.iter_from(0)
    }

    pub fn iter_from(&self, start_offset: u64) -> MessageLogIter {
        MessageLogIter {
            offset: start_offset,
            message_log: self,
            open_segment_file: None,
        }
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
    pub fn segments(&self) -> SegmentsIter {
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
    /// The iterator represents a _snapshot_ of the log at the time this method
    /// is called. That is, segments created after the method returns will not
    /// appear in the iteration. The last segment yielded by the iterator may be
    /// incomplete (i.e. still be appended to).
    pub fn segments_from(&self, offset: u64) -> SegmentsIter {
        let root = self.get_root();
        let pos = self
            .segments
            .iter()
            .rposition(|s| s.min_offset <= offset)
            .expect("a segment with offset 0 must exist");

        SegmentsIter {
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

    fn segment_for_offset(&self, offset: u64) -> Option<Segment> {
        let prev = self.segments[0];
        for segment in &self.segments {
            if segment.min_offset > offset {
                return Some(prev);
            }
        }
        if offset <= self.open_segment_max_offset {
            return Some(*self.segments.last().unwrap());
        }
        None
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
    pub fn try_into_iter(self) -> io::Result<SegmentIter> {
        self.try_into()
    }

    /// Turn this [`SegmentView`] into a [`Read`]able [`File`].
    pub fn try_into_file(self) -> io::Result<File> {
        self.try_into()
    }
}

impl TryFrom<SegmentView> for SegmentIter {
    type Error = io::Error;

    fn try_from(view: SegmentView) -> Result<Self, Self::Error> {
        File::try_from(view)
            .map(BufReader::new)
            .map(|file| SegmentIter { file })
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
pub struct SegmentIter {
    file: BufReader<File>,
}

impl SegmentIter {
    fn read_exact_or_none(&mut self, buf: &mut [u8]) -> Option<io::Result<()>> {
        match self.file.read_exact(buf) {
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => None,
            Err(e) => Some(Err(e)),
            Ok(()) => Some(Ok(())),
        }
    }
}

impl Iterator for SegmentIter {
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
pub struct SegmentsIter {
    root: PathBuf,
    inner: std::vec::IntoIter<Segment>,
}

impl SegmentsIter {
    pub fn empty() -> Self {
        Self {
            root: PathBuf::default(),
            inner: vec![].into_iter(),
        }
    }
}

impl Iterator for SegmentsIter {
    type Item = SegmentView;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|segment| SegmentView {
            info: segment,
            path: self.root.join(segment.name()).with_extension("log"),
        })
    }
}

pub struct MessageLogIter<'a> {
    offset: u64,
    message_log: &'a MessageLog,
    open_segment_file: Option<BufReader<File>>,
}

impl<'a> Iterator for MessageLogIter<'a> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        let open_segment_file: &mut BufReader<File>;
        if let Some(f) = &mut self.open_segment_file {
            open_segment_file = f;
        } else {
            let segment = self.message_log.segment_for_offset(self.offset).unwrap();
            let file = OpenOptions::new()
                .read(true)
                .open(self.message_log.root.join(segment.name() + ".log"))
                .unwrap();
            let file = BufReader::new(file);
            self.open_segment_file = Some(file);
            open_segment_file = self.open_segment_file.as_mut().unwrap();
        }

        // TODO: use offset to jump to the right spot in the file
        // open_segment_file.seek_relative(byte_offset(self.offset));

        let mut buf = [0; HEADER_SIZE];
        if let Err(err) = open_segment_file.read_exact(&mut buf) {
            match err.kind() {
                std::io::ErrorKind::UnexpectedEof => return None,
                _ => panic!("MessageLogIter: {:?}", err),
            }
        };
        let message_len = u32::from_le_bytes(buf);

        let mut buf = vec![0; message_len as usize];
        if let Err(err) = open_segment_file.read_exact(&mut buf) {
            match err.kind() {
                std::io::ErrorKind::UnexpectedEof => return None,
                _ => panic!("MessageLogIter: {:?}", err),
            }
        }

        self.offset += 1;

        Some(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::MessageLog;
    use spacetimedb_lib::error::ResultTest;
    use tempdir::{self, TempDir};

    #[test]
    fn test_message_log() -> ResultTest<()> {
        let tmp_dir = TempDir::new("message_log_test")?;
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
        let tmp_dir = TempDir::new("message_log_test")?;
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
}
