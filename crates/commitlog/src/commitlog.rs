use std::{
    fmt::Debug,
    io,
    marker::PhantomData,
    mem,
    ops::{Range, RangeBounds},
    vec,
};

use itertools::Itertools;
use log::{debug, info, trace, warn};

use crate::{
    commit::StoredCommit,
    error::{self, source_chain},
    index::IndexError,
    payload::Decoder,
    repo::{self, Repo, TxOffsetIndex},
    segment::{self, FileLike, Transaction, Writer},
    Commit, Encode, Options, DEFAULT_LOG_FORMAT_VERSION,
};

pub use crate::segment::Committed;

/// A commitlog generic over the storage backend as well as the type of records
/// its [`Commit`]s contain.
#[derive(Debug)]
pub struct Generic<R: Repo, T> {
    /// The storage backend.
    pub(crate) repo: R,
    /// The segment currently being written to.
    ///
    /// If we squint, all segments in a log are a non-empty linked list, the
    /// head of which is the segment open for writing.
    pub(crate) head: Writer<R::SegmentWriter>,
    /// The tail of the non-empty list of segments.
    ///
    /// We only retain the min transaction offset of each, from which the
    /// segments can be opened for reading when needed.
    ///
    /// This is a `Vec`, not a linked list, so the last element is the newest
    /// segment (after `head`).
    tail: Vec<u64>,
    /// Configuration options.
    opts: Options,
    /// Type of a single record in this log's [`Commit::records`].
    _record: PhantomData<T>,
    /// Tracks panics/errors to control what happens on drop.
    ///
    /// Set to `true` before any I/O operation, and back to `false` after it
    /// succeeded. This way, we won't try to perform I/O on drop when it is
    /// unlikely to succeed, or even has a chance to panic.
    panicked: bool,
}

