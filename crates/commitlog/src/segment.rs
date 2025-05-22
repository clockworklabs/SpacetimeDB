use std::{
    fs::File,
    io::{self, BufWriter, ErrorKind, SeekFrom, Write as _},
    num::{NonZeroU16, NonZeroU64},
    ops::Range,
};

use log::{debug, warn};

use crate::{
    commit::{self, Commit, StoredCommit},
    error,
    index::{IndexError, IndexFileMut},
    payload::Encode,
    repo::{TxOffset, TxOffsetIndex, TxOffsetIndexMut},
    Options,
};

pub const MAGIC: [u8; 6] = [b'(', b'd', b's', b')', b'^', b'2'];

pub const DEFAULT_LOG_FORMAT_VERSION: u8 = 1;
pub const DEFAULT_CHECKSUM_ALGORITHM: u8 = CHECKSUM_ALGORITHM_CRC32C;

pub const CHECKSUM_ALGORITHM_CRC32C: u8 = 0;
pub const CHECKSUM_CRC32C_LEN: usize = 4;

/// Lookup table for checksum length, index is [`Header::checksum_algorithm`].
// Supported algorithms must be numbered consecutively!
pub const CHECKSUM_LEN: [usize; 1] = [CHECKSUM_CRC32C_LEN];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Header {
    pub log_format_version: u8,
    pub checksum_algorithm: u8,
}

impl Header {
    pub const LEN: usize = MAGIC.len() + /* log_format_version + checksum_algorithm + reserved + reserved */ 4;

    pub fn write<W: io::Write>(&self, mut out: W) -> io::Result<()> {
        out.write_all(&MAGIC)?;
        out.write_all(&[self.log_format_version, self.checksum_algorithm, 0, 0])?;

        Ok(())
    }

    pub fn decode<R: io::Read>(mut read: R) -> io::Result<Self> {
        let mut buf = [0; Self::LEN];
        read.read_exact(&mut buf)?;

        if !buf.starts_with(&MAGIC) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "segment header does not start with magic",
            ));
        }

        Ok(Self {
            log_format_version: buf[MAGIC.len()],
            checksum_algorithm: buf[MAGIC.len() + 1],
        })
    }

    pub fn ensure_compatible(&self, max_log_format_version: u8, checksum_algorithm: u8) -> Result<(), String> {
        if self.log_format_version > max_log_format_version {
            return Err(format!("unsupported log format version: {}", self.log_format_version));
        }
        if self.checksum_algorithm != checksum_algorithm {
            return Err(format!("unsupported checksum algorithm: {}", self.checksum_algorithm));
        }

        Ok(())
    }
}

impl Default for Header {
    fn default() -> Self {
        Self {
            log_format_version: DEFAULT_LOG_FORMAT_VERSION,
            checksum_algorithm: DEFAULT_CHECKSUM_ALGORITHM,
        }
    }
}

/// Metadata about a [`Commit`] which was successfully written via [`Writer::commit`].
#[derive(Debug, PartialEq)]
pub struct Committed {
    /// The range of transaction offsets included in the commit.
    pub tx_range: Range<u64>,
    /// The crc32 checksum of the commit's serialized form,
    /// as written to the commitlog.
    pub checksum: u32,
}

#[derive(Debug)]
pub struct Writer<W: io::Write> {
    pub(crate) commit: Commit,
    pub(crate) inner: BufWriter<W>,

    pub(crate) min_tx_offset: u64,
    pub(crate) bytes_written: u64,

    pub(crate) max_records_in_commit: NonZeroU16,

    pub(crate) offset_index_head: Option<OffsetIndexWriter>,
}

impl<W: io::Write> Writer<W> {
    /// Append the record (aka transaction) `T` to the segment.
    ///
    /// If the number of currently buffered records would exceed `max_records_in_commit`
    /// after the method returns, the argument is returned in an `Err` and not
    /// appended to this writer's buffer.
    ///
    /// Otherwise, the `record` is encoded and and stored in the buffer.
    ///
    /// An `Err` result indicates that [`Self::commit`] should be called in
    /// order to flush the buffered records to persistent storage.
    pub fn append<T: Encode>(&mut self, record: T) -> Result<(), T> {
        if self.commit.n == u16::MAX || self.commit.n + 1 > self.max_records_in_commit.get() {
            Err(record)
        } else {
            self.commit.n += 1;
            record.encode_record(&mut self.commit.records);
            Ok(())
        }
    }

