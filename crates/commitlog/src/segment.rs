use std::{
    fs::File,
    io::{self, BufWriter, Write as _},
    num::NonZeroU16,
    ops::Range,
};

use log::debug;

use crate::{
    commit::{self, Commit, StoredCommit},
    error,
    payload::Encode,
};

pub const MAGIC: [u8; 6] = [b'(', b'd', b's', b')', b'^', b'2'];

pub const DEFAULT_LOG_FORMAT_VERSION: u8 = 0;
pub const DEFAULT_CHECKSUM_ALGORITHM: u8 = CHECKSUM_ALGORITHM_CRC32C;

pub const CHECKSUM_ALGORITHM_CRC32C: u8 = 0;

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

#[derive(Debug)]
pub struct Writer<W: io::Write> {
    pub(crate) commit: Commit,
    pub(crate) inner: BufWriter<W>,

    pub(crate) min_tx_offset: u64,
    pub(crate) bytes_written: u64,

    pub(crate) max_records_in_commit: NonZeroU16,
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
    pub fn commit(&mut self) -> io::Result<()> {
        if self.commit.n == 0 {
            return Ok(());
        }
        self.commit.write(&mut self.inner)?;
        self.inner.flush()?;

        self.bytes_written += self.commit.encoded_len() as u64;
        self.commit.min_tx_offset += self.commit.n as u64;
        self.commit.n = 0;
        self.commit.records.clear();

        Ok(())
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
    fn fsync(&self) -> io::Result<()>;
    fn ftruncate(&self, size: u64) -> io::Result<()>;
}

impl FileLike for File {
    fn fsync(&self) -> io::Result<()> {
        self.sync_all()
    }

    fn ftruncate(&self, size: u64) -> io::Result<()> {
        self.set_len(size)
    }
}

impl<W: io::Write + FileLike> FileLike for BufWriter<W> {
    fn fsync(&self) -> io::Result<()> {
        self.get_ref().fsync()
    }

    fn ftruncate(&self, size: u64) -> io::Result<()> {
        self.get_ref().ftruncate(size)
    }
}

impl<W: io::Write + FileLike> FileLike for Writer<W> {
    fn fsync(&self) -> io::Result<()> {
        self.inner.fsync()
    }

    fn ftruncate(&self, size: u64) -> io::Result<()> {
        self.inner.ftruncate(size)
    }
}

#[derive(Debug)]
pub struct Reader<R> {
    pub header: Header,
    pub min_tx_offset: u64,
    inner: R,
}

impl<R: io::Read> Reader<R> {
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

impl<R: io::Read> Reader<R> {
    pub fn commits(self) -> Commits<R> {
        Commits {
            header: self.header,
            reader: io::BufReader::new(self.inner),
        }
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
            .map_ok(move |(version, commit)| commit.into_transactions(version, de))
            .flatten_ok()
            .flatten_ok()
    }

    #[cfg(test)]
    pub(crate) fn metadata(self) -> Result<Metadata, error::SegmentMetadata> {
        Metadata::with_header(self.min_tx_offset, self.header, io::BufReader::new(self.inner))
    }
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
    reader: io::BufReader<R>,
}

impl<R: io::Read> Iterator for Commits<R> {
    type Item = io::Result<StoredCommit>;

    fn next(&mut self) -> Option<Self::Item> {
        StoredCommit::decode(&mut self.reader).transpose()
    }
}

#[cfg(test)]
impl<R: io::Read> Commits<R> {
    pub fn with_log_format_version(self) -> impl Iterator<Item = io::Result<(u8, StoredCommit)>> {
        CommitsWithVersion { inner: self }
    }
}

#[cfg(test)]
struct CommitsWithVersion<R> {
    inner: Commits<R>,
}

#[cfg(test)]
impl<R: io::Read> Iterator for CommitsWithVersion<R> {
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
    pub header: Header,
    pub tx_range: Range<u64>,
    pub size_in_bytes: u64,
}

impl Metadata {
    /// Read and validate metadata from a segment.
    ///
    /// This traverses the entire segment, consuming thre `reader.
    /// Doing so is necessary to determine the `max_tx_offset` and `size_in_bytes`.
    pub(crate) fn extract<R: io::Read>(min_tx_offset: u64, mut reader: R) -> Result<Self, error::SegmentMetadata> {
        let header = Header::decode(&mut reader)?;
        Self::with_header(min_tx_offset, header, reader)
    }

    fn with_header<R: io::Read>(
        min_tx_offset: u64,
        header: Header,
        mut reader: R,
    ) -> Result<Self, error::SegmentMetadata> {
        let mut sofar = Self {
            header,
            tx_range: Range {
                start: min_tx_offset,
                end: min_tx_offset,
            },
            size_in_bytes: Header::LEN as u64,
        };

        fn commit_meta<R: io::Read>(
            reader: &mut R,
            sofar: &Metadata,
        ) -> Result<Option<commit::Metadata>, error::SegmentMetadata> {
            commit::Metadata::extract(reader).map_err(|e| {
                if e.kind() == io::ErrorKind::InvalidData {
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
        }

        Ok(sofar)
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use super::*;
    use crate::{payload::ArrayDecoder, repo, Options};
    use itertools::Itertools;
    use proptest::prelude::*;

    #[test]
    fn header_roundtrip() {
        let hdr = Header {
            log_format_version: 42,
            checksum_algorithm: 7,
        };

        let mut buf = [0u8; Header::LEN];
        hdr.write(&mut buf[..]).unwrap();
        let h2 = Header::decode(&buf[..]).unwrap();

        assert_eq!(hdr, h2);
    }

    #[test]
    fn write_read_roundtrip() {
        let repo = repo::Memory::default();

        let mut writer = repo::create_segment_writer(&repo, Options::default(), 0).unwrap();
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

        let mut writer = repo::create_segment_writer(&repo, Options::default(), 0).unwrap();
        writer.append([0; 32]).unwrap();
        writer.append([0; 32]).unwrap();
        writer.commit().unwrap();
        writer.append([1; 32]).unwrap();
        writer.commit().unwrap();
        writer.append([2; 32]).unwrap();
        writer.append([2; 32]).unwrap();
        writer.commit().unwrap();

        let reader = repo::open_segment_reader(&repo, DEFAULT_LOG_FORMAT_VERSION, 0).unwrap();
        let Metadata {
            header: _,
            tx_range,
            size_in_bytes,
        } = reader.metadata().unwrap();

        assert_eq!(tx_range.start, 0);
        assert_eq!(tx_range.end, 5);
        assert_eq!(
            size_in_bytes,
            (Header::LEN + (5 * 32) + (3 * Commit::FRAMING_LEN)) as u64
        );
    }

    #[test]
    fn commits() {
        let repo = repo::Memory::default();
        let commits = vec![vec![[1; 32], [2; 32]], vec![[3; 32]], vec![[4; 32], [5; 32]]];

        let mut writer = repo::create_segment_writer(&repo, Options::default(), 0).unwrap();
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

        let mut writer = repo::create_segment_writer(&repo, Options::default(), 0).unwrap();
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
}
