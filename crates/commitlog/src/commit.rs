use std::{
    io::{self, Read, Write},
    ops::Range,
};

use crc32c::{Crc32cReader, Crc32cWriter};
use spacetimedb_sats::buffer::{BufReader, Cursor, DecodeError};

use crate::{
    error::ChecksumMismatch, payload::Decoder, segment::CHECKSUM_ALGORITHM_CRC32C, Transaction,
    DEFAULT_LOG_FORMAT_VERSION,
};

#[derive(Default)]
enum Version {
    V0,
    #[default]
    V1,
}

pub struct Header {
    pub min_tx_offset: u64,
    pub epoch: u64,
    pub n: u16,
    pub len: u32,
}

impl Header {
    pub const LEN: usize = /* offset */ 8 + /* epoch */ 8 + /* n */ 2 + /* len */  4;

    /// Read [`Self::LEN`] bytes from `reader` and interpret them as the
    /// "header" of a [`Commit`].
    ///
    /// Returns `None` if:
    ///
    /// - The reader cannot provide exactly [`Self::LEN`] bytes
    ///
    ///   I.e. it is at EOF
    ///
    /// - Or, the read bytes are all zeroes
    ///
    ///   This is to allow preallocation of segments.
    ///
    pub fn decode<R: Read>(reader: R) -> io::Result<Option<Self>> {
        Self::decode_v1(reader)
    }

    fn decode_internal<R: Read>(reader: R, v: Version) -> io::Result<Option<Self>> {
        use Version::*;
        match v {
            V0 => Self::decode_v0(reader),
            V1 => Self::decode_v1(reader),
        }
    }

    fn decode_v0<R: Read>(mut reader: R) -> io::Result<Option<Self>> {
        let mut hdr = [0; Self::LEN - 8];
        if let Err(e) = reader.read_exact(&mut hdr) {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                return Ok(None);
            }

            return Err(e);
        }
        match &mut hdr.as_slice() {
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] => Ok(None),
            buf => {
                let min_tx_offset = buf.get_u64().map_err(decode_error)?;
                let n = buf.get_u16().map_err(decode_error)?;
                let len = buf.get_u32().map_err(decode_error)?;

                Ok(Some(Self {
                    min_tx_offset,
                    epoch: Commit::DEFAULT_EPOCH,
                    n,
                    len,
                }))
            }
        }
    }

    fn decode_v1<R: Read>(mut reader: R) -> io::Result<Option<Self>> {
        let mut hdr = [0; Self::LEN];
        if let Err(e) = reader.read_exact(&mut hdr) {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                return Ok(None);
            }

            return Err(e);
        }
        match &mut hdr.as_slice() {
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] => Ok(None),
            buf => {
                let min_tx_offset = buf.get_u64().map_err(decode_error)?;
                let epoch = buf.get_u64().map_err(decode_error)?;
                let n = buf.get_u16().map_err(decode_error)?;
                let len = buf.get_u32().map_err(decode_error)?;

                Ok(Some(Self {
                    min_tx_offset,
                    epoch,
                    n,
                    len,
                }))
            }
        }
    }
}

/// Entry type of a [`crate::Commitlog`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Commit {
    /// The offset of the first record in this commit.
    ///
    /// The offset starts from zero and is counted from the beginning of the
    /// entire log.
    pub min_tx_offset: u64,
    /// The epoch within which the commit was created.
    ///
    /// Indicates the monotonically increasing term number of the leader when
    /// the commitlog is being written to in a distributed deployment.
    ///
    /// The default epoch is 0 (zero). It should be used when the log is written
    /// to by a single process.
    ///
    /// Note, however, that an existing log may have a non-zero epoch.
    /// It is currently unspecified how a commitlog is transitioned between
    /// distributed and single-node deployment, wrt the epoch.
    pub epoch: u64,
    /// The number of records in the commit.
    pub n: u16,
    /// A buffer of all records in the commit in serialized form.
    ///
    /// Readers must bring their own [`crate::Decoder`] to interpret this buffer.
    /// `n` indicates how many records the buffer contains.
    pub records: Vec<u8>,
}

impl Commit {
    pub const DEFAULT_EPOCH: u64 = 0;

