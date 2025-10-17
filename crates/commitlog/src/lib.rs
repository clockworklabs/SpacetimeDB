use std::{
    io,
    num::{NonZeroU16, NonZeroU64},
    ops::RangeBounds,
    sync::{Arc, RwLock},
};

use log::trace;
use repo::{fs::OnNewSegmentFn, Repo};
use spacetimedb_paths::server::CommitLogDir;

pub mod commit;
pub mod commitlog;
mod index;
pub mod repo;
pub mod segment;
mod varchar;
mod varint;

pub use crate::{
    commit::{Commit, StoredCommit},
    payload::{Decoder, Encode},
    segment::{Transaction, DEFAULT_LOG_FORMAT_VERSION},
    varchar::Varchar,
};
pub mod error;
pub mod payload;

#[cfg(feature = "streaming")]
pub mod stream;

#[cfg(any(test, feature = "test"))]
pub mod tests;

/// [`Commitlog`] options.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
pub struct Options {
    /// Set the log format version to write, and the maximum supported version.
    ///
    /// Choosing a payload format `T` of [`Commitlog`] should usually result in
    /// updating the [`DEFAULT_LOG_FORMAT_VERSION`] of this crate. Sometimes it
    /// may however be useful to set the version at runtime, e.g. to experiment
    /// with new or very old versions.
    ///
    /// Default: [`DEFAULT_LOG_FORMAT_VERSION`]
    #[cfg_attr(feature = "serde", serde(default = "Options::default_log_format_version"))]
    pub log_format_version: u8,
    /// The maximum size in bytes to which log segments should be allowed to
    /// grow.
    ///
    /// Default: 1GiB
    #[cfg_attr(feature = "serde", serde(default = "Options::default_max_segment_size"))]
    pub max_segment_size: u64,
    /// The maximum number of records in a commit.
    ///
    /// If this number is exceeded, the commit is flushed to disk even without
    /// explicitly calling [`Commitlog::flush`].
    ///
    /// Default: 65,535
    #[cfg_attr(feature = "serde", serde(default = "Options::default_max_records_in_commit"))]
    pub max_records_in_commit: NonZeroU16,
    /// Whenever at least this many bytes have been written to the currently
    /// active segment, an entry is added to its offset index.
    ///
    /// Default: 4096
    #[cfg_attr(feature = "serde", serde(default = "Options::default_offset_index_interval_bytes"))]
    pub offset_index_interval_bytes: NonZeroU64,
    /// If `true`, require that the segment must be synced to disk before an
    /// index entry is added.
    ///
    /// Setting this to `false` (the default) will update the index every
    /// `offset_index_interval_bytes`, even if the commitlog wasn't synced.
    /// This means that the index could contain non-existent entries in the
    /// event of a crash.
    ///
    /// Setting it to `true` will update the index when the commitlog is synced,
    /// and `offset_index_interval_bytes` have been written.
    /// This means that the index could contain fewer index entries than
    /// strictly every `offset_index_interval_bytes`.
    ///
    /// Default: false
    #[cfg_attr(
        feature = "serde",
        serde(default = "Options::default_offset_index_require_segment_fsync")
    )]
    pub offset_index_require_segment_fsync: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Options {
    pub const DEFAULT_MAX_SEGMENT_SIZE: u64 = 1024 * 1024 * 1024;
    pub const DEFAULT_MAX_RECORDS_IN_COMMIT: NonZeroU16 = NonZeroU16::MAX;
    pub const DEFAULT_OFFSET_INDEX_INTERVAL_BYTES: NonZeroU64 = NonZeroU64::new(4096).expect("4096 > 0, qed");
    pub const DEFAULT_OFFSET_INDEX_REQUIRE_SEGMENT_FSYNC: bool = false;

    pub const DEFAULT: Self = Self {
        log_format_version: DEFAULT_LOG_FORMAT_VERSION,
        max_segment_size: Self::default_max_segment_size(),
        max_records_in_commit: Self::default_max_records_in_commit(),
        offset_index_interval_bytes: Self::default_offset_index_interval_bytes(),
        offset_index_require_segment_fsync: Self::default_offset_index_require_segment_fsync(),
    };

    pub const fn default_log_format_version() -> u8 {
        DEFAULT_LOG_FORMAT_VERSION
    }

    pub const fn default_max_segment_size() -> u64 {
        Self::DEFAULT_MAX_SEGMENT_SIZE
    }

    pub const fn default_max_records_in_commit() -> NonZeroU16 {
        Self::DEFAULT_MAX_RECORDS_IN_COMMIT
    }

    pub const fn default_offset_index_interval_bytes() -> NonZeroU64 {
        Self::DEFAULT_OFFSET_INDEX_INTERVAL_BYTES
    }

    pub const fn default_offset_index_require_segment_fsync() -> bool {
        Self::DEFAULT_OFFSET_INDEX_REQUIRE_SEGMENT_FSYNC
    }

    /// Compute the length in bytes of an offset index based on the settings in
    /// `self`.
    pub fn offset_index_len(&self) -> u64 {
        self.max_segment_size / self.offset_index_interval_bytes
    }
}