    /// Write the current [`Commit`] to the underlying [`io::Write`].
    ///
    /// Will do nothing if the current commit is empty (i.e. `Commit::n` is zero).
    /// In this case, `None` is returned.
    ///
    /// Otherwise `Some` [`Committed`] is returned, providing some metadata about
    /// the commit.
    pub fn commit(&mut self) -> io::Result<Option<Committed>> {
        if self.commit.n == 0 {
            return Ok(None);
        }
        let checksum = self.commit.write(&mut self.inner)?;
        self.inner.flush()?;

        let commit_len = self.commit.encoded_len() as u64;
        self.offset_index_head.as_mut().map(|index| {
            debug!(
                "append_after commit min_tx_offset={} bytes_written={} commit_len={}",
                self.commit.min_tx_offset, self.bytes_written, commit_len
            );
            index
                .append_after_commit(self.commit.min_tx_offset, self.bytes_written, commit_len)
                .map_err(|e| {
                    debug!("failed to append to offset index: {:?}", e);
                })
        });

        let tx_range_start = self.commit.min_tx_offset;

        self.bytes_written += commit_len;
        self.commit.min_tx_offset += self.commit.n as u64;
        self.commit.n = 0;
        self.commit.records.clear();

        Ok(Some(Committed {
            tx_range: tx_range_start..self.commit.min_tx_offset,
            checksum,
        }))
    }

    /// Get the current epoch.
    pub fn epoch(&self) -> u64 {
        self.commit.epoch
    }

    /// Update the epoch.
    ///
    /// The caller must ensure that:
    ///
    /// - The new epoch is greater than the current epoch.
    /// - [`Self::commit`] has been called as appropriate.
    ///
    pub fn set_epoch(&mut self, epoch: u64) {
        self.commit.epoch = epoch;
    }

    /// The smallest transaction offset in this segment.
    pub fn min_tx_offset(&self) -> u64 {
        self.min_tx_offset
    }

    /// The next transaction offset to be written if [`Self::commit`] was called.
    pub fn next_tx_offset(&self) -> u64 {
        self.commit.min_tx_offset
    }

    /// `true` if the segment contains no commits.
    ///
    /// The segment will, however, contain a header. This thus violates the
    /// convention that `is_empty == (len == 0)`.
    pub fn is_empty(&self) -> bool {
        self.bytes_written <= Header::LEN as u64
    }

    /// Number of bytes written to this segment, including the header.
    pub fn len(&self) -> u64 {
        self.bytes_written
    }
}

pub trait FileLike {
    fn fsync(&mut self) -> io::Result<()>;
    fn ftruncate(&mut self, tx_offset: u64, size: u64) -> io::Result<()>;
}

impl FileLike for File {
    fn fsync(&mut self) -> io::Result<()> {
        self.sync_data()
    }

    fn ftruncate(&mut self, _tx_offset: u64, size: u64) -> io::Result<()> {
        self.set_len(size)
    }
}

impl<W: io::Write + FileLike> FileLike for BufWriter<W> {
    fn fsync(&mut self) -> io::Result<()> {
        self.get_mut().fsync()
    }

    fn ftruncate(&mut self, tx_offset: u64, size: u64) -> io::Result<()> {
        self.get_mut().ftruncate(tx_offset, size)
    }
}

impl<W: io::Write + FileLike> FileLike for Writer<W> {
    fn fsync(&mut self) -> io::Result<()> {
        self.inner.fsync()?;
        self.offset_index_head.as_mut().map(|index| index.fsync());
        Ok(())
    }

    fn ftruncate(&mut self, tx_offset: u64, size: u64) -> io::Result<()> {
        self.inner.ftruncate(tx_offset, size)?;
        self.offset_index_head
            .as_mut()
            .map(|index| index.ftruncate(tx_offset, size));
        Ok(())
    }
}

#[derive(Debug)]
pub struct OffsetIndexWriter {
    pub(crate) head: TxOffsetIndexMut,

    require_segment_fsync: bool,
    min_write_interval: NonZeroU64,

    pub(crate) candidate_min_tx_offset: TxOffset,
    pub(crate) candidate_byte_offset: u64,
    pub(crate) bytes_since_last_index: u64,
}

