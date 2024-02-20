use super::{
    datastore::traits::{MutTxDatastore, TxData},
    message_log::{self, MessageLog},
    messages::commit::Commit,
    ostorage::{memory_object_db::MemoryObjectDB, ObjectDB},
    FsyncPolicy,
};
use crate::{
    db::{
        datastore::traits::TxOp,
        db_metrics::DB_METRICS,
        messages::{
            transaction::Transaction,
            write::{Operation, Write},
        },
    },
    error::{DBError, LogReplayError},
    execution_context::ExecutionContext,
};
use anyhow::Context;
use spacetimedb_sats::hash::{hash_bytes, Hash};
use spacetimedb_sats::DataKey;
use spacetimedb_table::indexes::RowPointer;
use std::{
    collections::{hash_map, HashMap},
    io,
    sync::{Arc, Mutex, MutexGuard},
};

/// A read-only handle to the commit log.
#[derive(Clone)]
pub struct CommitLog {
    mlog: Arc<Mutex<MessageLog>>,
    odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
}

impl CommitLog {
    pub const fn new(mlog: Arc<Mutex<MessageLog>>, odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>) -> Self {
        Self { mlog, odb }
    }

    pub fn max_commit_offset(&self) -> u64 {
        self.mlog.lock().unwrap().open_segment_max_offset
    }

    /// Obtain a [`CommitLogMut`], which permits write access.
    ///
    /// Like [`Self::replay`], this traverses the log from the start and ensures
    /// the resulting [`CommitLogMut`] can safely be written to.
    ///
    /// Equivalent to `self.replay(|_, _| Ok(()))`.
    pub fn to_mut(&self) -> Result<CommitLogMut, DBError> {
        self.replay(|_, _| Ok(()))
    }

    /// Traverse the log from the start, calling `F` with each [`Commit`]
    /// encountered.
    ///
    /// The second parameter to `F` is an [`ObjectDB`] which is guaranteed to
    /// contain all non-inline objects referenced from the corresponding commit.
    /// If a commit is encountered for which an object cannot be resolved from
    /// the [`CommitLog`]s underlying object storage, `replay` aborts and `F` is
    /// not called.
    ///
    /// The traversal performs some consistency checks, and _may_ perform error
    /// correction on the persistent log before returning.
    ///
    /// **NOTE**: Error correction modifies the on-disk state and may thus
    /// interfere with concurrent readers. External synchronization is required
    /// to avoid this.
    ///
    /// Currently, this method is the only way to ensure the log is consistent,
    /// and can thus safely be written to via the resulting [`CommitLogMut`].
    pub fn replay<F>(&self, mut f: F) -> Result<CommitLogMut, DBError>
    where
        F: FnMut(Commit, &dyn ObjectDB) -> Result<(), DBError>,
    {
        let unwritten_commit = {
            let mut mlog = self.mlog.lock().unwrap();
            let total_segments = mlog.total_segments();
            let segments = mlog.segments();
            let mut iter = Replay {
                tx_offset: 0,
                last_commit_offset: None,
                last_hash: None,

                odb: self.odb.clone(),

                segments,
                segment_offset: 0,
                current_segment: None,
            };

            for commit in &mut iter {
                match commit {
                    Ok((commit, objects)) => f(commit, &objects)?,
                    Err(ReplayError::Other { source }) => return Err(source.into()),
                    // Note that currently the commit offset is just a u64.
                    // It does not have any additional structure or semantic validation.
                    // Hence an OutOfOrder error does not imply an out of order write.
                    // The commit itself may not even be valid.
                    //
                    // However until we improve the on-disk format,
                    // We should not ignore such errors and truncate the log.
                    // They could still point to a serious bug in the database,
                    // such as not properly synchronizing access to the commit log,
                    // among other things.
                    Err(ReplayError::OutOfOrder {
                        segment_offset,
                        last_commit_offset,
                        decoded_commit_offset,
                        expected,
                    }) => {
                        log::warn!("Out-of-order commit {}, expected {}", decoded_commit_offset, expected);
                        return Err(LogReplayError::OutOfOrderCommit {
                            commit_offset: decoded_commit_offset,
                            segment_offset,
                            last_commit_offset,
                        }
                        .into());
                    }
                    // We expect that partial writes can occur at the end of a
                    // segment. Trimming the log is, however, only safe if we're
                    // at the end of the _log_.
                    Err(ReplayError::CorruptedData {
                        segment_offset,
                        last_commit_offset: commit_offset,
                        source,
                    }) if segment_offset < total_segments - 1 => {
                        log::warn!("Corrupt commit after offset {}", commit_offset);
                        return Err(LogReplayError::TrailingSegments {
                            segment_offset,
                            total_segments,
                            commit_offset,
                            source,
                        }
                        .into());
                    }
                    Err(ReplayError::MissingObject {
                        segment_offset,
                        last_commit_offset,
                        hash,
                        referenced_from_commit_offset,
                    }) if segment_offset < total_segments - 1 => {
                        log::warn!(
                            "Missing object {} referenced from {}",
                            hash,
                            referenced_from_commit_offset
                        );
                        return Err(LogReplayError::TrailingSegments {
                            segment_offset,
                            total_segments,
                            commit_offset: last_commit_offset,
                            source: io::Error::new(io::ErrorKind::Other, "Missing object"),
                        }
                        .into());
                    }

                    // We are near the end of the log, so trim it to the known-
                    // good prefix.
                    Err(
                        ReplayError::CorruptedData { last_commit_offset, .. }
                        | ReplayError::MissingObject { last_commit_offset, .. },
                    ) => {
                        mlog.reset_to(last_commit_offset)
                            .map_err(|source| LogReplayError::Reset {
                                offset: last_commit_offset,
                                source,
                            })?;
                        break;
                    }
                }
            }

            Commit {
                parent_commit_hash: iter.last_hash,
                commit_offset: iter.last_commit_offset.map(|off| off + 1).unwrap_or_default(),
                min_tx_offset: iter.tx_offset,
                transactions: Vec::new(),
            }
        };

        Ok(CommitLogMut {
            mlog: self.mlog.clone(),
            odb: self.odb.clone(),
            unwritten_commit: Arc::new(Mutex::new(unwritten_commit)),
            fsync: FsyncPolicy::Never,
        })
    }