/// The canonical commitlog, backed by on-disk log files.
///
/// Records in the log are of type `T`, which canonically is instantiated to
/// [`payload::Txdata`].
pub struct Commitlog<T> {
    inner: RwLock<commitlog::Generic<repo::Fs, T>>,
}

impl<T> Commitlog<T> {
    /// Open the log at root directory `root` with [`Options`].
    ///
    /// The root directory must already exist.
    ///
    /// Note that opening a commitlog involves I/O: some consistency checks are
    /// performed, and the next writing position is determined.
    ///
    /// This is only necessary when opening the commitlog for writing. See the
    /// free-standing functions in this module for how to traverse a read-only
    /// commitlog.
    pub fn open(root: CommitLogDir, opts: Options, on_new_segment: Option<Arc<OnNewSegmentFn>>) -> io::Result<Self> {
        let inner = commitlog::Generic::open(repo::Fs::new(root, on_new_segment)?, opts)?;

        Ok(Self {
            inner: RwLock::new(inner),
        })
    }

    /// Determine the maximum transaction offset considered durable.
    ///
    /// The offset is `None` if the log hasn't been flushed to disk yet.
    pub fn max_committed_offset(&self) -> Option<u64> {
        self.inner.read().unwrap().max_committed_offset()
    }

    /// Determine the minimum transaction offset in the log.
    ///
    /// The offset is `None` if the log hasn't been flushed to disk yet.
    pub fn min_committed_offset(&self) -> Option<u64> {
        self.inner.read().unwrap().min_committed_offset()
    }

    /// Get the current epoch.
    ///
    /// See also: [`Commit::epoch`].
    pub fn epoch(&self) -> u64 {
        self.inner.read().unwrap().epoch()
    }

    /// Update the current epoch.
    ///
    /// Does nothing if the given `epoch` is equal to the current epoch.
    /// Otherwise flushes outstanding transactions to disk (equivalent to
    /// [`Self::flush`]) before updating the epoch.
    ///
    /// Returns the maximum transaction offset written to disk. The offset is
    /// `None` if the log is empty and no data was pending to be flushed.
    ///
    /// # Errors
    ///
    /// If `epoch` is smaller than the current epoch, an error of kind
    /// [`io::ErrorKind::InvalidInput`] is returned.
    ///
    /// Errors from the implicit flush are propagated.
    pub fn set_epoch(&self, epoch: u64) -> io::Result<Option<u64>> {
        let mut inner = self.inner.write().unwrap();
        inner.set_epoch(epoch)?;

        Ok(inner.max_committed_offset())
    }