impl<R: Repo, T> Generic<R, T> {
    pub fn open(repo: R, opts: Options) -> io::Result<Self> {
        let mut tail = repo.existing_offsets()?;
        if !tail.is_empty() {
            debug!("segments: {tail:?}");
        }
        let head = if let Some(last) = tail.pop() {
            debug!("resuming last segment: {last}");
            // Resume the last segment for writing, or create a new segment
            // starting from the last good commit + 1.
            repo::resume_segment_writer(&repo, opts, last)?.or_else(|meta| {
                // The first commit in the last segment being corrupt is an
                // edge case: we'd try to start a new segment with an offset
                // equal to the already existing one, which would fail.
                //
                // We cannot just skip it either, as we don't know the reason
                // for the corruption (there could be more, potentially
                // recoverable commits in the segment).
                //
                // Thus, provide some context about what is wrong and refuse to
                // start.
                if meta.tx_range.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("repo {}: first commit in resumed segment {} is corrupt", repo, last),
                    ));
                }
                tail.push(meta.tx_range.start);
                repo::create_segment_writer(&repo, opts, meta.max_epoch, meta.tx_range.end)
            })?
        } else {
            debug!("starting fresh log");
            repo::create_segment_writer(&repo, opts, Commit::DEFAULT_EPOCH, 0)?
        };

        Ok(Self {
            repo,
            head,
            tail,
            opts,
            _record: PhantomData,
            panicked: false,
        })
    }

    /// Get the current epoch.
    ///
    /// See also: [`Commit::epoch`].
    pub fn epoch(&self) -> u64 {
        self.head.commit.epoch
    }

    /// Update the current epoch.
    ///
    /// Calls [`Self::commit`] to flush all data of the previous epoch, and
    /// returns the result.
    ///
    /// Does nothing if the given `epoch` is equal to the current epoch.
    ///
    /// # Errors
    ///
    /// If `epoch` is smaller than the current epoch, an error of kind
    /// [`io::ErrorKind::InvalidInput`] is returned.
    ///
    /// Also see [`Self::commit`].
    pub fn set_epoch(&mut self, epoch: u64) -> io::Result<Option<Committed>> {
        use std::cmp::Ordering::*;

        match epoch.cmp(&self.head.epoch()) {
            Less => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "new epoch is smaller than current epoch",
            )),
            Equal => Ok(None),
            Greater => {
                let res = self.commit()?;
                self.head.set_epoch(epoch);
                Ok(res)
            }
        }
    }

    /// Write the currently buffered data to storage and rotate segments as
    /// necessary.
    ///
    /// Note that this does not imply that the data is durable, in particular
    /// when a filesystem storage backend is used. Call [`Self::sync`] to flush
    /// any OS buffers to stable storage.
    ///
    /// # Errors
    ///
    /// If an error occurs writing the data, the current [`Commit`] buffer is
    /// retained, but a new segment is created. Retrying in case of an `Err`
    /// return value thus will write the current data to that new segment.
    ///
    /// If this fails, however, the next attempt to create a new segment will
    /// fail with [`io::ErrorKind::AlreadyExists`]. Encountering this error kind
    /// this means that something is seriously wrong underlying storage, and the
    /// caller should stop writing to the log.
    pub fn commit(&mut self) -> io::Result<Option<Committed>> {
        self.panicked = true;
        let writer = &mut self.head;
        let sz = writer.commit.encoded_len();
        // If the segment is empty, but the commit exceeds the max size,
        // we got a huge commit which needs to be written even if that
        // results in a huge segment.
        let should_rotate = !writer.is_empty() && writer.len() + sz as u64 > self.opts.max_segment_size;
        let writer = if should_rotate {
            self.sync();
            self.start_new_segment()?
        } else {
            writer
        };

        let ret = writer.commit().or_else(|e| {
            warn!("Commit failed: {e}");
            // Nb.: Don't risk a panic by calling `self.sync()`.
            // We already gave up on the last commit, and will retry it next time.
            self.start_new_segment()?;
            Err(e)
        });
        self.panicked = false;
        ret
    }

    /// Force the currently active segment to be flushed to storage.
    ///
    /// Using a filesystem backend, this means to call `fsync(2)`.
    ///
    /// # Panics
    ///
    /// As an `fsync` failure leaves a file in a more of less undefined state,
    /// this method panics in this case, thereby preventing any further writes
    /// to the log and forcing the user to re-read the state from disk.
    pub fn sync(&mut self) {
        self.panicked = true;
        if let Err(e) = self.head.fsync() {
            panic!("Failed to fsync segment: {e}");
        }
        self.panicked = false;
    }

    /// The last transaction offset written to disk, or `None` if nothing has
    /// been written yet.
    ///
    /// Note that this does not imply durability: [`Self::sync`] may not have
    /// been called at this offset.
    pub fn max_committed_offset(&self) -> Option<u64> {
        // Naming is hard: the segment's `next_tx_offset` indicates how many
        // txs are already in the log (it's the next commit's min-tx-offset).
        // If the value is zero, however, the initial commit hasn't been
        // committed yet.
        self.head.next_tx_offset().checked_sub(1)
    }

    /// The first transaction offset written to disk, or `None` if nothing has
    /// been written yet.
    pub fn min_committed_offset(&self) -> Option<u64> {
        self.tail
            .first()
            .copied()
            .or_else(|| (!self.head.is_empty()).then(|| self.head.min_tx_offset()))
    }

    // Helper to obtain a list of the segment offsets which include transaction
    // offset `offset`.
    //
    // The returned `Vec` is sorted in **ascending** order, such that the first
    // element is the segment which contains `offset`.
    //
    // The offset of `self.head` is always included, regardless of how many
    // entries it actually contains.
    fn segment_offsets_from(&self, offset: u64) -> Vec<u64> {
        if offset >= self.head.min_tx_offset {
            vec![self.head.min_tx_offset]
        } else {
            let mut offs = Vec::with_capacity(self.tail.len() + 1);
            if let Some(pos) = self.tail.iter().rposition(|off| off <= &offset) {
                offs.extend_from_slice(&self.tail[pos..]);
                offs.push(self.head.min_tx_offset);
            }

            offs
        }
    }

    pub fn commits_from(&self, offset: u64) -> Commits<R> {
        let offsets = self.segment_offsets_from(offset);
        let segments = Segments {
            offs: offsets.into_iter(),
            repo: self.repo.clone(),
            max_log_format_version: self.opts.log_format_version,
        };
        Commits {
            inner: None,
            segments,
            last_commit: CommitInfo::Initial { next_offset: offset },
            last_error: None,
        }
    }

    pub fn reset(mut self) -> io::Result<Self> {
        info!("hard reset");

        self.panicked = true;
        self.tail.reserve(1);
        self.tail.push(self.head.min_tx_offset);
        for segment in self.tail.iter().rev() {
            debug!("removing segment {segment}");
            self.repo.remove_segment(*segment)?;
        }
        // Prevent finalizer from running by not updating self.panicked.

        Self::open(self.repo.clone(), self.opts)
    }

    pub fn reset_to(mut self, offset: u64) -> io::Result<Self> {
        info!("reset to {offset}");

        self.panicked = true;
        self.tail.reserve(1);
        self.tail.push(self.head.min_tx_offset);
        reset_to_internal(&self.repo, &self.tail, offset)?;
        // Prevent finalizer from running by not updating self.panicked.

        Self::open(self.repo.clone(), self.opts)
    }

    /// Start a new segment, preserving the current head's `Commit`.
    ///
    /// The caller must ensure that the current head is synced to disk as
    /// appropriate. It is not appropriate to sync after a write error, as that
    /// is likely to return an error as well: the `Commit` will be written to
    /// the new segment anyway.
    fn start_new_segment(&mut self) -> io::Result<&mut Writer<R::SegmentWriter>> {
        debug!(
            "starting new segment offset={} prev-offset={}",
            self.head.next_tx_offset(),
            self.head.min_tx_offset()
        );
        let new = repo::create_segment_writer(&self.repo, self.opts, self.head.epoch(), self.head.next_tx_offset())?;
        let old = mem::replace(&mut self.head, new);
        self.tail.push(old.min_tx_offset());
        self.head.commit = old.commit;

        Ok(&mut self.head)
    }
}

impl<R: Repo, T: Encode> Generic<R, T> {
    pub fn append(&mut self, record: T) -> Result<(), T> {
        self.head.append(record)
    }