    /// The number of bytes on disk occupied by the [MessageLog].
    pub fn message_log_size_on_disk(&self) -> u64 {
        let guard = self.mlog.lock().unwrap();
        guard.size()
    }

    /// The number of bytes on disk occupied by the [ObjectDB].
    #[tracing::instrument(skip(self), err)]
    pub fn object_db_size_on_disk(&self) -> Result<u64, DBError> {
        let guard = self.odb.lock().unwrap();
        guard.size_on_disk()
    }

    /// Obtain an iterator over a snapshot of the raw message log segments.
    ///
    /// See also: [`MessageLog::segments`]
    pub fn message_log_segments(&self) -> message_log::Segments {
        self.message_log_segments_from(0)
    }

    /// Obtain an iterator over a snapshot of the raw message log segments
    /// containing messages equal to or newer than `offset`.
    ///
    /// See [`MessageLog::segments_from`] for more information.
    pub fn message_log_segments_from(&self, offset: u64) -> message_log::Segments {
        let mlog = self.mlog.lock().unwrap();
        mlog.segments_from(offset)
    }

    /// Obtain an iterator over the [`Commit`]s in the log.
    ///
    /// The iterator represents a snapshot of the log.
    pub fn iter(&self) -> Iter {
        self.iter_from(0)
    }

    /// Obtain an iterator over the [`Commit`]s in the log, starting at `offset`.
    ///
    /// The iterator represents a snapshot of the log.
    ///
    /// Note that [`Commit`]s with an offset _smaller_ than `offset` may be
    /// yielded if the offset doesn't fall on a segment boundary, due to the
    /// lack of slicing support.
    ///
    /// See [`MessageLog::segments_from`] for more information.
    pub fn iter_from(&self, offset: u64) -> Iter {
        self.message_log_segments_from(offset).into()
    }