    /// Sync all OS-buffered writes to disk.
    ///
    /// Note that this does **not** write outstanding records to disk.
    /// Use [`Self::flush_and_sync`] or call [`Self::flush`] prior to this
    /// method to ensure all data is on disk.
    ///
    /// Returns the maximum transaction offset which is considered durable after
    /// this method returns successfully. The offset is `None` if the log hasn't
    /// been flushed to disk yet.
    ///
    /// # Panics
    ///
    /// This method panics if syncing fails irrecoverably.
    pub fn sync(&self) -> Option<u64> {
        let mut inner = self.inner.write().unwrap();
        trace!("sync commitlog");
        inner.sync();

        inner.max_committed_offset()
    }

    /// Write all outstanding transaction records to disk.
    ///
    /// Note that this does **not** force the OS to sync the data to disk.
    /// Use [`Self::flush_and_sync`] or call [`Self::sync`] after this method
    /// to ensure all data is on disk.
    ///
    /// Returns the maximum transaction offset written to disk. The offset is
    /// `None` if the log is empty and no data was pending to be flushed.
    ///
    /// Repeatedly calling this method may return the same value.
    pub fn flush(&self) -> io::Result<Option<u64>> {
        let mut inner = self.inner.write().unwrap();
        trace!("flush commitlog");
        inner.commit()?;

        Ok(inner.max_committed_offset())
    }

    /// Write all outstanding transaction records to disk and flush OS buffers.
    ///
    /// Equivalent to calling [`Self::flush`] followed by [`Self::sync`], but
    /// without releasing the write lock in between.
    ///
    /// # Errors
    ///
    /// An error is returned if writing to disk fails due to an I/O error.
    ///
    /// # Panics
    ///
    /// This method panics if syncing fails irrecoverably.
    pub fn flush_and_sync(&self) -> io::Result<Option<u64>> {
        let mut inner = self.inner.write().unwrap();
        trace!("flush and sync commitlog");
        inner.commit()?;
        inner.sync();

        Ok(inner.max_committed_offset())
    }

    /// Obtain an iterator which traverses the log from the start, yielding
    /// [`StoredCommit`]s.
    ///
    /// The returned iterator is not aware of segment rotation. That is, if a
    /// new segment is created after this method returns, the iterator will not
    /// traverse it.
    ///
    /// Commits appended to the log while it is being traversed are generally
    /// visible to the iterator. Upon encountering [`io::ErrorKind::UnexpectedEof`],
    /// however, a new iterator should be created using [`Self::commits_from`]
    /// with the last transaction offset yielded.
    ///
    /// Note that the very last [`StoredCommit`] in a commitlog may be corrupt
    /// (e.g. due to a partial write to disk), but a subsequent `append` will
    /// bring the log into a consistent state.
    ///
    /// This means that, when this iterator yields an `Err` value, the consumer
    /// may want to check if the iterator is exhausted (by calling `next()`)
    /// before treating the `Err` value as an application error.
    pub fn commits(&self) -> impl Iterator<Item = Result<StoredCommit, error::Traversal>> {
        self.commits_from(0)
    }

    /// Obtain an iterator starting from transaction offset `offset`, yielding
    /// [`StoredCommit`]s.
    ///
    /// Similar to [`Self::commits`] but will skip until the offset is contained
    /// in the next [`StoredCommit`] to yield.
    ///
    /// Note that the first [`StoredCommit`] yielded is the first commit
    /// containing the given transaction offset, i.e. its `min_tx_offset` may be
    /// smaller than `offset`.
    pub fn commits_from(&self, offset: u64) -> impl Iterator<Item = Result<StoredCommit, error::Traversal>> {
        self.inner.read().unwrap().commits_from(offset)
    }

    /// Get a list of segment offsets, sorted in ascending order.
    pub fn existing_segment_offsets(&self) -> io::Result<Vec<u64>> {
        self.inner.read().unwrap().repo.existing_offsets()
    }