impl OffsetIndexWriter {
    pub fn new(head: TxOffsetIndexMut, opts: Options) -> Self {
        OffsetIndexWriter {
            head,
            require_segment_fsync: opts.offset_index_require_segment_fsync,
            min_write_interval: opts.offset_index_interval_bytes,
            candidate_min_tx_offset: TxOffset::default(),
            candidate_byte_offset: 0,
            bytes_since_last_index: 0,
        }
    }

    fn reset(&mut self) {
        self.candidate_byte_offset = 0;
        self.candidate_min_tx_offset = TxOffset::default();
        self.bytes_since_last_index = 0;
    }

    /// Either append to index or save offsets to append at future fsync
    pub fn append_after_commit(
        &mut self,
        min_tx_offset: TxOffset,
        byte_offset: u64,
        commit_len: u64,
    ) -> Result<(), IndexError> {
        self.bytes_since_last_index += commit_len;

        if self.candidate_min_tx_offset == 0 {
            self.candidate_byte_offset = byte_offset;
            self.candidate_min_tx_offset = min_tx_offset;
        }

        if !self.require_segment_fsync {
            self.append_internal()?;
        }

        Ok(())
    }

    fn append_internal(&mut self) -> Result<(), IndexError> {
        // If the candidate offset is zero, there has not been a commit since the last offset entry
        if self.candidate_min_tx_offset == 0 {
            return Ok(());
        }

        if self.bytes_since_last_index < self.min_write_interval.get() {
            return Ok(());
        }

        self.head
            .append(self.candidate_min_tx_offset, self.candidate_byte_offset)?;
        self.head.async_flush()?;
        self.reset();

        Ok(())
    }
}

impl FileLike for OffsetIndexWriter {
    /// Must be called via SegmentWriter::fsync
    fn fsync(&mut self) -> io::Result<()> {
        let _ = self.append_internal().map_err(|e| {
            warn!("failed to append to offset index: {e:?}");
        });
        let _ = self
            .head
            .async_flush()
            .map_err(|e| warn!("failed to flush offset index: {e:?}"));
        Ok(())
    }

    fn ftruncate(&mut self, tx_offset: u64, _size: u64) -> io::Result<()> {
        self.reset();
        self.head
            .truncate(tx_offset)
            .inspect_err(|e| {
                warn!("failed to truncate offset index at {tx_offset}: {e:?}");
            })
            .ok();
        Ok(())
    }
}

impl FileLike for IndexFileMut<TxOffset> {
    fn fsync(&mut self) -> io::Result<()> {
        self.async_flush()
    }

    fn ftruncate(&mut self, tx_offset: u64, _size: u64) -> io::Result<()> {
        self.truncate(tx_offset).map_err(|e| {
            io::Error::new(
                ErrorKind::Other,
                format!("failed to truncate offset index at {tx_offset}: {e:?}"),
            )
        })
    }
}

#[derive(Debug)]
pub struct Reader<R> {
    pub header: Header,
    pub min_tx_offset: u64,
    inner: R,
}

impl<R: io::Read + io::Seek> Reader<R> {
    pub fn new(max_log_format_version: u8, min_tx_offset: u64, mut inner: R) -> io::Result<Self> {
        let header = Header::decode(&mut inner)?;
        header
            .ensure_compatible(max_log_format_version, Commit::CHECKSUM_ALGORITHM)
            .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;

        Ok(Self {
            header,
            min_tx_offset,
            inner,
        })
    }
}

impl<R: io::BufRead + io::Seek> Reader<R> {
    pub fn commits(self) -> Commits<R> {
        Commits {
            header: self.header,
            reader: self.inner,
        }
    }

    pub fn seek_to_offset(&mut self, index_file: &TxOffsetIndex, start_tx_offset: u64) -> Result<(), IndexError> {
        seek_to_offset(&mut self.inner, index_file, start_tx_offset)
    }