    /// Obtain an iterator over the large objects in [`Commit`], if any.
    ///
    /// Large objects are stored in the [`ObjectDB`], and are referenced from
    /// the transactions in a [`Commit`].
    ///
    /// The iterator attempts to read each large object in turn, yielding an
    /// [`io::Error`] with kind [`io::ErrorKind::NotFound`] if the object was
    /// not found.
    //
    // TODO(kim): We probably want a more efficient way to stream the contents
    // of the ODB over the network for replication purposes.
    pub fn commit_objects<'a>(&self, commit: &'a Commit) -> impl Iterator<Item = io::Result<bytes::Bytes>> + 'a {
        fn hashes(tx: &Arc<Transaction>) -> impl Iterator<Item = Hash> + '_ {
            tx.writes.iter().filter_map(|write| {
                if let DataKey::Hash(h) = write.data_key {
                    Some(h)
                } else {
                    None
                }
            })
        }

        let odb = self.odb.clone();
        commit.transactions.iter().flat_map(hashes).map(move |hash| {
            let odb = odb.lock().unwrap();
            odb.get(hash)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("Missing object: {hash}")))
        })
    }
}

/// A mutable handle to the commit log.
///
/// "Mutable" specifically means that new commits can be appended to the log
/// via [`CommitLogMut::append_tx`].
///
/// A [`CommitLog`] can by obtained from [`CommitLogMut`] via the [`From`] impl.
#[derive(Clone)]
pub struct CommitLogMut {
    mlog: Arc<Mutex<MessageLog>>,
    odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
    unwritten_commit: Arc<Mutex<Commit>>,
    fsync: FsyncPolicy,
}

impl CommitLogMut {
    /// Change the [`FsyncPolicy`].
    ///
    /// In effect for the next call to [`CommitLogMut::append_tx`].
    pub fn set_fsync(&mut self, fsync: FsyncPolicy) {
        self.fsync = fsync
    }

    /// Change the [`FsyncPolicy`].
    ///
    /// In effect for the next call to [`CommitLogMut::append_tx`].
    pub fn with_fsync(self, fsync: FsyncPolicy) -> Self {
        Self { fsync, ..self }
    }

    /// Return the latest commit offset.
    pub fn commit_offset(&self) -> u64 {
        self.mlog.lock().unwrap().open_segment_max_offset
    }

    /// Append the result of committed transaction [`TxData`] to the log.
    ///
    /// Returns the number of bytes written, or `None` if it was an empty
    /// transaction (i.e. one which did not modify any rows).
    #[tracing::instrument(skip_all)]
    pub fn append_tx<D>(
        &self,
        ctx: &ExecutionContext,
        tx_data: &TxData,
        _datastore: &D,
    ) -> Result<Option<usize>, DBError>
    where
        D: MutTxDatastore<RowId = RowPointer>,
    {
        // IMPORTANT: writes to the log must be sequential, so as to maintain
        // the commit order. `generate_commit` establishes an order between
        // [`Commit`] payloads, so the lock must be acquired here.
        //
        // See also: https://github.com/clockworklabs/SpacetimeDB/pull/465
        let mut mlog = self.mlog.lock().unwrap();
        // Also applies to `unwritten_commit` (see below).
        let mut unwritten_commit = self.unwritten_commit.lock().unwrap();
        let sz = if let Some(encoded_commit) = self.generate_commit(ctx, &mut unwritten_commit, tx_data) {
            // Clear transations immediately, so we don't write them again after
            // `append_commit_bytes` returned and error.
            unwritten_commit.transactions.clear();

            // Write and flush to the log.
            let sz = self.append_commit_bytes(&mut mlog, &encoded_commit)?;

            // Update offsets and parent hash only after we are reasonably sure
            // that the commit has been flushed to disk.
            // Otherwise gaps in the offset sequence will be hard to distinguish
            // from otherwise corrupt commits.
            // Commit checksums will be helpful in the future.
            unwritten_commit.parent_commit_hash = Some(hash_bytes(&encoded_commit));
            unwritten_commit.commit_offset += 1;
            unwritten_commit.min_tx_offset += unwritten_commit.transactions.len() as u64;

            Some(sz)
        } else {
            None
        };

        Ok(sz)
    }

    // Only for testing! Use `append_tx` if at all possible.
    fn append_commit_bytes(&self, mlog: &mut MutexGuard<'_, MessageLog>, commit: &[u8]) -> Result<usize, DBError> {
        mlog.append(commit)?;
        match self.fsync {
            FsyncPolicy::Never => mlog.flush()?,
            FsyncPolicy::EveryTx => {
                let offset = mlog.open_segment_max_offset;
                // Sync the odb first, as the mlog depends on its data. This is
                // not an atomicity guarantee, but the error context may help
                // with forensics.
                let mut odb = self.odb.lock().unwrap();
                odb.sync_all()
                    .with_context(|| format!("Error syncing odb to disk. Log offset: {offset}"))?;
                mlog.sync_all()
                    .with_context(|| format!("Error syncing mlog to disk. Log offset: {offset}"))?;
                log::trace!("DATABASE: FSYNC");
            }
        }

        Ok(commit.len())
    }

