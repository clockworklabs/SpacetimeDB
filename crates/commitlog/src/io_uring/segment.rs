use std::{fs::OpenOptions, io, num::NonZeroU16, ops::Range, path::Path};

use async_stream::try_stream;
use crc32c::{crc32c, crc32c_append};
use futures::Stream;
use log::{debug, warn};
use spacetimedb_sats::buffer::BufReader as _;
use tokio_uring::buf::{BoundedBuf, BoundedBufMut};

pub use crate::segment::Header;
use crate::{
    commit::StoredCommit,
    error::{self, ChecksumMismatch},
    repo::fs::segment_file_name,
    segment::Metadata,
    Commit, Encode, Options,
};

pub struct File {
    pos: u64,
    inner: tokio_uring::fs::File,
}

impl File {
    pub fn new(turing: tokio_uring::fs::File) -> Self {
        Self { pos: 0, inner: turing }
    }

    #[allow(unused)]
    pub fn set_position(&mut self, pos: u64) {
        self.pos = pos;
    }

    pub async fn write_all<T: BoundedBuf>(&mut self, buf: T) -> io::Result<()> {
        let (res, buf) = self.inner.write_all_at(buf, self.pos).await;
        res.map(|()| {
            self.pos += buf.bytes_init() as u64;
        })
    }

    pub async fn read_exact<T: BoundedBufMut>(&mut self, buf: T) -> io::Result<T> {
        let (res, buf) = self.inner.read_exact_at(buf, self.pos).await;
        res.map(|()| {
            self.pos += buf.bytes_init() as u64;
            buf
        })
    }

    pub async fn sync_all(&self) -> io::Result<()> {
        self.inner.sync_all().await
    }

    #[allow(unused)]
    pub async fn sync_data(&self) -> io::Result<()> {
        self.inner.sync_data().await
    }

    pub async fn close(self) -> io::Result<()> {
        self.inner.close().await
    }
}

impl crate::segment::Header {
    async fn fwrite(&self, f: &mut File) -> io::Result<()> {
        f.write_all(&crate::segment::MAGIC[..]).await?;
        f.write_all(vec![self.log_format_version, self.checksum_algorithm, 0, 0])
            .await?;

        Ok(())
    }

    async fn fread(f: &mut File) -> io::Result<Self> {
        let buf = f.read_exact(Vec::with_capacity(Self::LEN)).await?;

        if !buf.starts_with(&crate::segment::MAGIC) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "segment header does not start with magic",
            ));
        }

        Ok(Self {
            log_format_version: buf[crate::segment::MAGIC.len()],
            checksum_algorithm: buf[crate::segment::MAGIC.len() + 1],
        })
    }
}