    #[cfg(test)]
    pub fn transactions<'a, D>(self, de: &'a D) -> impl Iterator<Item = Result<Transaction<D::Record>, D::Error>> + 'a
    where
        D: crate::Decoder,
        D::Error: From<io::Error>,
        R: 'a,
    {
        use itertools::Itertools as _;

        self.commits()
            .with_log_format_version()
            .map(|x| x.map_err(Into::into))
            .map_ok(move |(version, commit)| {
                let start = commit.min_tx_offset;
                commit.into_transactions(version, start, de)
            })
            .flatten_ok()
            .map(|x| x.and_then(|y| y))
    }

    #[cfg(test)]
    pub(crate) fn metadata(self) -> Result<Metadata, error::SegmentMetadata> {
        Metadata::with_header(self.min_tx_offset, self.header, self.inner, None)
    }
}

/// Advances the `segment` reader to the position corresponding to the `start_tx_offset`
/// using the `index_file` for efficient seeking.
///
/// Input:
/// - `segment` - segment reader
/// - `min_tx_offset` - minimum transaction offset in the segment
/// - `start_tx_offset` - transaction offset to advance to
pub fn seek_to_offset<R: io::Read + io::Seek>(
    mut segment: &mut R,
    index_file: &TxOffsetIndex,
    start_tx_offset: u64,
) -> Result<(), IndexError> {
    let (index_key, byte_offset) = index_file.key_lookup(start_tx_offset)?;

    // If the index_key is 0, it means the index file is empty, return error without seeking
    if index_key == 0 {
        return Err(IndexError::KeyNotFound);
    }
    debug!("index lookup for key={start_tx_offset}: found key={index_key} at byte-offset={byte_offset}");
    // returned `index_key` should never be greater than `start_tx_offset`
    debug_assert!(index_key <= start_tx_offset);

    // Check if the offset index is pointing to the right commit.
    validate_commit_header(&mut segment, byte_offset).map(|hdr| {
        if hdr.min_tx_offset == index_key {
            // Advance the segment Seek if expected commit is found.
            segment
                .seek(SeekFrom::Start(byte_offset))
                .map(|_| ())
                .map_err(Into::into)
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidData, "mismatch key in index offset file").into())
        }
    })?
}

/// Try to extract the commit header from the asked position without advancing seek.
/// `IndexFileMut` fsync asynchoronously, which makes it important for reader to verify its entry
pub fn validate_commit_header<Reader: io::Read + io::Seek>(
    mut reader: &mut Reader,
    byte_offset: u64,
) -> io::Result<commit::Header> {
    let pos = reader.stream_position()?;
    reader.seek(SeekFrom::Start(byte_offset))?;

    let hdr = commit::Header::decode(&mut reader)
        .and_then(|hdr| hdr.ok_or_else(|| io::Error::new(ErrorKind::UnexpectedEof, "unexpected EOF")));

    // Restore the original position
    reader.seek(SeekFrom::Start(pos))?;

    hdr
}

/// Pair of transaction offset and payload.
///
/// Created by iterators which "flatten" commits into individual transaction
/// records.
#[derive(Debug, PartialEq)]
pub struct Transaction<T> {
    /// The offset of this transaction relative to the start of the log.
    pub offset: u64,
    /// The transaction payload.
    pub txdata: T,
}

pub struct Commits<R> {
    pub header: Header,
    reader: R,
}

impl<R: io::BufRead> Iterator for Commits<R> {
    type Item = io::Result<StoredCommit>;

    fn next(&mut self) -> Option<Self::Item> {
        StoredCommit::decode_internal(&mut self.reader, self.header.log_format_version).transpose()
    }
}

#[cfg(test)]
impl<R: io::BufRead> Commits<R> {
    pub fn with_log_format_version(self) -> impl Iterator<Item = io::Result<(u8, StoredCommit)>> {
        CommitsWithVersion { inner: self }
    }
}

#[cfg(test)]
struct CommitsWithVersion<R> {
    inner: Commits<R>,
}

