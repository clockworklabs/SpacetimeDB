use std::{
    io::{self, Seek as _},
    ops::Range,
};

use futures::TryFutureExt;
use log::{debug, error, info, trace, warn};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt},
    task::spawn_blocking,
};

use crate::{
    commit, error,
    index::IndexFile,
    repo::{Repo, SegmentLen as _},
    segment::{self, FileLike as _, OffsetIndexWriter, CHECKSUM_LEN, DEFAULT_CHECKSUM_ALGORITHM},
    stream::common::{read_exact, AsyncFsync},
    Options, StoredCommit, DEFAULT_LOG_FORMAT_VERSION,
};

use super::{
    common::{peek_buf, AsyncLen as _, AsyncRepo, CommitBuf},
    IntoAsyncWriter,
};

/// Progress reporting for [`StreamWriter::write_all`].
pub trait Progress {
    /// Report that the transaction range `tx_range` was written to the
    /// destination commitlog.
    ///
    /// The method is called after each commit written, so should be cheap to
    /// call and never block. A call does not imply that the commit is already
    /// flushed to disk.
    fn range_written(&mut self, tx_range: Range<u64>);
}

impl<T: FnMut(Range<u64>)> Progress for T {
    fn range_written(&mut self, tx_range: Range<u64>) {
        (self)(tx_range)
    }
}

/// Write a raw byte stream of commitlog data to a local commitlog.
///
/// Intended for mirroring commitlogs over the network.
///
/// The source stream is expected to contain the raw commitlog data, including
/// segment headers.
///
/// Whenever a segment header is encountered in the stream, a new segment is
/// created locally. The stream data is decoded as a series of [commits],
/// without inspecting their payload. The checksum of each commit is verified,
/// and it is checked that the commit offsets are contiguous.
///
/// Apart from this **no further validation is performed**, it is assumed that
/// the source is trusted.
///
/// [commits]: crate::commit::StoredCommit
pub struct StreamWriter<R>
where
    R: AsyncRepo + Send + 'static,
{
    repo: R,
    commitlog_options: Options,

    last_written_tx_range: Option<Range<u64>>,
    current_segment: Option<CurrentSegment<R::AsyncSegmentWriter>>,
    commit_buf: CommitBuf,
}