    pub fn transactions_from<'a, D>(
        &self,
        offset: u64,
        decoder: &'a D,
    ) -> impl Iterator<Item = Result<Transaction<T>, D::Error>> + 'a
    where
        D: Decoder<Record = T>,
        D::Error: From<error::Traversal>,
        R: 'a,
        T: 'a,
    {
        transactions_from_internal(self.commits_from(offset).with_log_format_version(), offset, decoder)
    }

    pub fn fold_transactions_from<D>(&self, offset: u64, decoder: D) -> Result<(), D::Error>
    where
        D: Decoder,
        D::Error: From<error::Traversal>,
    {
        fold_transactions_internal(self.commits_from(offset).with_log_format_version(), decoder, offset..)
    }

    pub fn fold_transaction_range<D>(&self, range: impl RangeBounds<u64>, decoder: D) -> Result<(), D::Error>
    where
        D: Decoder,
        D::Error: From<error::Traversal>,
    {
        use std::ops::Bound::*;

        let start = match range.start_bound() {
            Included(x) => *x,
            Excluded(x) => x + 1,
            Unbounded => 0,
        };
        fold_transactions_internal(self.commits_from(start).with_log_format_version(), decoder, range)
    }
}

impl<R: Repo, T> Drop for Generic<R, T> {
    fn drop(&mut self) {
        if !self.panicked {
            if let Err(e) = self.head.commit() {
                warn!("failed to commit on drop: {e}");
            }
        }
    }
}

/// Extract the most recently written [`segment::Metadata`] from the commitlog
/// in `repo`.
///
/// Returns `None` if the commitlog is empty.
///
/// Note that this function validates the most recent segment, which entails
/// traversing it from the start.
///
/// The function can be used instead of the pattern:
///
/// ```ignore
/// let log = Commitlog::open(..)?;
/// let max_offset = log.max_committed_offset();
/// ```
///
/// like so:
///
/// ```ignore
/// let max_offset = committed_meta(..)?.map(|meta| meta.tx_range.end);
/// ```
///
/// Unlike `open`, no segment will be created in an empty `repo`.
pub fn committed_meta(repo: impl Repo) -> Result<Option<segment::Metadata>, error::SegmentMetadata> {
    let Some(last) = repo.existing_offsets()?.pop() else {
        return Ok(None);
    };

    let mut storage = repo.open_segment_reader(last)?;
    let offset_index = repo.get_offset_index(last).ok();
    segment::Metadata::extract(last, &mut storage, offset_index.as_ref()).map(Some)
}

pub fn commits_from<R: Repo>(repo: R, max_log_format_version: u8, offset: u64) -> io::Result<Commits<R>> {
    let mut offsets = repo.existing_offsets()?;
    if let Some(pos) = offsets.iter().rposition(|&off| off <= offset) {
        offsets = offsets.split_off(pos);
    }
    let segments = Segments {
        offs: offsets.into_iter(),
        repo,
        max_log_format_version,
    };
    Ok(Commits {
        inner: None,
        segments,
        last_commit: CommitInfo::Initial { next_offset: offset },
        last_error: None,
    })
}

pub fn transactions_from<'a, R, D, T>(
    repo: R,
    max_log_format_version: u8,
    offset: u64,
    de: &'a D,
) -> io::Result<impl Iterator<Item = Result<Transaction<T>, D::Error>> + 'a>
where
    R: Repo + 'a,
    D: Decoder<Record = T>,
    D::Error: From<error::Traversal>,
    T: 'a,
{
    commits_from(repo, max_log_format_version, offset)
        .map(|commits| transactions_from_internal(commits.with_log_format_version(), offset, de))
}

pub fn fold_transactions_from<R, D>(repo: R, max_log_format_version: u8, offset: u64, de: D) -> Result<(), D::Error>
where
    R: Repo,
    D: Decoder,
    D::Error: From<error::Traversal> + From<io::Error>,
{
    fold_transaction_range(repo, max_log_format_version, offset.., de)
}

pub fn fold_transaction_range<R, D>(
    repo: R,
    max_log_format_version: u8,
    range: impl RangeBounds<u64>,
    de: D,
) -> Result<(), D::Error>
where
    R: Repo,
    D: Decoder,
    D::Error: From<error::Traversal> + From<io::Error>,
{
    use std::ops::Bound::*;

    let start = match range.start_bound() {
        Included(x) => *x,
        Excluded(x) => x + 1,
        Unbounded => 0,
    };
    let commits = commits_from(repo, max_log_format_version, start)?;
    fold_transactions_internal(commits.with_log_format_version(), de, range)
}

fn transactions_from_internal<'a, R, D, T>(
    commits: CommitsWithVersion<R>,
    offset: u64,
    de: &'a D,
) -> impl Iterator<Item = Result<Transaction<T>, D::Error>> + 'a
where
    R: Repo + 'a,
    D: Decoder<Record = T>,
    D::Error: From<error::Traversal>,
    T: 'a,
{
    commits
        .map(|x| x.map_err(D::Error::from))
        .map_ok(move |(version, commit)| commit.into_transactions(version, offset, de))
        .flatten_ok()
        .map(|x| x.and_then(|y| y))
}