    /// Compress the segments at the offsets provided, marking them as immutable.
    pub fn compress_segments(&self, offsets: &[u64]) -> io::Result<()> {
        // even though `compress_segment` takes &self, we take an
        // exclusive lock to avoid any weirdness happening.
        #[allow(clippy::readonly_write_lock)]
        let inner = self.inner.write().unwrap();
        assert!(!offsets.contains(&inner.head.min_tx_offset()));
        // TODO: parallelize, maybe
        offsets
            .iter()
            .try_for_each(|&offset| inner.repo.compress_segment(offset))
    }

    /// Remove all data from the log and reopen it.
    ///
    /// Log segments are deleted starting from the newest. As multiple segments
    /// cannot be deleted atomically, the log may not be completely empty if
    /// the method returns an error.
    ///
    /// Note that the method consumes `self` to ensure the log is not modified
    /// while resetting.
    pub fn reset(self) -> io::Result<Self> {
        let inner = self.inner.into_inner().unwrap().reset()?;
        Ok(Self {
            inner: RwLock::new(inner),
        })
    }

    /// Remove all data past the given transaction `offset` from the log and
    /// reopen it.
    ///
    /// Like with [`Self::reset`], it may happen that not all segments newer
    /// than `offset` can be deleted.
    ///
    /// If the method returns successfully, the most recent [`Commit`] in the
    /// log will contain the transaction at `offset`.
    ///
    /// Note that the method consumes `self` to ensure the log is not modified
    /// while resetting.
    pub fn reset_to(self, offset: u64) -> io::Result<Self> {
        let inner = self.inner.into_inner().unwrap().reset_to(offset)?;
        Ok(Self {
            inner: RwLock::new(inner),
        })
    }

    /// Determine the size on disk of this commitlog.
    pub fn size_on_disk(&self) -> io::Result<u64> {
        let inner = self.inner.read().unwrap();
        inner.repo.size_on_disk()
    }
}

impl<T: Encode> Commitlog<T> {
    /// Append the record `txdata` to the log.
    ///
    /// If the internal buffer exceeds [`Options::max_records_in_commit`], the
    /// argument is returned in an `Err`. The caller should [`Self::flush`] the
    /// log and try again.
    ///
    /// In case the log is appended to from multiple threads, this may result in
    /// a busy loop trying to acquire a slot in the buffer. In such scenarios,
    /// [`Self::append_maybe_flush`] is preferable.
    pub fn append(&self, txdata: T) -> Result<(), T> {
        let mut inner = self.inner.write().unwrap();
        inner.append(txdata)
    }

    /// Append the record `txdata` to the log.
    ///
    /// The `txdata` payload is buffered in memory until either:
    ///
    /// - [`Self::flush`] is called explicitly, or
    /// - [`Options::max_records_in_commit`] is exceeded
    ///
    /// In the latter case, [`Self::append`] flushes implicitly, _before_
    /// appending the `txdata` argument.
    ///
    /// I.e. the argument is not guaranteed to be flushed after the method
    /// returns. If that is desired, [`Self::flush`] must be called explicitly.
    ///
    /// If writing `txdata` to the commitlog results in a new segment file being opened,
    /// we will send a message down `on_new_segment`.
    /// This will be hooked up to the `request_snapshot` channel of a `SnapshotWorker`.
    ///
    /// # Errors
    ///
    /// If the log needs to be flushed, but an I/O error occurs, ownership of
    /// `txdata` is returned back to the caller alongside the [`io::Error`].
    ///
    /// The value can then be used to retry appending.
    pub fn append_maybe_flush(&self, txdata: T) -> Result<(), error::Append<T>> {
        let mut inner = self.inner.write().unwrap();

        if let Err(txdata) = inner.append(txdata) {
            if let Err(source) = inner.commit() {
                return Err(error::Append { txdata, source });
            }

            // `inner.commit.n` must be zero at this point
            let res = inner.append(txdata);
            debug_assert!(res.is_ok(), "failed to append while holding write lock");
        }

        Ok(())
    }