    fn generate_commit(
        &self,
        ctx: &ExecutionContext,
        unwritten_commit: &mut MutexGuard<'_, Commit>,
        tx_data: &TxData,
    ) -> Option<Vec<u8>> {
        // We are not creating a commit for empty transactions.
        // The reason for this is that empty transactions get encoded as 0 bytes,
        // so a commit containing an empty transaction contains no useful information.
        if tx_data.records.is_empty() {
            return None;
        }

        let mut writes = Vec::with_capacity(tx_data.records.len());

        let workload = &ctx.workload();
        let db = &ctx.database();
        let reducer = &ctx.reducer_name();

        for record in &tx_data.records {
            let table_id: u32 = record.table_id.into();
            let table_name = record.table_name.as_str();

            let operation = match record.op {
                TxOp::Insert(_) => {
                    // Increment rows inserted metric
                    #[cfg(feature = "metrics")]
                    DB_METRICS
                        .rdb_num_rows_inserted
                        .with_label_values(workload, db, reducer, &table_id, table_name)
                        .inc();
                    // Increment table rows gauge
                    DB_METRICS
                        .rdb_num_table_rows
                        .with_label_values(db, &table_id, table_name)
                        .inc();
                    Operation::Insert
                }
                TxOp::Delete => {
                    // Increment rows deleted metric
                    #[cfg(feature = "metrics")]
                    DB_METRICS
                        .rdb_num_rows_deleted
                        .with_label_values(workload, db, reducer, &table_id, table_name)
                        .inc();
                    // Decrement table rows gauge
                    DB_METRICS
                        .rdb_num_table_rows
                        .with_label_values(db, &table_id, table_name)
                        .dec();
                    Operation::Delete
                }
            };

            writes.push(Write {
                operation,
                set_id: table_id,
                data_key: record.key,
            })
        }

        let transaction = Transaction { writes };
        unwritten_commit.transactions.push(Arc::new(transaction));

        const COMMIT_SIZE: usize = 1;

        if unwritten_commit.transactions.len() >= COMMIT_SIZE {
            {
                let mut guard = self.odb.lock().unwrap();
                for record in &tx_data.records {
                    if let (DataKey::Hash(_), TxOp::Insert(bytes)) = (&record.key, &record.op) {
                        guard.add(Vec::clone(bytes));
                    }
                }
            }

            let mut bytes = Vec::with_capacity(unwritten_commit.encoded_len());
            unwritten_commit.encode(&mut bytes);
            Some(bytes)
        } else {
            None
        }
    }
}

impl From<&CommitLogMut> for CommitLog {
    fn from(log: &CommitLogMut) -> Self {
        Self {
            mlog: log.mlog.clone(),
            odb: log.odb.clone(),
        }
    }
}

/// Iterator over a single [`MessageLog`] segment, yielding [`Commit`]s.
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct IterSegment {
    inner: message_log::IterSegment,
}

impl IterSegment {
    fn bytes_read(&self) -> u64 {
        self.inner.bytes_read()
    }
}

impl Iterator for IterSegment {
    type Item = io::Result<Commit>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.inner.next()?;

        let ctx = || {
            format!(
                "Failed to decode commit in segment {:0>20} at byte offset: {}",
                self.inner.segment(),
                self.bytes_read()
            )
        };
        let io = |e| io::Error::new(io::ErrorKind::InvalidData, e);
        Some(next.and_then(|bytes| Commit::decode(&mut bytes.as_slice()).with_context(ctx).map_err(io)))
    }
}

impl From<message_log::IterSegment> for IterSegment {
    fn from(inner: message_log::IterSegment) -> Self {
        Self { inner }
    }
}

/// Iterator over a [`CommitLog`], yielding [`Commit`]s.
///
/// Created by [`CommitLog::iter`] and [`CommitLog::iter_from`] respectively.
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct Iter {
    commits: Option<IterSegment>,
    segments: message_log::Segments,
}