impl crate::segment::Metadata {
    async fn fextract(min_tx_offset: u64, f: &mut File) -> Result<Self, error::SegmentMetadata> {
        let mut sofar = Self {
            header: Header::fread(f).await?,
            tx_range: Range {
                start: min_tx_offset,
                end: min_tx_offset,
            },
            size_in_bytes: Header::LEN as u64,
        };

        async fn commit_meta(
            f: &mut File,
            sofar: &crate::segment::Metadata,
        ) -> Result<Option<crate::commit::Metadata>, error::SegmentMetadata> {
            crate::commit::Metadata::fextract(f).await.map_err(|e| {
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

        while let Some(commit) = commit_meta(f, &sofar).await? {
            if commit.tx_range.start != sofar.tx_range.end {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "out-of-order offset: expected={} actual={}",
                        sofar.tx_range.end, commit.tx_range.start
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

impl crate::commit::Commit {
    async fn fread(f: &mut File) -> io::Result<Option<Self>> {
        crate::commit::StoredCommit::fread(f)
            .await
            .map(|maybe_commit| maybe_commit.map(Self::from))
    }

    async fn fwrite(&self, f: &mut File) -> io::Result<()> {
        let min_tx_offset = self.min_tx_offset.to_le_bytes();
        let n = self.n.to_le_bytes();
        let len = (self.records.len() as u32).to_le_bytes();

        let mut crc = crc32c(&min_tx_offset);
        crc = crc32c_append(crc, &n);
        crc = crc32c_append(crc, &len);
        crc = crc32c_append(crc, &self.records);

        let mut buf = Vec::with_capacity(min_tx_offset.len() + n.len() + len.len());
        buf.extend_from_slice(&min_tx_offset);
        buf.extend_from_slice(&n);
        buf.extend_from_slice(&len);
        f.write_all(buf).await?;
        f.write_all(self.records.clone()).await?;
        f.write_all(crc.to_le_bytes().to_vec()).await?;

        Ok(())
    }
}

impl crate::commit::StoredCommit {
    async fn fread(f: &mut File) -> io::Result<Option<Self>> {
        let Some((hdr, mut crc)) = crate::commit::Header::fread(f).await? else {
            return Ok(None);
        };

        let records = f.read_exact(Vec::with_capacity(hdr.len as usize)).await?;
        crc = crc32c_append(crc, &records);
        let chk = {
            let buf = f.read_exact(Vec::with_capacity(4)).await?;
            let arr: [u8; 4] = buf.try_into().unwrap();
            u32::from_le_bytes(arr)
        };

        if chk != crc {
            return Err(crate::commit::invalid_data(ChecksumMismatch));
        }

        Ok(Some(Self {
            min_tx_offset: hdr.min_tx_offset,
            n: hdr.n,
            records,
            checksum: crc,
        }))
    }
}

impl crate::commit::Header {
    async fn fread(f: &mut File) -> io::Result<Option<(Self, u32)>> {
        match f.read_exact(Vec::with_capacity(Self::LEN)).await {
            Err(e) => {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    return Ok(None);
                }

                Err(e)
            }
            Ok(buf) => {
                let slice = buf.slice_full();
                match &mut &slice[..] {
                    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] => Ok(None),
                    buf => {
                        let min_tx_offset = buf.get_u64().map_err(crate::commit::decode_error)?;
                        let n = buf.get_u16().map_err(crate::commit::decode_error)?;
                        let len = buf.get_u32().map_err(crate::commit::decode_error)?;
                        let crc = crc32c(&slice);

                        Ok(Some((Self { min_tx_offset, n, len }, crc)))
                    }
                }
            }
        }
    }
}

impl crate::commit::Metadata {
    async fn fextract(f: &mut File) -> io::Result<Option<Self>> {
        Commit::fread(f).await.map(|maybe_commit| maybe_commit.map(Self::from))
    }
}

pub struct Writer {
    pub(crate) commit: Commit,
    pub(crate) inner: File,

    pub(crate) min_tx_offset: u64,
    pub(crate) bytes_written: u64,

    pub(crate) max_records_in_commit: NonZeroU16,
}

impl Writer {
    pub async fn create(root: &Path, opts: Options, offset: u64) -> io::Result<Self> {
        let path = root.join(segment_file_name(offset));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            //.custom_flags(libc::O_DIRECT)
            .open(path)
            .map(tokio_uring::fs::File::from_std)
            .map(File::new)?;
        Header {
            log_format_version: opts.log_format_version,
            checksum_algorithm: Commit::CHECKSUM_ALGORITHM,
        }
        .fwrite(&mut file)
        .await?;
        file.sync_all().await?;

        Ok(Self {
            commit: Commit {
                min_tx_offset: offset,
                n: 0,
                records: Vec::new(),
            },
            inner: file,

            min_tx_offset: offset,
            bytes_written: Header::LEN as u64,

            max_records_in_commit: opts.max_records_in_commit,
        })
    }

    pub async fn resume(root: &Path, opts: Options, offset: u64) -> io::Result<Result<Self, Metadata>> {
        let path = root.join(segment_file_name(offset));
        debug!("resuming writer at {}", path.display());

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            //.custom_flags(libc::O_DIRECT)
            .open(path)
            .map(tokio_uring::fs::File::from_std)
            .map(File::new)?;

        let Metadata {
            header,
            tx_range,
            size_in_bytes,
        } = match Metadata::fextract(offset, &mut file).await {
            Err(error::SegmentMetadata::InvalidCommit { sofar, source }) => {
                warn!("invalid commit in segment {offset}: {source}");
                debug!("sofar={sofar:?}");
                return Ok(Err(sofar));
            }
            Err(error::SegmentMetadata::Io(e)) => return Err(e),
            Ok(meta) => meta,
        };
        header
            .ensure_compatible(opts.log_format_version, Commit::CHECKSUM_ALGORITHM)
            .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;

        Ok(Ok(Self {
            commit: Commit {
                min_tx_offset: tx_range.end,
                n: 0,
                records: Vec::new(),
            },
            inner: file,

            min_tx_offset: tx_range.start,
            bytes_written: size_in_bytes,

            max_records_in_commit: opts.max_records_in_commit,
        }))
    }

    pub fn append<T: Encode>(&mut self, record: T) -> Result<(), T> {
        if self.commit.n == u16::MAX || self.commit.n + 1 > self.max_records_in_commit.get() {
            Err(record)
        } else {
            self.commit.n += 1;
            record.encode_record(&mut self.commit.records);
            Ok(())
        }
    }

    pub async fn commit(&mut self) -> io::Result<()> {
        if self.commit.n == 0 {
            return Ok(());
        }
        let encoded_len = self.commit.encoded_len();
        self.commit.fwrite(&mut self.inner).await?;

        debug!(
            "segment {}: wrote commit {} {} bytes",
            self.min_tx_offset, self.commit.min_tx_offset, encoded_len
        );

        self.bytes_written += encoded_len as u64;
        self.commit.min_tx_offset += self.commit.n as u64;
        self.commit.n = 0;
        self.commit.records.clear();

        Ok(())
    }

    pub async fn fsync(&self) -> io::Result<()> {
        self.inner.sync_all().await
    }

    pub async fn close(self) -> io::Result<()> {
        self.inner.close().await
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

pub struct Reader {
    pub header: Header,
    pub min_tx_offset: u64,
    inner: File,
}

impl Reader {
    pub async fn new(max_log_format_version: u8, min_tx_offset: u64, path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .create(false)
            //.custom_flags(libc::O_DIRECT)
            .open(path)
            .map(tokio_uring::fs::File::from_std)
            .map(File::new)?;
        let header = Header::fread(&mut file).await?;
        header
            .ensure_compatible(max_log_format_version, Commit::CHECKSUM_ALGORITHM)
            .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;

        Ok(Self {
            header,
            min_tx_offset,
            inner: file,
        })
    }

    pub fn commits(self) -> impl Stream<Item = io::Result<StoredCommit>> {
        try_stream! {
            let mut reader = self.inner;
            while let Some(commit) = StoredCommit::fread(&mut reader).await? {
                yield commit
            }
            reader.close().await?;
        }
    }
}