fn fold_transactions_internal<R, D>(
    mut commits: CommitsWithVersion<R>,
    de: D,
    range: impl RangeBounds<u64>,
) -> Result<(), D::Error>
where
    R: Repo,
    D: Decoder,
    D::Error: From<error::Traversal>,
{
    use std::ops::Bound::*;

    // Avoid reading the first commit if it wouldn't be in the range anyway.
    if range_is_empty(&range) {
        return Ok(());
    }

    // `true` if `offset` is outside `range`, s.t. it is smaller than the start
    // bound.
    let before_start = |offset: &u64| match range.start_bound() {
        Included(x) => offset < x,
        Excluded(x) => offset <= x,
        Unbounded => false,
    };
    // `true` if `offset` is outside `range`, s.t. it is greater than the end
    // bound.
    let past_end = |offset: &u64| match range.end_bound() {
        Included(x) => offset > x,
        Excluded(x) => offset >= x,
        Unbounded => false,
    };

    while let Some(commit) = commits.next() {
        let (version, commit) = match commit {
            Ok(version_and_commit) => version_and_commit,
            Err(e) => {
                // Ignore it if the very last commit in the log is broken.
                // The next `append` will fix the log, but the `decoder`
                // has no way to tell whether we're at the end or not.
                // This is unlike the consumer of an iterator, which can
                // perform below check itself.
                if commits.next().is_none() {
                    return Ok(());
                }

                return Err(e.into());
            }
        };
        trace!("commit {} n={} version={}", commit.min_tx_offset, commit.n, version);

        let max_tx_offset = commit.min_tx_offset + commit.n as u64;
        // Skip if no transaction in the commit is in range.
        if before_start(&max_tx_offset) {
            continue;
        }

        let records = &mut commit.records.as_slice();
        for n in 0..commit.n {
            let tx_offset = commit.min_tx_offset + n as u64;
            if before_start(&tx_offset) {
                de.skip_record(version, tx_offset, records)?;
            } else if past_end(&tx_offset) {
                return Ok(());
            } else {
                de.consume_record(version, tx_offset, records)?;
            }
        }
    }

    Ok(())
}

/// Remove all data past the given transaction `offset`.
///
/// The function deletes log segments starting from the newest. As multiple
/// segments cannot be deleted atomically, the log may be left longer than
/// `offset` if the function does not return successfully.
///
/// If the function returns successfully, the most recent [`Commit`] in the
/// log will contain the transaction at `offset`.
///
/// The log must be re-opened if it is to be used after calling this function.
pub fn reset_to(repo: &impl Repo, offset: u64) -> io::Result<()> {
    let segments = repo.existing_offsets()?;
    reset_to_internal(repo, &segments, offset)
}

fn reset_to_internal(repo: &impl Repo, segments: &[u64], offset: u64) -> io::Result<()> {
    for segment in segments.iter().copied().rev() {
        if segment > offset {
            // Segment is outside the offset, so remove it wholesale.
            debug!("removing segment {segment}");
            repo.remove_segment(segment)?;
        } else {
            // Read commit-wise until we find the byte offset.
            let mut reader = repo::open_segment_reader(repo, DEFAULT_LOG_FORMAT_VERSION, segment)?;

            let (index_file, mut byte_offset) = try_seek_using_offset_index(repo, &mut reader, offset)
                .map(|(index_file, byte_offset)| (Some(index_file), byte_offset))
                .unwrap_or((None, segment::Header::LEN as u64));

            let commits = reader.commits();

            for commit in commits {
                let commit = commit?;
                if commit.min_tx_offset > offset {
                    break;
                }
                byte_offset += Commit::from(commit).encoded_len() as u64;
            }

            if byte_offset == segment::Header::LEN as u64 {
                // Segment is empty, just remove it.
                repo.remove_segment(segment)?;
            } else {
                debug!("truncating segment {segment} to {offset} at {byte_offset}");
                let mut file = repo.open_segment_writer(segment)?;

                if let Some(mut index_file) = index_file {
                    let index_file = index_file.as_mut();
                    // Note: The offset index truncates equal or greater,
                    // inclusive. We'd like to retain `offset` in the index, as
                    // the commit is also retained in the log.
                    index_file.ftruncate(offset + 1, byte_offset).map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("Failed to truncate offset index: {e}"),
                        )
                    })?;
                    index_file.async_flush()?;
                }

                file.ftruncate(offset, byte_offset)?;
                // Some filesystems require fsync after ftruncate.
                file.fsync()?;
                break;
            }
        }
    }

    Ok(())
}

pub struct Segments<R> {
    repo: R,
    offs: vec::IntoIter<u64>,
    max_log_format_version: u8,
}

impl<R: Repo> Iterator for Segments<R> {
    type Item = io::Result<segment::Reader<R::SegmentReader>>;

    fn next(&mut self) -> Option<Self::Item> {
        let off = self.offs.next()?;
        debug!("iter segment {off}");
        Some(repo::open_segment_reader(&self.repo, self.max_log_format_version, off))
    }
}