impl<R> StreamWriter<R>
where
    R: AsyncRepo + Send + 'static,
{
    /// Create a new [`StreamWriter`] from the commitlog in `repo`.
    ///
    /// Opens the latest segment of the commitlog for writing.
    /// If the commitlog is empty, no segment is created and [`Self::append_all`]
    /// expects that the source stream starts with a segment header.
    ///
    /// The method traverses the most recent segment to ensure it contains valid
    /// data, and to ensure [`Self::append_all`] can only write consecutive
    /// commits. The `on_trailing` parameter an be used to trim the segment if
    /// it contains trailing invalid data (i.e. due to a partial write).
    pub fn create(repo: R, commitlog_options: Options, on_trailing: OnTrailingData) -> io::Result<Self> {
        Self::create_and_metadata(repo, commitlog_options, on_trailing).map(|(this, _)| this)
    }

    /// Like [`Self::create`], create a new [`StreamWriter`]. Additionally
    /// return the [`segment::Metadata`] of the most recent segment.
    ///
    /// The metadata is `None` if the commitlog is empty.
    pub fn create_and_metadata(
        repo: R,
        commitlog_options: Options,
        on_trailing: OnTrailingData,
    ) -> io::Result<(Self, Option<segment::Metadata>)> {
        let Some(last) = repo.existing_offsets()?.pop() else {
            let this = Self {
                repo,
                commitlog_options,
                last_written_tx_range: None,
                current_segment: None,
                commit_buf: <_>::default(),
            };
            return Ok((this, None));
        };

        let mut segment = repo.open_segment_writer(last)?;
        let mut offset_index = repo
            .get_offset_index(last)
            .inspect_err(|e| {
                warn!("unable to open offset index for segment {last}: {e}");
            })
            .ok();

        let meta = match segment::Metadata::extract(last, &mut segment, offset_index.as_ref()) {
            Ok(sofar) => sofar,
            Err(error::SegmentMetadata::InvalidCommit { sofar, source }) => match on_trailing {
                OnTrailingData::Error => {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, source));
                }
                OnTrailingData::Trim => {
                    info!("trimming segment {last} after invalid commit: {sofar:?}");
                    if let Some(idx) = offset_index.as_mut().map(IndexFile::as_mut) {
                        idx.ftruncate(sofar.tx_range.end, sofar.size_in_bytes)
                            .inspect_err(|e| {
                                error!(
                                    "failed to truncate offset index for segment {} containing trailing data: {}",
                                    last, e
                                )
                            })?;
                        segment.ftruncate(sofar.tx_range.end, sofar.size_in_bytes)?;
                        segment.seek(io::SeekFrom::End(0))?;
                    }
                    sofar
                }
            },
            Err(error::SegmentMetadata::Io(e)) => Err(e)?,
        };

        meta.header
            .ensure_compatible(DEFAULT_LOG_FORMAT_VERSION, DEFAULT_CHECKSUM_ALGORITHM)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let current_segment = CurrentSegment {
            header: meta.header,
            segment: segment.into_async_writer(),
            offset_index: offset_index.map(|index| OffsetIndexWriter::new(index.into(), commitlog_options)),
        };

        let this = Self {
            repo,
            commitlog_options,
            last_written_tx_range: Some(meta.tx_range.clone()),
            current_segment: Some(current_segment),
            commit_buf: <_>::default(),
        };

        Ok((this, Some(meta)))
    }

    /// Consume `stream` and append it to the local commitog.
    ///
    /// The `stream` should be the suffix after the commitlog already present
    /// in the local [`Repo`]. The method checks that commit offsets are
    /// contiguous.
    ///
    /// Segments are created whenever the stream yields a segment header.
    /// If the stream doesn't start with a segment header, the data is appended
    /// to the latest segment found when the writer was created.
    ///
    /// An offset index is maintained locally per segment according to the
    /// [`Options`] used for [`Self::create`]ing the writer.
    ///
    /// Writing data to the commitlog incrementally by calling `append_all`
    /// repeatedly is supported. However, I/O errors may leave the local
    /// commitlog in an inconsistent state. To prevent further appends, this
    /// method consumes `self`, and returns it back if the input `stream` was
    /// consumed successfully. In case of errors, the caller must re-open the
    /// writer via [`Self::create`] in order to perform consistency checks.
    ///
    /// Segments and their offset indexes are synced to disk when a new
    /// segment is created while processing the input stream.
    ///
    /// The caller should use [`Self::sync_all`] to ensure that if a segment
    /// remains open after `append_all`, it is synced to disk.
    pub async fn append_all(
        mut self,
        mut stream: impl AsyncBufRead + Unpin,
        mut progress: impl Progress,
    ) -> io::Result<Self> {
        loop {
            let Some(buf) = peek_buf(&mut stream).await? else {
                break;
            };

            let mut current_segment = if buf.starts_with(&segment::MAGIC) {
                // Ensure the previous segment, if any, is fsync'ed.
                self.close_current_segment().await?;
                // Ensure we actually have a valid segment header.
                let header =
                    segment::Header::decode(buf).inspect_err(|e| warn!("failed to decode segment header: {e}"))?;
                trace!(
                    "create segment at {}",
                    self.last_written_tx_range
                        .as_ref()
                        .map(|range| range.end)
                        .unwrap_or_default()
                );
                let (mut segment, index) = spawn_blocking({
                    let repo = self.repo.clone();
                    let last_written_tx_range = self.last_written_tx_range.clone();
                    let commitlog_options = self.commitlog_options;
                    move || create_segment(repo, last_written_tx_range, commitlog_options)
                })
                .await
                .unwrap()
                .map(|(segment, index)| (segment.into_async_writer(), index))?;

                segment.write_all(&buf[..segment::Header::LEN]).await?;
                stream.consume(segment::Header::LEN as _);

                CurrentSegment {
                    header,
                    segment,
                    offset_index: index,
                }
            } else if let Some(current_segment) = self.current_segment.take() {
                current_segment
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "no current segment, expected segment header",
                ));
            };

            // What follows is commits to be written to `current_segment`,
            // until we encounter EOF or a segment marker.
            let res = self
                .append_all_inner(&mut stream, &mut current_segment, &mut progress)
                .await;
            // Ensure we flush application buffers (BufWriter).
            current_segment.segment.flush().await?;
            let maybe_eof = res?;
            // Put back segment, so it is available for syncing or closing.
            self.current_segment = Some(current_segment);
            match maybe_eof {
                AppendInnerResult::StreamExhausted => break,
                AppendInnerResult::SegmentMarker => continue,
            }
        }

        Ok(self)
    }

    /// Flush and sync the currently written-to segment (if any) to disk.
    ///
    /// Dropping a [`StreamWriter`] will attempt to invoke this, but any errors
    /// will not be visible. Also, if the async runtime is already shutting down,
    /// the task spawned by the [`Drop`] impl may not get a chance to run.
    pub async fn sync_all(&mut self) -> io::Result<()> {
        let Some(current_segment) = self.current_segment.as_mut() else {
            return Ok(());
        };
        current_segment.flush_and_sync().await
    }

    async fn append_all_inner(
        &mut self,
        stream: &mut (impl AsyncBufRead + Unpin),
        current_segment: &mut CurrentSegment<R::AsyncSegmentWriter>,
        progress: &mut impl Progress,
    ) -> io::Result<AppendInnerResult> {
        let mut bytes_written = current_segment
            .segment
            .segment_len()
            .await?
            // We may not have flushed the segment header yet,
            // but the offset index needs to be offset by the header length.
            .max(segment::Header::LEN as _);

        loop {
            let Some(buf) = peek_buf(stream).await? else {
                // The stream is exhausted, break the outer loop.
                trace!("eof");
                return Ok(AppendInnerResult::StreamExhausted);
            };
            if buf.starts_with(&segment::MAGIC) {
                // New segment, break inner loop.
                trace!("segment marker");
                return Ok(AppendInnerResult::SegmentMarker);
            }

            // Read the header, so we can determine the size of the commit.
            if read_exact(stream, &mut self.commit_buf.header).await?.is_eof() {
                return Ok(AppendInnerResult::StreamExhausted);
            }
            let Some(commit_header) = commit::Header::decode(&self.commit_buf.header[..])
                .inspect_err(|e| warn!("failed to decode commit header: {e}"))?
            else {
                // Nb. eof handled above.
                return Err(io::Error::new(io::ErrorKind::InvalidData, "all-zeroes commit header"));
            };

            // Read the rest of the commit.
            self.commit_buf.body.resize(
                commit_header.len as usize + CHECKSUM_LEN[current_segment.header.checksum_algorithm as usize],
                0,
            );
            stream.read_exact(&mut self.commit_buf.body).await?;
            // Decode the commit and verify its checksum.
            let commit = StoredCommit::decode(self.commit_buf.as_reader())
                .inspect_err(|e| warn!("failed to decode commit: {e}"))?
                .expect("commit decode cannot return `None` because we already decoded the header");

            // Check that the commit offset is what we expect.
            let expected_offset = self
                .last_written_tx_range
                .as_ref()
                .map(|range| range.end)
                .unwrap_or_default();
            if commit.min_tx_offset != expected_offset {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "expected commit offset {} but encountered {}",
                        expected_offset, commit.min_tx_offset
                    ),
                ));
            }
            trace!("received commit {commit:?}");

            // Write the commit and report progress.
            current_segment
                .segment
                .write_all_buf(&mut self.commit_buf.as_buf())
                .await?;
            let written_range = commit.min_tx_offset..(commit.min_tx_offset + commit.n as u64);
            self.last_written_tx_range = Some(written_range.clone());
            progress.range_written(written_range);

            let commit_len = (self.commit_buf.header.len() + self.commit_buf.body.len()) as u64;

            // Update to offset index if we have one.
            if let Some(offset_index) = current_segment.offset_index.as_mut() {
                debug!(
                    "append_after_commit min_tx_offset={} bytes_written={} commit_len={}",
                    commit.min_tx_offset, bytes_written, commit_len
                );
                offset_index
                    .append_after_commit(commit.min_tx_offset, bytes_written, commit_len)
                    .inspect_err(|e| warn!("failed to append to offset index: {e}"))
                    .ok();
            }

            bytes_written += commit_len;
        }
    }

    async fn close_current_segment(&mut self) -> io::Result<()> {
        if let Some(current_segment) = self.current_segment.take() {
            trace!("closing current segment");
            current_segment.close().await?;
        }

        Ok(())
    }
}