impl Iterator for Iter {
    type Item = io::Result<Commit>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(commits) = self.commits.as_mut() {
                if let Some(commit) = commits.next() {
                    return Some(commit);
                }
            }

            let segment = self.segments.next()?;
            match segment.try_into_iter() {
                Err(e) => return Some(Err(e)),
                Ok(inner) => {
                    self.commits = Some(IterSegment { inner });
                }
            }
        }
    }
}

impl From<message_log::Segments> for Iter {
    fn from(segments: message_log::Segments) -> Self {
        Self {
            commits: None,
            segments,
        }
    }
}

/// Iterator created by [`CommitLog::replay`].
///
/// Similar to [`Iter`], but performs integrity checking and maintains
/// additional state.
#[must_use = "iterators are lazy and do nothing unless consumed"]
struct Replay {
    tx_offset: u64,
    last_commit_offset: Option<u64>,
    last_hash: Option<Hash>,

    odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,

    segments: message_log::Segments,
    segment_offset: usize,

    current_segment: Option<IterSegment>,
}

impl Replay {
    fn collect_objects(&self, commit: &Commit) -> Result<MemoryObjectDB, ReplayError> {
        let odb = self.odb.lock().unwrap();
        let mut objects = HashMap::new();

        let hashes = commit
            .transactions
            .iter()
            .flat_map(|tx| &tx.writes)
            .filter_map(|write| {
                if let DataKey::Hash(hash) = write.data_key {
                    Some(hash)
                } else {
                    None
                }
            });
        for hash in hashes {
            if let hash_map::Entry::Vacant(entry) = objects.entry(hash) {
                let obj = odb.get(hash).ok_or(ReplayError::MissingObject {
                    segment_offset: self.segment_offset,
                    last_commit_offset: self.last_commit_offset.unwrap_or_default(),
                    hash,
                    referenced_from_commit_offset: commit.commit_offset,
                })?;
                entry.insert(obj);
            }
        }

        Ok(objects.into())
    }
}

enum ReplayError {
    /// A [`Commit`] was decoded successfully, but is not contiguous.
    ///
    /// The current format permits successful decoding even if the slice of data
    /// being decoded from is slightly off. This usually causes the commit
    /// offset to be wrong with respect to the preceding commit.
    ///
    /// This error may also arise if appending to a [`CommitLogMut`] is not
    /// properly synchronized, i.e. a regression of [`#465`][465].
    ///
    /// We may in the future verify the commit hash, and include expected and
    /// actual value in this variant.
    ///
    /// [465]: https://github.com/clockworklabs/SpacetimeDB/pull/465
    OutOfOrder {
        segment_offset: usize,
        last_commit_offset: u64,
        decoded_commit_offset: u64,
        expected: u64,
    },
    /// A [`Commit`] could not be decoded.
    ///
    /// Either the input was malformed, or we reached EOF unexpectedly. In
    /// either case, the segment is most definitely irrecoverably corrupted
    /// after `last_commit_offset`.
    CorruptedData {
        segment_offset: usize,
        last_commit_offset: u64,
        source: io::Error,
    },
    /// An object referenced from a [`Commit`] was not found in the object db.
    ///
    /// This error may occur in [`FsyncPolicy::Never`] mode, if the object db
    /// happened to not be flushed to disk but the corresponding message log
    /// write was.
    MissingObject {
        segment_offset: usize,
        last_commit_offset: u64,
        hash: Hash,
        referenced_from_commit_offset: u64,
    },
    /// Some other error occurred.
    ///
    /// May be a transient error. Processing should be aborted, and potentially
    /// retried later.
    Other { source: io::Error },
}