/// Helper for the [`Commits`] iterator.
enum CommitInfo {
    /// Constructed in [`Generic::commits_from`], specifying the offset the next
    /// commit should have.
    Initial { next_offset: u64 },
    /// The last commit seen by the iterator.
    ///
    /// Stores the range of transaction offsets, where `tx_range.end` is the
    /// offset the next commit is expected to have. Also retains the checksum
    /// needed to detect duplicate commits.
    LastSeen { tx_range: Range<u64>, checksum: u32 },
}

impl CommitInfo {
    /// `true` if the last seen commit in self and the provided one have the
    /// same `min_tx_offset`.
    fn same_offset_as(&self, commit: &StoredCommit) -> bool {
        let Self::LastSeen { tx_range, .. } = self else {
            return false;
        };
        tx_range.start == commit.min_tx_offset
    }

    /// `true` if the last seen commit in self and the provided one have the
    /// same `checksum`.
    fn same_checksum_as(&self, commit: &StoredCommit) -> bool {
        let Some(checksum) = self.checksum() else { return false };
        checksum == &commit.checksum
    }

    fn checksum(&self) -> Option<&u32> {
        match self {
            Self::Initial { .. } => None,
            Self::LastSeen { checksum, .. } => Some(checksum),
        }
    }

    fn expected_offset(&self) -> &u64 {
        match self {
            Self::Initial { next_offset } => next_offset,
            Self::LastSeen { tx_range, .. } => &tx_range.end,
        }
    }

    // If initial offset falls within a commit, adjust it to the commit boundary.
    //
    // Returns `true` if the initial offset is past `commit`.
    // Returns `false` if `self` isn't `Self::Initial`,
    // or the initial offset has been adjusted to the starting offset of `commit`.
    //
    // For iteration, `true` means to skip the commit, `false` to yield it.
    fn adjust_initial_offset(&mut self, commit: &StoredCommit) -> bool {
        if let Self::Initial { next_offset } = self {
            let last_tx_offset = commit.min_tx_offset + commit.n as u64 - 1;
            if *next_offset > last_tx_offset {
                return true;
            } else {
                *next_offset = commit.min_tx_offset;
            }
        }

        false
    }
}

pub struct Commits<R: Repo> {
    inner: Option<segment::Commits<R::SegmentReader>>,
    segments: Segments<R>,
    last_commit: CommitInfo,
    last_error: Option<error::Traversal>,
}

impl<R: Repo> Commits<R> {
    fn current_segment_header(&self) -> Option<&segment::Header> {
        self.inner.as_ref().map(|segment| &segment.header)
    }

    /// Turn `self` into an iterator which pairs the log format version of the
    /// current segment with the [`Commit`].
    pub fn with_log_format_version(self) -> CommitsWithVersion<R> {
        CommitsWithVersion { inner: self }
    }

    /// Advance the current-segment iterator to yield the next commit.
    ///
    /// Checks that the offset sequence is contiguous, and may skip commits
    /// until the requested offset.
    ///
    /// Returns `None` if the segment iterator is exhausted or returns an error.
    fn next_commit(&mut self) -> Option<Result<StoredCommit, error::Traversal>> {
        loop {
            match self.inner.as_mut()?.next()? {
                Ok(commit) => {
                    // Pop the last error. Either we'll return it below, or it's no longer
                    // interesting.
                    let prev_error = self.last_error.take();

                    // Skip entries before the initial commit.
                    if self.last_commit.adjust_initial_offset(&commit) {
                        trace!("adjust initial offset");
                        continue;
                    // Same offset: ignore if duplicate (same crc), else report a "fork".
                    } else if self.last_commit.same_offset_as(&commit) {
                        if !self.last_commit.same_checksum_as(&commit) {
                            warn!(
                                "forked: commit={:?} last-error={:?} last-crc={:?}",
                                commit,
                                prev_error,
                                self.last_commit.checksum()
                            );
                            return Some(Err(error::Traversal::Forked {
                                offset: commit.min_tx_offset,
                            }));
                        } else {
                            trace!("ignore duplicate");
                            continue;
                        }
                    // Not the expected offset: report out-of-order.
                    } else if self.last_commit.expected_offset() != &commit.min_tx_offset {
                        warn!("out-of-order: commit={commit:?} last-error={prev_error:?}");
                        return Some(Err(error::Traversal::OutOfOrder {
                            expected_offset: *self.last_commit.expected_offset(),
                            actual_offset: commit.min_tx_offset,
                            prev_error: prev_error.map(Box::new),
                        }));
                    // Seems legit, record info.
                    } else {
                        self.last_commit = CommitInfo::LastSeen {
                            tx_range: commit.tx_range(),
                            checksum: commit.checksum,
                        };

                        return Some(Ok(commit));
                    }
                }

                Err(e) => {
                    warn!("error reading next commit: {e}");
                    // Stop traversing this segment here.
                    //
                    // If this is just a partial write at the end of the segment,
                    // we may be able to obtain a commit with right offset from
                    // the next segment.
                    //
                    // If we don't, the error here is likely more helpful, but
                    // would be clobbered by `OutOfOrder`. Therefore we store it
                    // here.
                    self.set_last_error(e);

                    return None;
                }
            }
        }
    }