#[cfg(test)]
impl<R: io::BufRead> Iterator for CommitsWithVersion<R> {
    type Item = io::Result<(u8, StoredCommit)>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.inner.next()?;
        match next {
            Ok(commit) => {
                let version = self.inner.header.log_format_version;
                Some(Ok((version, commit)))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metadata {
    /// The segment header.
    pub header: Header,
    /// The range of transactions contained in the segment.
    pub tx_range: Range<u64>,
    /// The size of the segment.
    pub size_in_bytes: u64,
    /// The largest epoch found in the segment.
    pub max_epoch: u64,
    /// The latest commit found in the segment.
    ///
    /// The value is the `min_tx_offset` of the commit, i.e.
    /// `max_commit_offset..tx_range.end` is the range of
    /// transactions contained in it.
    pub max_commit_offset: u64,
}

impl Metadata {
    /// Reads and validates metadata from a segment.
    /// It will look for last commit index offset and then traverse the segment
    ///
    /// Determines `max_tx_offset`, `size_in_bytes`, and `max_epoch` from the segment.
    pub(crate) fn extract<R: io::Read + io::Seek>(
        min_tx_offset: TxOffset,
        mut reader: R,
        offset_index: Option<&TxOffsetIndex>,
    ) -> Result<Self, error::SegmentMetadata> {
        let header = Header::decode(&mut reader)?;
        Self::with_header(min_tx_offset, header, reader, offset_index)
    }

    fn with_header<R: io::Read + io::Seek>(
        min_tx_offset: u64,
        header: Header,
        mut reader: R,
        offset_index: Option<&TxOffsetIndex>,
    ) -> Result<Self, error::SegmentMetadata> {
        let mut sofar = offset_index
            .and_then(|index| Self::find_valid_indexed_commit(min_tx_offset, header, &mut reader, index).ok())
            .unwrap_or_else(|| Self {
                header,
                tx_range: Range {
                    start: min_tx_offset,
                    end: min_tx_offset,
                },
                size_in_bytes: Header::LEN as u64,
                max_epoch: u64::default(),
                max_commit_offset: min_tx_offset,
            });

        reader.seek(SeekFrom::Start(sofar.size_in_bytes))?;

        fn commit_meta<R: io::Read>(
            reader: &mut R,
            sofar: &Metadata,
        ) -> Result<Option<commit::Metadata>, error::SegmentMetadata> {
            commit::Metadata::extract(reader).map_err(|e| {
                if matches!(e.kind(), io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof) {
                    error::SegmentMetadata::InvalidCommit {
                        sofar: sofar.clone(),
                        source: e,
                    }
                } else {
                    e.into()
                }
            })
        }
        while let Some(commit) = commit_meta(&mut reader, &sofar)? {
            debug!("commit::{commit:?}");
            if commit.tx_range.start != sofar.tx_range.end {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "out-of-order offset: expected={} actual={}",
                        sofar.tx_range.end, commit.tx_range.start,
                    ),
                )
                .into());
            }
            sofar.tx_range.end = commit.tx_range.end;
            sofar.size_in_bytes += commit.size_in_bytes;
            // TODO: Should it be an error to encounter an epoch going backwards?
            sofar.max_epoch = commit.epoch.max(sofar.max_epoch);
            sofar.max_commit_offset = commit.tx_range.start;
        }

        Ok(sofar)
    }