impl Iterator for Replay {
    type Item = Result<(Commit, MemoryObjectDB), ReplayError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(cur) = self.current_segment.as_mut() {
            if let Some(commit) = cur.next() {
                // We may be able to recover from a corrupt suffix of the log.
                // For this, we need to consider three cases:
                //
                //   1. The `Commit` was decoded successfully, but is invalid
                //   2. The `Commit` failed to decode
                //   3. The underlying `MessageLog` reported an unexpected EOF
                //
                // Case 1. can occur because the on-disk format does not
                // currently have any consistency checks built in. To detect it,
                // we check that the `commit_offset` sequence is contiguous.
                //
                // TODO(kim): We should probably check the `parent_commit_hash`
                // instead, but only after measuring the performance overhead.
                let res = match commit {
                    Ok(commit) => {
                        let expected = self.last_commit_offset.map(|last| last + 1).unwrap_or_default();
                        if commit.commit_offset != expected {
                            Err(ReplayError::OutOfOrder {
                                segment_offset: self.segment_offset,
                                last_commit_offset: self.last_commit_offset.unwrap_or_default(),
                                decoded_commit_offset: commit.commit_offset,
                                expected,
                            })
                        } else {
                            self.collect_objects(&commit).map(|objects| {
                                self.last_commit_offset = Some(commit.commit_offset);
                                self.last_hash = commit.parent_commit_hash;
                                self.tx_offset += commit.transactions.len() as u64;

                                (commit, objects)
                            })
                        }
                    }

                    Err(e) => {
                        let err = match e.kind() {
                            io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof => ReplayError::CorruptedData {
                                segment_offset: self.segment_offset,
                                last_commit_offset: self.last_commit_offset.unwrap_or_default(),
                                source: e,
                            },
                            _ => ReplayError::Other { source: e },
                        };
                        Err(err)
                    }
                };

                return Some(res);
            }
        }

        // Pop the next segment, if available.
        let next_segment = self.segments.next()?;
        self.segment_offset += 1;
        match next_segment.try_into_iter().map(IterSegment::from) {
            Ok(current_segment) => {
                self.current_segment = Some(current_segment);
                self.next()
            }
            Err(e) => Some(Err(ReplayError::Other { source: e })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    use crate::db::ostorage::memory_object_db::MemoryObjectDB;
    use spacetimedb_sats::data_key::InlineData;

    #[test]
    fn test_iter_commits() {
        let tmp = TempDir::with_prefix("commit_log_test").unwrap();

        let data_key = DataKey::Data(InlineData::from_bytes(b"asdf").unwrap());
        let tx = Transaction {
            writes: vec![
                Write {
                    operation: Operation::Insert,
                    set_id: 42,
                    data_key,
                },
                Write {
                    operation: Operation::Delete,
                    set_id: 42,
                    data_key,
                },
            ],
        };

        // The iterator doesn't verify integrity of commits, so we can just
        // write the same one repeatedly.
        let commit = Commit {
            parent_commit_hash: None,
            commit_offset: 0,
            min_tx_offset: 0,
            transactions: vec![Arc::new(tx)],
        };
        let mut commit_bytes = Vec::new();
        commit.encode(&mut commit_bytes);

        const COMMITS_PER_SEGMENT: usize = 10_000;
        const TOTAL_MESSAGES: usize = (COMMITS_PER_SEGMENT * 3) - 1;
        let segment_size: usize = COMMITS_PER_SEGMENT * (commit_bytes.len() + 4);

        let mlog = message_log::MessageLog::options()
            .max_segment_size(segment_size as u64)
            .open(tmp.path())
            .unwrap();
        let odb = MemoryObjectDB::default();

        let log = CommitLog::new(Arc::new(Mutex::new(mlog)), Arc::new(Mutex::new(Box::new(odb))))
            .to_mut()
            .unwrap()
            .with_fsync(FsyncPolicy::EveryTx);

        {
            let mut guard = log.mlog.lock().unwrap();
            for _ in 0..TOTAL_MESSAGES {
                log.append_commit_bytes(&mut guard, &commit_bytes).unwrap();
            }
        }

        let view = CommitLog::from(&log);
        let commits = view.iter().map(Result::unwrap).count();
        assert_eq!(TOTAL_MESSAGES, commits);

        let commits = view.iter_from(1_000_000).map(Result::unwrap).count();
        assert_eq!(0, commits);

        // No slicing yet, so offsets on segment boundaries yield an additional
        // COMMITS_PER_SEGMENT.
        let commits = view.iter_from(20_000).map(Result::unwrap).count();
        assert_eq!(9999, commits);

        let commits = view.iter_from(10_000).map(Result::unwrap).count();
        assert_eq!(19_999, commits);

        let commits = view.iter_from(9_999).map(Result::unwrap).count();
        assert_eq!(29_999, commits);
    }
}