    /// Store `e` has the last error for delayed reporting.
    fn set_last_error(&mut self, e: io::Error) {
        // Recover a checksum mismatch.
        let last_error = if e.kind() == io::ErrorKind::InvalidData && e.get_ref().is_some() {
            e.into_inner()
                .unwrap()
                .downcast::<error::ChecksumMismatch>()
                .map(|source| error::Traversal::Checksum {
                    offset: *self.last_commit.expected_offset(),
                    source: *source,
                })
                .unwrap_or_else(|e| io::Error::new(io::ErrorKind::InvalidData, e).into())
        } else {
            error::Traversal::from(e)
        };
        self.last_error = Some(last_error);
    }

    /// If we're still looking for the initial commit, try to use the offset
    /// index to advance the segment reader.
    fn try_seek_to_initial_offset(&self, segment: &mut segment::Reader<R::SegmentReader>) {
        if let CommitInfo::Initial { next_offset } = &self.last_commit {
            try_seek_using_offset_index(&self.segments.repo, segment, *next_offset);
        }
    }
}

impl<R: Repo> Iterator for Commits<R> {
    type Item = Result<StoredCommit, error::Traversal>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.next_commit() {
            return Some(item);
        }

        match self.segments.next() {
            // When there is no more data, the last commit being bad is an error
            None => self.last_error.take().map(Err),
            Some(segment) => segment.map_or_else(
                |e| Some(Err(e.into())),
                |mut segment| {
                    self.try_seek_to_initial_offset(&mut segment);
                    self.inner = Some(segment.commits());
                    self.next()
                },
            ),
        }
    }
}

pub struct CommitsWithVersion<R: Repo> {
    inner: Commits<R>,
}

impl<R: Repo> Iterator for CommitsWithVersion<R> {
    type Item = Result<(u8, Commit), error::Traversal>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.inner.next()?;
        match next {
            Ok(commit) => {
                let version = self
                    .inner
                    .current_segment_header()
                    .map(|hdr| hdr.log_format_version)
                    .expect("segment header none even though segment yielded a commit");
                Some(Ok((version, commit.into())))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

/// Try to advance `reader` to `offset` using the offset index.
///
/// If successful, returns the offset index and the byte position of `reader`.
/// `None` if the position of `reader` is unchanged.
fn try_seek_using_offset_index<R: Repo>(
    repo: &R,
    reader: &mut segment::Reader<R::SegmentReader>,
    offset: u64,
) -> Option<(TxOffsetIndex, u64)> {
    let segment_offset = reader.min_tx_offset;
    let index = repo
        .get_offset_index(segment_offset)
        .inspect_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                debug!("offset index does not exist segment={segment_offset}");
            } else {
                warn!(
                    "error opening offset index segment={segment_offset}: {e} {}",
                    source_chain(&e)
                );
            }
        })
        .ok()?;

    reader
        .seek_to_offset(&index, offset)
        .inspect_err(|e| match e {
            // Can happen if the segment is empty or small, so don't spam the logs.
            IndexError::KeyNotFound => {
                debug!("offset not found segment={segment_offset} offset={offset}");
            }
            e => {
                warn!(
                    "error reading index segment={segment_offset} offset={offset}: {e} {}",
                    source_chain(&e)
                );
            }
        })
        .ok()
        .map(|pos| (index, pos))
}