impl<R> Drop for StreamWriter<R>
where
    R: AsyncRepo + Send + 'static,
{
    fn drop(&mut self) {
        if let Some(current_segment) = self.current_segment.take() {
            trace!("closing current segment on writer drop");
            tokio::spawn(
                current_segment
                    .close()
                    .inspect_err(|e| warn!("error closing segment on drop: {e}")),
            );
        }
    }
}

/// What to do when [`StreamWriter::create`] detects trailing (invalid) data
/// in the commitlog.
#[derive(Default)]
pub enum OnTrailingData {
    /// Return an error. This is the default.
    #[default]
    Error,
    /// Remove the suffix of the log after the last valid commit.
    Trim,
}

enum AppendInnerResult {
    StreamExhausted,
    SegmentMarker,
}

struct CurrentSegment<W> {
    header: segment::Header,
    segment: W,
    offset_index: Option<OffsetIndexWriter>,
}

impl<W: AsyncWriteExt + AsyncFsync + Unpin> CurrentSegment<W> {
    async fn close(mut self) -> io::Result<()> {
        self.flush_and_sync().await
    }

    async fn flush_and_sync(&mut self) -> io::Result<()> {
        self.segment.flush().await?;
        self.segment.fsync().await;
        if let Some(mut index) = self.offset_index.take() {
            let index = spawn_blocking(move || {
                index
                    .fsync()
                    .inspect_err(|e| warn!("offset index fsync failed: {e}"))
                    .ok();
                index
            })
            .await
            .unwrap();
            self.offset_index = Some(index);
        }

        Ok(())
    }
}

