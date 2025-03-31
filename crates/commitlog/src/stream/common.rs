use std::{
    future::Future,
    io,
    ops::{Bound, RangeBounds},
};

use tokio::io::{
    AsyncBufRead, AsyncBufReadExt as _, AsyncRead, AsyncReadExt as _, AsyncSeek, AsyncSeekExt, AsyncWrite,
};

use crate::{commit, repo::Repo};

/// How to convert [`crate::repo::SegmentWriter`]s into async I/O types.
pub trait IntoAsyncWriter {
    type AsyncWriter: AsyncWrite + AsyncFsync + AsyncLen + Unpin + Send;

    fn into_async_writer(self) -> Self::AsyncWriter;
}

impl IntoAsyncWriter for std::fs::File {
    type AsyncWriter = tokio::io::BufWriter<tokio::fs::File>;

    fn into_async_writer(self) -> Self::AsyncWriter {
        tokio::io::BufWriter::new(tokio::fs::File::from_std(self))
    }
}

pub trait AsyncRepo: Repo<SegmentWriter: IntoAsyncWriter<AsyncWriter = Self::AsyncSegmentWriter>> {
    type AsyncSegmentWriter: AsyncWrite + AsyncLen + AsyncFsync + AsyncLen + Unpin + Send;
    type AsyncSegmentReader: AsyncBufRead + AsyncLen + Unpin + Send;

    fn open_segment_reader_async(
        &self,
        offset: u64,
    ) -> impl Future<Output = io::Result<Self::AsyncSegmentReader>> + Send;
}

pub trait AsyncFsync {
    fn fsync(&self) -> impl Future<Output = ()> + Send;
}

impl<T: AsyncWrite + AsyncFsync + Send + Sync> AsyncFsync for tokio::io::BufWriter<T> {
    async fn fsync(&self) {
        self.get_ref().fsync().await
    }
}

impl AsyncFsync for tokio::fs::File {
    async fn fsync(&self) {
        self.sync_data().await.expect("fsync failed")
    }
}

pub trait AsyncLen: AsyncSeek + Unpin + Send {
    fn segment_len(&mut self) -> impl Future<Output = io::Result<u64>> + Send {
        async {
            let old_pos = self.stream_position().await?;
            let len = self.seek(io::SeekFrom::End(0)).await?;
            // If we're already at the end of the file, avoid seeking.
            if old_pos != len {
                self.seek(io::SeekFrom::Start(old_pos)).await?;
            }

            Ok(len)
        }
    }
}

impl<T: AsyncWrite + AsyncLen + Send> AsyncLen for tokio::io::BufWriter<T> {
    async fn segment_len(&mut self) -> io::Result<u64> {
        self.get_mut().segment_len().await
    }
}

impl<T: AsyncRead + AsyncLen + Send> AsyncLen for tokio::io::BufReader<T> {
    async fn segment_len(&mut self) -> io::Result<u64> {
        self.get_mut().segment_len().await
    }
}

impl AsyncLen for tokio::fs::File {}

/// An optionally half-open range.
///
/// Can express both `start..=end` and `start..`.
#[derive(Clone, Copy, Debug)]
pub struct RangeFromMaybeToInclusive {
    /// The start of the range, inclusive.
    pub start: u64,
    /// The end of the range, inclusive, or unbounded if `None`.
    pub end: Option<u64>,
}

impl RangeFromMaybeToInclusive {
    pub fn from_range_bounds(b: impl RangeBounds<u64>) -> Self {
        let start = match b.start_bound() {
            Bound::Unbounded => 0,
            Bound::Included(start) => *start,
            Bound::Excluded(start) => start + 1,
        };
        let end = match b.end_bound() {
            Bound::Unbounded => None,
            Bound::Included(end) => Some((*end).max(start)),
            Bound::Excluded(end) => Some(end.saturating_sub(1).max(start)),
        };

        Self { start, end }
    }

    pub fn is_empty(&self) -> bool {
        self.end.is_some_and(|end| end <= self.start)
    }

    pub fn contains(&self, item: &u64) -> bool {
        item >= &self.start
            && match &self.end {
                None => true,
                Some(end) => item <= end,
            }
    }
}

impl RangeBounds<u64> for RangeFromMaybeToInclusive {
    fn start_bound(&self) -> Bound<&u64> {
        Bound::Included(&self.start)
    }

    fn end_bound(&self) -> Bound<&u64> {
        self.end.as_ref().map(Bound::Included).unwrap_or(Bound::Unbounded)
    }
}

#[derive(Default)]
pub(super) struct CommitBuf {
    pub header: [u8; commit::Header::LEN],
    pub body: Vec<u8>,
}

impl CommitBuf {
    pub fn as_buf(&self) -> impl bytes::Buf + '_ {
        bytes::Buf::chain(&self.header[..], &self.body[..])
    }

    pub fn as_reader(&self) -> impl io::Read + '_ {
        io::Read::chain(&self.header[..], &self.body[..])
    }

    pub fn filled_len(&self) -> usize {
        self.header.len() + self.body.len()
    }
}

pub(super) enum DidReadExact {
    All,
    Eof,
}

impl DidReadExact {
    pub fn is_eof(&self) -> bool {
        matches!(self, Self::Eof)
    }
}

pub(super) async fn read_exact(src: &mut (impl AsyncRead + Unpin), buf: &mut [u8]) -> io::Result<DidReadExact> {
    src.read_exact(buf).await.map(|_| DidReadExact::All).or_else(|e| {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            Ok(DidReadExact::Eof)
        } else {
            Err(e)
        }
    })
}

/// Get a reference to the [`AsyncBufRead`]'s buffer, filling it if necessary.
pub(super) async fn peek_buf(src: &mut (impl AsyncBufRead + Unpin)) -> io::Result<Option<&[u8]>> {
    let buf = src.fill_buf().await?;
    Ok(if buf.is_empty() { None } else { Some(buf) })
}