    pub const FRAMING_LEN: usize = Header::LEN + /* crc32 */ 4;
    pub const CHECKSUM_ALGORITHM: u8 = CHECKSUM_ALGORITHM_CRC32C;

    /// The range of transaction offsets contained in this commit.
    pub fn tx_range(&self) -> Range<u64> {
        self.min_tx_offset..self.min_tx_offset + self.n as u64
    }

    /// Length in bytes of this commit when written to the log via [`Self::write`].
    pub fn encoded_len(&self) -> usize {
        Self::FRAMING_LEN + self.records.len()
    }

    /// Serialize and write `self` to `out`.
    ///
    /// Returns the crc32 checksum of the commit on success.
    pub fn write<W: Write>(&self, out: W) -> io::Result<u32> {
        let mut out = Crc32cWriter::new(out);

        let min_tx_offset = self.min_tx_offset.to_le_bytes();
        let epoch = self.epoch.to_le_bytes();
        let n = self.n.to_le_bytes();
        let len = (self.records.len() as u32).to_le_bytes();

        out.write_all(&min_tx_offset)?;
        out.write_all(&epoch)?;
        out.write_all(&n)?;
        out.write_all(&len)?;
        out.write_all(&self.records)?;

        let crc = out.crc32c();
        let mut out = out.into_inner();
        out.write_all(&crc.to_le_bytes())?;

        Ok(crc)
    }

    /// Attempt to read one [`Commit`] from the given [`Read`]er.
    ///
    /// Returns `None` if the reader is already at EOF.
    ///
    /// Verifies the checksum of the commit. If it doesn't match, an error of
    /// kind [`io::ErrorKind::InvalidData`] with an inner error downcastable to
    /// [`ChecksumMismatch`] is returned.
    ///
    /// To retain access to the checksum, use [`StoredCommit::decode`].
    pub fn decode<R: Read>(reader: R) -> io::Result<Option<Self>> {
        let commit = StoredCommit::decode(reader)?;
        Ok(commit.map(Into::into))
    }

    /// Convert `self` into an iterator yielding [`Transaction`]s.
    ///
    /// The supplied [`Decoder`] is responsible for extracting individual
    /// transactions from the `records` buffer.
    pub fn into_transactions<D: Decoder>(
        self,
        version: u8,
        de: &D,
    ) -> impl Iterator<Item = Result<Transaction<D::Record>, D::Error>> + '_ {
        let records = Cursor::new(self.records);
        (self.min_tx_offset..(self.min_tx_offset + self.n as u64)).scan(records, move |recs, offset| {
            let mut cursor = &*recs;
            let tx = de
                .decode_record(version, offset, &mut cursor)
                .map(|txdata| Transaction { offset, txdata });
            Some(tx)
        })
    }
}

impl From<StoredCommit> for Commit {
    fn from(
        StoredCommit {
            min_tx_offset,
            epoch,
            n,
            records,
            checksum: _,
        }: StoredCommit,
    ) -> Self {
        Self {
            min_tx_offset,
            epoch,
            n,
            records,
        }
    }
}

/// A [`Commit`] as stored on disk.
///
/// Differs from [`Commit`] only in the presence of a `checksum` field, which
/// is computed when encoding a commit for storage.
#[derive(Debug, PartialEq)]
pub struct StoredCommit {
    /// See [`Commit::min_tx_offset`].
    pub min_tx_offset: u64,
    /// See [`Commit::epoch`].
    pub epoch: u64,
    /// See [`Commit::n`].
    pub n: u16,
    /// See [`Commit::records`].
    pub records: Vec<u8>,
    /// The checksum computed when encoding a [`Commit`] for storage.
    pub checksum: u32,
}

impl StoredCommit {
    /// The range of transaction offsets contained in this commit.
    pub fn tx_range(&self) -> Range<u64> {
        self.min_tx_offset..self.min_tx_offset + self.n as u64
    }

    /// Attempt to read one [`StoredCommit`] from the given [`Read`]er.
    ///
    /// Returns `None` if the reader is already at EOF.
    ///
    /// Verifies the checksum of the commit. If it doesn't match, an error of
    /// kind [`io::ErrorKind::InvalidData`] with an inner error downcastable to
    /// [`ChecksumMismatch`] is returned.
    pub fn decode<R: Read>(reader: R) -> io::Result<Option<Self>> {
        Self::decode_internal(reader, DEFAULT_LOG_FORMAT_VERSION)
    }