// `range_bounds_is_empty` https://github.com/rust-lang/rust/issues/137300
//
// This is correct for integers, but unsound for arbitrary `T`, so unlikely to
// be stabilized.
fn range_is_empty(range: &impl RangeBounds<u64>) -> bool {
    use std::ops::Bound::*;

    #[rustfmt::skip]
    let not_empty = match (range.start_bound(), range.end_bound()) {
        (Unbounded, _) | (_, Unbounded) => true,
        (Included(start), Excluded(end))
        | (Excluded(start), Included(end))
        | (Excluded(start), Excluded(end)) => start < end,
        (Included(start), Included(end)) => start <= end,
    };

    !not_empty
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, iter::repeat};

    use pretty_assertions::assert_matches;

    use super::*;
    use crate::{
        payload::{ArrayDecodeError, ArrayDecoder},
        tests::helpers::{fill_log, mem_log},
    };

    #[test]
    fn rotate_segments_simple() {
        let mut log = mem_log::<[u8; 32]>(128);
        for _ in 0..3 {
            log.append([0; 32]).unwrap();
            log.commit().unwrap();
        }

        let offsets = log.repo.existing_offsets().unwrap();
        assert_eq!(&offsets[..offsets.len() - 1], &log.tail);
        assert_eq!(offsets[offsets.len() - 1], 2);
    }

    #[test]
    fn huge_commit() {
        let mut log = mem_log::<[u8; 32]>(32);

        log.append([0; 32]).unwrap();
        log.append([1; 32]).unwrap();
        log.commit().unwrap();
        assert!(log.head.len() > log.opts.max_segment_size);

        log.append([2; 32]).unwrap();
        log.commit().unwrap();

        assert_eq!(&log.tail, &[0]);
        assert_eq!(&log.repo.existing_offsets().unwrap(), &[0, 2]);
    }

    #[test]
    fn traverse_commits() {
        let mut log = mem_log::<[u8; 32]>(32);
        fill_log(&mut log, 10, repeat(1));

        for (i, commit) in (0..10).zip(log.commits_from(0)) {
            assert_eq!(i, commit.unwrap().min_tx_offset);
        }
    }

    #[test]
    fn traverse_commits_with_offset() {
        let mut log = mem_log::<[u8; 32]>(32);
        fill_log(&mut log, 10, repeat(1));

        for offset in 0..10 {
            for commit in log.commits_from(offset) {
                let commit = commit.unwrap();
                assert!(commit.min_tx_offset >= offset);
            }
        }
        assert_eq!(0, log.commits_from(10).count());
    }

    #[test]
    fn fold_transactions_with_offset() {
        let mut log = mem_log::<[u8; 32]>(32);
        fill_log(&mut log, 10, repeat(1));

        /// A [`Decoder`] which counts the number of records decoded,
        /// and asserts that the `tx_offset` is as expected.
        struct CountDecoder {
            count: Cell<u64>,
            next_tx_offset: Cell<u64>,
        }

        impl Decoder for &CountDecoder {
            type Record = [u8; 32];
            type Error = ArrayDecodeError;

            fn decode_record<'a, R: spacetimedb_sats::buffer::BufReader<'a>>(
                &self,
                _version: u8,
                _tx_offset: u64,
                _reader: &mut R,
            ) -> Result<Self::Record, Self::Error> {
                unreachable!("Folding never calls `decode_record`")
            }

            fn consume_record<'a, R: spacetimedb_sats::buffer::BufReader<'a>>(
                &self,
                version: u8,
                tx_offset: u64,
                reader: &mut R,
            ) -> Result<(), Self::Error> {
                let decoder = ArrayDecoder::<32>;
                decoder.consume_record(version, tx_offset, reader)?;
                self.count.set(self.count.get() + 1);
                let expected_tx_offset = self.next_tx_offset.get();
                assert_eq!(expected_tx_offset, tx_offset);
                self.next_tx_offset.set(expected_tx_offset + 1);
                Ok(())
            }

            fn skip_record<'a, R: spacetimedb_sats::buffer::BufReader<'a>>(
                &self,
                version: u8,
                tx_offset: u64,
                reader: &mut R,
            ) -> Result<(), Self::Error> {
                let decoder = ArrayDecoder::<32>;
                decoder.consume_record(version, tx_offset, reader)?;
                Ok(())
            }
        }

        for offset in 0..10 {
            let decoder = CountDecoder {
                count: Cell::new(0),
                next_tx_offset: Cell::new(offset),
            };

            log.fold_transactions_from(offset, &decoder).unwrap();

            assert_eq!(decoder.count.get(), 10 - offset);
            assert_eq!(decoder.next_tx_offset.get(), 10);
        }
    }

    #[test]
    fn traverse_commits_ignores_duplicates() {
        let mut log = mem_log::<[u8; 32]>(1024);

        log.append([42; 32]).unwrap();
        let commit1 = log.head.commit.clone();
        log.commit().unwrap();
        log.head.commit = commit1.clone();
        log.commit().unwrap();
        log.append([43; 32]).unwrap();
        let commit2 = log.head.commit.clone();
        log.commit().unwrap();

        assert_eq!(
            [commit1, commit2].as_slice(),
            &log.commits_from(0)
                .map_ok(Commit::from)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        );
    }

    #[test]
    fn traverse_commits_errors_when_forked() {
        let mut log = mem_log::<[u8; 32]>(1024);

        log.append([42; 32]).unwrap();
        log.commit().unwrap();
        log.head.commit = Commit {
            min_tx_offset: 0,
            n: 1,
            records: [43; 32].to_vec(),
            epoch: 0,
        };
        log.commit().unwrap();

        let res = log.commits_from(0).collect::<Result<Vec<_>, _>>();
        assert!(
            matches!(res, Err(error::Traversal::Forked { offset: 0 })),
            "expected fork error: {res:?}"
        )
    }

    #[test]
    fn traverse_commits_errors_when_offset_not_contiguous() {
        let mut log = mem_log::<[u8; 32]>(1024);

        log.append([42; 32]).unwrap();
        log.commit().unwrap();
        log.head.commit.min_tx_offset = 18;
        log.append([42; 32]).unwrap();
        log.commit().unwrap();

        let res = log.commits_from(0).collect::<Result<Vec<_>, _>>();
        assert!(
            matches!(
                res,
                Err(error::Traversal::OutOfOrder {
                    expected_offset: 1,
                    actual_offset: 18,
                    prev_error: None
                })
            ),
            "expected fork error: {res:?}"
        )
    }

    #[test]
    fn traverse_transactions() {
        let mut log = mem_log::<[u8; 32]>(32);
        let total_txs = fill_log(&mut log, 10, (1..=3).cycle()) as u64;

        for (i, tx) in (0..total_txs).zip(log.transactions_from(0, &ArrayDecoder)) {
            assert_eq!(i, tx.unwrap().offset);
        }
    }

    #[test]
    fn traverse_transactions_with_offset() {
        let mut log = mem_log::<[u8; 32]>(32);
        let total_txs = fill_log(&mut log, 10, (1..=3).cycle()) as u64;

        for offset in 0..total_txs {
            let mut iter = log.transactions_from(offset, &ArrayDecoder);
            assert_eq!(offset, iter.next().expect("at least one tx expected").unwrap().offset);
            for tx in iter {
                assert!(tx.unwrap().offset >= offset);
            }
        }
        assert_eq!(0, log.transactions_from(total_txs, &ArrayDecoder).count());
    }

    #[test]
    fn traverse_empty() {
        let log = mem_log::<[u8; 32]>(32);

        assert_eq!(0, log.commits_from(0).count());
        assert_eq!(0, log.commits_from(42).count());
        assert_eq!(0, log.transactions_from(0, &ArrayDecoder).count());
        assert_eq!(0, log.transactions_from(42, &ArrayDecoder).count());
    }

    #[test]
    fn reset_hard() {
        let mut log = mem_log::<[u8; 32]>(128);
        fill_log(&mut log, 50, (1..=10).cycle());

        log = log.reset().unwrap();
        assert_eq!(0, log.transactions_from(0, &ArrayDecoder).count());
    }

    #[test]
    fn reset_to_offset() {
        let mut log = mem_log::<[u8; 32]>(128);
        let total_txs = fill_log(&mut log, 50, repeat(1)) as u64;

        for offset in (0..total_txs).rev() {
            log = log.reset_to(offset).unwrap();
            assert_eq!(
                offset,
                log.transactions_from(0, &ArrayDecoder)
                    .map(Result::unwrap)
                    .last()
                    .unwrap()
                    .offset
            );
            // We're counting from zero, so offset + 1 is the # of txs.
            assert_eq!(
                offset + 1,
                log.transactions_from(0, &ArrayDecoder).map(Result::unwrap).count() as u64
            );
        }
    }

    #[test]
    fn reset_to_offset_many_txs_per_commit() {
        let mut log = mem_log::<[u8; 32]>(128);
        let total_txs = fill_log(&mut log, 50, (1..=10).cycle()) as u64;

        // No op.
        log = log.reset_to(total_txs).unwrap();
        assert_eq!(total_txs, log.transactions_from(0, &ArrayDecoder).count() as u64);

        let middle_commit = log.commits_from(0).nth(25).unwrap().unwrap();

        // Both fall into the middle commit, which should be retained.
        log = log.reset_to(middle_commit.min_tx_offset + 1).unwrap();
        assert_eq!(
            middle_commit.tx_range().end,
            log.transactions_from(0, &ArrayDecoder).count() as u64
        );
        log = log.reset_to(middle_commit.min_tx_offset).unwrap();
        assert_eq!(
            middle_commit.tx_range().end,
            log.transactions_from(0, &ArrayDecoder).count() as u64
        );

        // Offset falls into 2nd commit.
        // 1st commit (1 tx) + 2nd commit (2 txs) = 3
        log = log.reset_to(1).unwrap();
        assert_eq!(3, log.transactions_from(0, &ArrayDecoder).count() as u64);

        // Offset falls into 1st commit.
        // 1st commit (1 tx) = 1
        log = log.reset_to(0).unwrap();
        assert_eq!(1, log.transactions_from(0, &ArrayDecoder).count() as u64);
    }

    #[test]
    fn reopen() {
        let mut log = mem_log::<[u8; 32]>(1024);
        let mut total_txs = fill_log(&mut log, 100, (1..=10).cycle());
        assert_eq!(
            total_txs,
            log.transactions_from(0, &ArrayDecoder).map(Result::unwrap).count()
        );

        let mut log = Generic::<_, [u8; 32]>::open(
            log.repo.clone(),
            Options {
                max_segment_size: 1024,
                ..Options::default()
            },
        )
        .unwrap();
        total_txs += fill_log(&mut log, 100, (1..=10).cycle());

        assert_eq!(
            total_txs,
            log.transactions_from(0, &ArrayDecoder).map(Result::unwrap).count()
        );
    }

    #[test]
    fn set_same_epoch_does_nothing() {
        let mut log = Generic::<_, [u8; 32]>::open(repo::Memory::new(), <_>::default()).unwrap();
        assert_eq!(log.epoch(), Commit::DEFAULT_EPOCH);
        let committed = log.set_epoch(Commit::DEFAULT_EPOCH).unwrap();
        assert_eq!(committed, None);
    }

    #[test]
    fn set_new_epoch_commits() {
        let mut log = Generic::<_, [u8; 32]>::open(repo::Memory::new(), <_>::default()).unwrap();
        assert_eq!(log.epoch(), Commit::DEFAULT_EPOCH);
        log.append(<_>::default()).unwrap();
        let committed = log
            .set_epoch(42)
            .unwrap()
            .expect("should have committed the pending transaction");
        assert_eq!(log.epoch(), 42);
        assert_eq!(committed.tx_range.start, 0);
    }

    #[test]
    fn set_lower_epoch_returns_error() {
        let mut log = Generic::<_, [u8; 32]>::open(repo::Memory::new(), <_>::default()).unwrap();
        log.set_epoch(42).unwrap();
        assert_eq!(log.epoch(), 42);
        assert_matches!(log.set_epoch(7), Err(e) if e.kind() == io::ErrorKind::InvalidInput)
    }
}