    /// Finds the last valid commit in the segment using the offset index.
    /// It traverses the index in reverse order, starting from the last key.
    ///
    /// Returns
    /// * `Ok((Metadata)` - If a valid commit is found containing the commit, It adds a default
    ///     header, which should be replaced with the actual header.
    /// * `Err` - If no valid commit is found or if the index is empty
    fn find_valid_indexed_commit<R: io::Read + io::Seek>(
        min_tx_offset: u64,
        header: Header,
        reader: &mut R,
        offset_index: &TxOffsetIndex,
    ) -> io::Result<Metadata> {
        let mut candidate_last_key = TxOffset::MAX;

        while let Ok((key, byte_offset)) = offset_index.key_lookup(candidate_last_key) {
            match Self::validate_commit_at_offset(reader, key, byte_offset) {
                Ok(commit) => {
                    return Ok(Metadata {
                        header,
                        tx_range: Range {
                            start: min_tx_offset,
                            end: commit.tx_range.end,
                        },
                        size_in_bytes: byte_offset + commit.size_in_bytes,
                        max_epoch: commit.epoch,
                        max_commit_offset: commit.tx_range.start,
                    });
                }

                // `TxOffset` at `byte_offset` is not valid, so try with previous entry
                Err(_) => {
                    candidate_last_key = key.saturating_sub(1);
                    if candidate_last_key == 0 {
                        break;
                    }
                }
            }
        }

        Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("No valid commit found in index up to key: {}", candidate_last_key),
        ))
    }

    /// Validates and decodes a commit at `byte_offset` in the segment.
    ///
    /// # Returns
    /// * `Ok(commit::Metadata)` - If a valid commit is found with matching transaction offset
    /// * `Err` - If commit can't be decoded or has mismatched transaction offset
    fn validate_commit_at_offset<R: io::Read + io::Seek>(
        reader: &mut R,
        tx_offset: TxOffset,
        byte_offset: u64,
    ) -> io::Result<commit::Metadata> {
        reader.seek(SeekFrom::Start(byte_offset))?;
        let commit = commit::Metadata::extract(reader)?
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "failed to decode commit"))?;

        if commit.tx_range.start != tx_offset {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "mismatch key in index offset file: expected={} actual={}",
                    tx_offset, commit.tx_range.start
                ),
            ));
        }

        Ok(commit)
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use super::*;
    use crate::{payload::ArrayDecoder, repo, Options};
    use itertools::Itertools;
    use proptest::prelude::*;
    use spacetimedb_paths::server::CommitLogDir;
    use tempfile::tempdir;

    #[test]
    fn header_roundtrip() {
        let hdr = Header {
            log_format_version: 42,
            checksum_algorithm: 7,
        };

        let mut buf = [0u8; Header::LEN];
        hdr.write(&mut &mut buf[..]).unwrap();
        let h2 = Header::decode(&buf[..]).unwrap();

        assert_eq!(hdr, h2);
    }

    #[test]
    fn write_read_roundtrip() {
        let repo = repo::Memory::default();

        let mut writer = repo::create_segment_writer(&repo, Options::default(), Commit::DEFAULT_EPOCH, 0).unwrap();
        writer.append([0; 32]).unwrap();
        writer.append([1; 32]).unwrap();
        writer.append([2; 32]).unwrap();
        writer.commit().unwrap();

        let reader = repo::open_segment_reader(&repo, DEFAULT_LOG_FORMAT_VERSION, 0).unwrap();
        let header = reader.header;
        let commit = reader
            .commits()
            .next()
            .expect("expected one commit")
            .expect("unexpected IO");

        assert_eq!(
            header,
            Header {
                log_format_version: DEFAULT_LOG_FORMAT_VERSION,
                checksum_algorithm: DEFAULT_CHECKSUM_ALGORITHM
            }
        );
        assert_eq!(commit.min_tx_offset, 0);
        assert_eq!(commit.records, [[0; 32], [1; 32], [2; 32]].concat());
    }

    #[test]
    fn metadata() {
        let repo = repo::Memory::default();

        let mut writer = repo::create_segment_writer(&repo, Options::default(), Commit::DEFAULT_EPOCH, 0).unwrap();
        // Commit 0..2
        writer.append([0; 32]).unwrap();
        writer.append([0; 32]).unwrap();
        writer.commit().unwrap();
        // Commit 2..3
        writer.append([1; 32]).unwrap();
        writer.commit().unwrap();
        // Commit 3..5
        writer.append([2; 32]).unwrap();
        writer.append([2; 32]).unwrap();
        writer.commit().unwrap();

        let reader = repo::open_segment_reader(&repo, DEFAULT_LOG_FORMAT_VERSION, 0).unwrap();
        let metadata = reader.metadata().unwrap();

        assert_eq!(
            metadata,
            Metadata {
                header: Header::default(),
                tx_range: Range { start: 0, end: 5 },
                // header + 5 txs + 3 commits
                size_in_bytes: (Header::LEN + (5 * 32) + (3 * Commit::FRAMING_LEN)) as u64,
                max_epoch: Commit::DEFAULT_EPOCH,
                max_commit_offset: 3
            }
        );
    }

    #[test]
    fn commits() {
        let repo = repo::Memory::default();
        let commits = vec![vec![[1; 32], [2; 32]], vec![[3; 32]], vec![[4; 32], [5; 32]]];

        let mut writer = repo::create_segment_writer(&repo, Options::default(), Commit::DEFAULT_EPOCH, 0).unwrap();
        for commit in &commits {
            for tx in commit {
                writer.append(*tx).unwrap();
            }
            writer.commit().unwrap();
        }

        let reader = repo::open_segment_reader(&repo, DEFAULT_LOG_FORMAT_VERSION, 0).unwrap();
        let mut commits1 = Vec::with_capacity(commits.len());
        let mut min_tx_offset = 0;
        for txs in commits {
            commits1.push(Commit {
                min_tx_offset,
                n: txs.len() as u16,
                records: txs.concat(),
                epoch: 0,
            });
            min_tx_offset += txs.len() as u64;
        }
        let commits2 = reader
            .commits()
            .map_ok(Into::into)
            .collect::<Result<Vec<Commit>, _>>()
            .unwrap();
        assert_eq!(commits1, commits2);
    }

    #[test]
    fn transactions() {
        let repo = repo::Memory::default();
        let commits = vec![vec![[1; 32], [2; 32]], vec![[3; 32]], vec![[4; 32], [5; 32]]];

        let mut writer = repo::create_segment_writer(&repo, Options::default(), Commit::DEFAULT_EPOCH, 0).unwrap();
        for commit in &commits {
            for tx in commit {
                writer.append(*tx).unwrap();
            }
            writer.commit().unwrap();
        }

        let reader = repo::open_segment_reader(&repo, DEFAULT_LOG_FORMAT_VERSION, 0).unwrap();
        let txs = reader
            .transactions(&ArrayDecoder)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            txs,
            commits
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(offset, txdata)| Transaction {
                    offset: offset as u64,
                    txdata
                })
                .collect::<Vec<_>>()
        );
    }

    proptest! {
        #[test]
        fn max_records_in_commit(max_records_in_commit in any::<NonZeroU16>()) {
            let mut writer = Writer {
                commit: Commit::default(),
                inner: BufWriter::new(Vec::new()),

                min_tx_offset: 0,
                bytes_written: 0,

                max_records_in_commit,

                offset_index_head: None,
            };

            for i in 0..max_records_in_commit.get() {
                assert!(
                    writer.append([0; 16]).is_ok(),
                    "less than {} records written: {}",
                    max_records_in_commit.get(),
                    i
                );
            }
            assert!(
                writer.append([0; 16]).is_err(),
                "more than {} records written",
                max_records_in_commit.get()
            );
        }
    }

    #[test]
    fn next_tx_offset() {
        let mut writer = Writer {
            commit: Commit::default(),
            inner: BufWriter::new(Vec::new()),

            min_tx_offset: 0,
            bytes_written: 0,

            max_records_in_commit: NonZeroU16::MAX,
            offset_index_head: None,
        };

        assert_eq!(0, writer.next_tx_offset());
        writer.append([0; 16]).unwrap();
        assert_eq!(0, writer.next_tx_offset());
        writer.commit().unwrap();
        assert_eq!(1, writer.next_tx_offset());
        writer.commit().unwrap();
        assert_eq!(1, writer.next_tx_offset());
        writer.append([1; 16]).unwrap();
        writer.append([1; 16]).unwrap();
        writer.commit().unwrap();
        assert_eq!(3, writer.next_tx_offset());
    }

    #[test]
    fn offset_index_writer_truncates_to_offset() {
        use spacetimedb_paths::FromPathUnchecked as _;

        let tmp = tempdir().unwrap();
        let commitlog_dir = CommitLogDir::from_path_unchecked(tmp.path());
        let index_path = commitlog_dir.index(0);
        let mut writer = OffsetIndexWriter::new(
            TxOffsetIndexMut::create_index_file(&index_path, 100).unwrap(),
            Options {
                // Ensure we're writing every index entry.
                offset_index_interval_bytes: 127.try_into().unwrap(),
                offset_index_require_segment_fsync: false,
                ..Default::default()
            },
        );

        for i in 1..=10 {
            writer.append_after_commit(i, i * 128, 128).unwrap();
        }
        // Ensure all entries have been written.
        for i in 1..=10 {
            assert_eq!(writer.head.key_lookup(i).unwrap(), (i, i * 128));
        }

        // Truncating to any offset in the written range or larger
        // retains that offset - 1, or the max offset written.
        let truncate_to: TxOffset = rand::random_range(1..=32);
        let retained_key = truncate_to.saturating_sub(1).min(10);
        let retained_val = retained_key * 128;
        let retained = (retained_key, retained_val);

        writer.ftruncate(truncate_to, rand::random()).unwrap();
        assert_eq!(writer.head.key_lookup(truncate_to).unwrap(), retained);
        // Make sure this also holds after reopen.
        drop(writer);
        let index = TxOffsetIndex::open_index_file(&index_path).unwrap();
        assert_eq!(index.key_lookup(truncate_to).unwrap(), retained);
    }
}