    /// Obtain an iterator which traverses the log from the start, yielding
    /// [`Transaction`]s.
    ///
    /// The provided `decoder`'s [`Decoder::decode_record`] method will be
    /// called [`Commit::n`] times per [`Commit`] to obtain the individual
    /// transaction payloads.
    ///
    /// Like [`Self::commits`], the iterator is not aware of segment rotation.
    /// That is, if a new segment is created after this method returns, the
    /// iterator will not traverse it.
    ///
    /// Transactions appended to the log while it is being traversed are
    /// generally visible to the iterator. Upon encountering [`io::ErrorKind::UnexpectedEof`],
    /// however, a new iterator should be created using [`Self::transactions_from`]
    /// with the last transaction offset yielded.
    ///
    /// Note that the very last [`Commit`] in a commitlog may be corrupt (e.g.
    /// due to a partial write to disk), but a subsequent `append` will bring
    /// the log into a consistent state.
    ///
    /// This means that, when this iterator yields an `Err` value, the consumer
    /// may want to check if the iterator is exhausted (by calling `next()`)
    /// before treating the `Err` value as an application error.
    pub fn transactions<'a, D>(&self, de: &'a D) -> impl Iterator<Item = Result<Transaction<T>, D::Error>> + 'a
    where
        D: Decoder<Record = T>,
        D::Error: From<error::Traversal>,
        T: 'a,
    {
        self.transactions_from(0, de)
    }

    /// Obtain an iterator starting from transaction offset `offset`, yielding
    /// [`Transaction`]s.
    ///
    /// Similar to [`Self::transactions`] but will skip until the provided
    /// `offset`, i.e. the first [`Transaction`] yielded will be the transaction
    /// with offset `offset`.
    pub fn transactions_from<'a, D>(
        &self,
        offset: u64,
        de: &'a D,
    ) -> impl Iterator<Item = Result<Transaction<T>, D::Error>> + 'a
    where
        D: Decoder<Record = T>,
        D::Error: From<error::Traversal>,
        T: 'a,
    {
        self.inner.read().unwrap().transactions_from(offset, de)
    }

    /// Traverse the log from the start and "fold" its transactions into the
    /// provided [`Decoder`].
    ///
    /// A [`Decoder`] is a stateful object due to the requirement to store
    /// schema information in the log itself. That is, a [`Decoder`] may need to
    /// be able to resolve transaction schema information dynamically while
    /// traversing the log.
    ///
    /// This is equivalent to "replaying" a log into a database state. In this
    /// scenario, it is not interesting to consume the [`Transaction`] payload
    /// as an iterator.
    ///
    /// This method allows the use of a [`Decoder`] which returns zero-sized
    /// data (e.g. `Decoder<Record = ()>`), as it will not allocate the commit
    /// payload into a struct.
    ///
    /// Note that, unlike [`Self::transactions`], this method will ignore a
    /// corrupt commit at the very end of the traversed log.
    pub fn fold_transactions<D>(&self, de: D) -> Result<(), D::Error>
    where
        D: Decoder,
        D::Error: From<error::Traversal>,
    {
        self.fold_transactions_from(0, de)
    }

    /// Traverse the log from the given transaction offset and "fold" its
    /// transactions into the provided [`Decoder`].
    ///
    /// Similar to [`Self::fold_transactions`] but will skip until the provided
    /// `offset`, i.e. the first `tx_offset` passed to [`Decoder::decode_record`]
    /// will be equal to `offset`.
    pub fn fold_transactions_from<D>(&self, offset: u64, de: D) -> Result<(), D::Error>
    where
        D: Decoder,
        D::Error: From<error::Traversal>,
    {
        self.inner.read().unwrap().fold_transactions_from(offset, de)
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
pub fn committed_meta(root: CommitLogDir) -> Result<Option<segment::Metadata>, error::SegmentMetadata> {
    commitlog::committed_meta(repo::Fs::new(root, None)?)
}

/// Obtain an iterator which traverses the commitlog located at the `root`
/// directory from the start, yielding [`StoredCommit`]s.
///
/// Starts the traversal without the upfront I/O imposed by [`Commitlog::open`].
/// See [`Commitlog::commits`] for more information.
pub fn commits(root: CommitLogDir) -> io::Result<impl Iterator<Item = Result<StoredCommit, error::Traversal>>> {
    commits_from(root, 0)
}

/// Obtain an iterator which traverses the commitlog located at the `root`
/// directory starting from `offset` and yielding [`StoredCommit`]s.
///
/// Starts the traversal without the upfront I/O imposed by [`Commitlog::open`].
/// See [`Commitlog::commits_from`] for more information.
pub fn commits_from(
    root: CommitLogDir,
    offset: u64,
) -> io::Result<impl Iterator<Item = Result<StoredCommit, error::Traversal>>> {
    commitlog::commits_from(repo::Fs::new(root, None)?, DEFAULT_LOG_FORMAT_VERSION, offset)
}

/// Obtain an iterator which traverses the commitlog located at the `root`
/// directory from the start, yielding [`Transaction`]s.
///
/// Starts the traversal without the upfront I/O imposed by [`Commitlog::open`].
/// See [`Commitlog::transactions`] for more information.
pub fn transactions<'a, D, T>(
    root: CommitLogDir,
    de: &'a D,
) -> io::Result<impl Iterator<Item = Result<Transaction<T>, D::Error>> + 'a>
where
    D: Decoder<Record = T>,
    D::Error: From<error::Traversal>,
    T: 'a,
{
    transactions_from(root, 0, de)
}