/// Create a new segment at offset `last_written_tx_range.end`.
///
/// If the segment file already exists but has a size equal to or smaller than
/// a segment header, the file is truncated. Otherwise, an already existing
/// segment is an error.
fn create_segment<R: Repo>(
    repo: R,
    last_written_tx_range: Option<Range<u64>>,
    commitlog_options: Options,
) -> io::Result<(R::SegmentWriter, Option<OffsetIndexWriter>)> {
    let segment_offset = last_written_tx_range
        .as_ref()
        .map(|range| range.end)
        .unwrap_or_default();
    let segment = repo.create_segment(segment_offset).or_else(|e| {
        if e.kind() == io::ErrorKind::AlreadyExists {
            trace!("segment already exists");
            let mut s = repo.open_segment_writer(segment_offset)?;
            let len = s.segment_len()?;
            trace!("segment len: {len}");
            if len <= segment::Header::LEN as _ {
                trace!("overwriting existing segment");
                s.ftruncate(0, 0)?;
                return Ok(s);
            }
        }

        Err(e)
    })?;
    let index_writer = repo
        .create_offset_index(segment_offset, commitlog_options.offset_index_len())
        .inspect_err(|e| warn!("unable to create offset index segment={segment_offset} err={e:?}"))
        .map(|index| OffsetIndexWriter::new(index, commitlog_options))
        .ok();

    Ok((segment, index_writer))
}