    pub(crate) fn decode_internal<R: Read>(reader: R, log_format_version: u8) -> io::Result<Option<Self>> {
        let mut reader = Crc32cReader::new(reader);

        let v = if log_format_version == 0 {
            Version::V0
        } else {
            Version::V1
        };
        let Some(hdr) = Header::decode_internal(&mut reader, v)? else {
            return Ok(None);
        };
        let mut records = vec![0; hdr.len as usize];
        reader.read_exact(&mut records)?;

        let chk = reader.crc32c();
        let crc = decode_u32(reader.into_inner())?;

        if chk != crc {
            return Err(invalid_data(ChecksumMismatch));
        }

        Ok(Some(Self {
            min_tx_offset: hdr.min_tx_offset,
            epoch: hdr.epoch,
            n: hdr.n,
            records,
            checksum: crc,
        }))
    }

    /// Convert `self` into an iterator yielding [`Transaction`]s.
    ///
    /// The supplied [`Decoder`] is responsible for extracting individual
    /// transactions from the `records` buffer.
    pub fn into_transactions<D: Decoder>(
        self,
        version: u8,
        de: &D,
    ) -> impl Iterator<Item = Result<Transaction<D::Record>, D::Error>> + '_ {
        Commit::from(self).into_transactions(version, de)
    }
}

/// Numbers needed to compute [`crate::segment::Header`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metadata {
    pub tx_range: Range<u64>,
    pub size_in_bytes: u64,
    pub epoch: u64,
}

impl Metadata {
    /// Extract the [`Metadata`] of a single [`Commit`] from the given reader.
    ///
    /// Note that this decodes the commit due to checksum verification.
    /// Like [`Commit::decode`], returns `None` if the reader is at EOF already.
    pub fn extract<R: io::Read>(reader: R) -> io::Result<Option<Self>> {
        Commit::decode(reader).map(|maybe_commit| maybe_commit.map(Self::from))
    }
}

impl From<Commit> for Metadata {
    fn from(commit: Commit) -> Self {
        Self {
            tx_range: commit.tx_range(),
            size_in_bytes: commit.encoded_len() as u64,
            epoch: commit.epoch,
        }
    }
}

fn decode_u32<R: Read>(mut read: R) -> io::Result<u32> {
    let mut buf = [0; 4];
    read.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn decode_error(e: DecodeError) -> io::Error {
    invalid_data(e)
}

fn invalid_data<E>(e: E) -> io::Error
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    io::Error::new(io::ErrorKind::InvalidData, e)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use proptest::prelude::*;

    use super::*;

    #[test]
    fn commit_roundtrip() {
        let records = vec![0; 128];
        let commit = Commit {
            min_tx_offset: 0,
            n: 3,
            records,
            epoch: Commit::DEFAULT_EPOCH,
        };

        let mut buf = Vec::with_capacity(commit.encoded_len());
        commit.write(&mut buf).unwrap();
        let commit2 = Commit::decode(&mut buf.as_slice()).unwrap().unwrap();

        assert_eq!(commit, commit2);
    }

    proptest! {
        #[test]
        fn bitflip(pos in Header::LEN..512, mask in any::<NonZeroU8>()) {
            let commit = Commit {
                min_tx_offset: 42,
                n: 10,
                records: vec![1; 512],
                epoch: Commit::DEFAULT_EPOCH,
            };

            let mut buf = Vec::with_capacity(commit.encoded_len());
            commit.write(&mut buf).unwrap();

            // Flip bit in the `records` section,
            // so we get `ChecksumMismatch` not any other error.
            buf[pos] ^= mask.get();

            match Commit::decode(&mut buf.as_slice()) {
                Err(e) => {
                    assert_eq!(e.kind(), io::ErrorKind::InvalidData);
                    e.into_inner()
                        .unwrap()
                        .downcast::<ChecksumMismatch>()
                        .expect("IO inner should be checksum mismatch");
                }
                Ok(commit) => panic!("expected checksum mismatch, got valid commit: {commit:?}"),
            }
        }
    }
}