/// Obtain an iterator which traverses the commitlog located at the `root`
/// directory starting from `offset` and yielding [`Transaction`]s.
///
/// Starts the traversal without the upfront I/O imposed by [`Commitlog::open`].
/// See [`Commitlog::transactions_from`] for more information.
pub fn transactions_from<'a, D, T>(
    root: CommitLogDir,
    offset: u64,
    de: &'a D,
) -> io::Result<impl Iterator<Item = Result<Transaction<T>, D::Error>> + 'a>
where
    D: Decoder<Record = T>,
    D::Error: From<error::Traversal>,
    T: 'a,
{
    commitlog::transactions_from(repo::Fs::new(root, None)?, DEFAULT_LOG_FORMAT_VERSION, offset, de)
}

/// Traverse the commitlog located at the `root` directory from the start and
/// "fold" its transactions into the provided [`Decoder`].
///
/// Starts the traversal without the upfront I/O imposed by [`Commitlog::open`].
/// See [`Commitlog::fold_transactions`] for more information.
pub fn fold_transactions<D>(root: CommitLogDir, de: D) -> Result<(), D::Error>
where
    D: Decoder,
    D::Error: From<error::Traversal> + From<io::Error>,
{
    fold_transactions_from(root, 0, de)
}

/// Traverse the commitlog located at the `root` directory starting from `offset`
/// and "fold" its transactions into the provided [`Decoder`].
///
/// Starts the traversal without the upfront I/O imposed by [`Commitlog::open`].
/// See [`Commitlog::fold_transactions_from`] for more information.
pub fn fold_transactions_from<D>(root: CommitLogDir, offset: u64, de: D) -> Result<(), D::Error>
where
    D: Decoder,
    D::Error: From<error::Traversal> + From<io::Error>,
{
    commitlog::fold_transactions_from(repo::Fs::new(root, None)?, DEFAULT_LOG_FORMAT_VERSION, offset, de)
}

pub fn fold_transaction_range<D>(root: CommitLogDir, range: impl RangeBounds<u64>, de: D) -> Result<(), D::Error>
where
    D: Decoder,
    D::Error: From<error::Traversal> + From<io::Error>,
{
    commitlog::fold_transaction_range(repo::Fs::new(root, None)?, DEFAULT_LOG_FORMAT_VERSION, range, de)
}
