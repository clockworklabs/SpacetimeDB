use super::{
    datastore::traits::{MutTxDatastore, TxData},
    message_log::{self, MessageLog},
    messages::commit::Commit,
    ostorage::ObjectDB,
};
use crate::{
    db::{
        datastore::{locking_tx_datastore::RowId, traits::TxOp},
        db_metrics::DB_METRICS,
        messages::{
            transaction::Transaction,
            write::{Operation, Write},
        },
    },
    error::DBError,
    execution_context::ExecutionContext,
};

use anyhow::Context;
use parking_lot::RwLock;
use spacetimedb_lib::{
    hash::{hash_bytes, Hash},
    DataKey,
};

use std::io;
use std::sync::Arc;

#[derive(Clone)]
pub struct CommitLog {
    writer: Arc<RwLock<MessageLogWriter>>,
    odb: Arc<Box<dyn ObjectDB>>,
    fsync: bool,
}

impl CommitLog {
    pub fn new(mlog: MessageLog, odb: Box<dyn ObjectDB>, unwritten_commit: Commit, fsync: bool) -> Self {
        let encode_buf = Vec::with_capacity(unwritten_commit.encoded_len());
        let writer = Arc::new(RwLock::new(MessageLogWriter {
            mlog,
            unwritten_commit,
            encode_buf,
        }));
        Self {
            writer,
            odb: Arc::new(odb),
            fsync,
        }
    }

    /// Persist to disk the [Tx] result into the [MessageLog].
    ///
    /// Returns `Some(n_bytes_written)` if `commit_result` was persisted, `None` if it doesn't have bytes to write.
    #[tracing::instrument(skip_all)]
    pub fn append_tx<D>(
        &self,
        ctx: &ExecutionContext,
        tx_data: &TxData,
        _datastore: &D,
    ) -> Result<Option<usize>, DBError>
    where
        D: MutTxDatastore<RowId = RowId>,
    {
        let mut writer = self.writer.write();
        let bytes_written = writer.append(&self.odb, ctx, tx_data)?;
        if self.fsync {
            let offset = writer.mlog.open_segment_max_offset;
            // Sync the odb first, as the mlog depends on its data. This is
            // not an atomicity guarantee, but the error context may help
            // with forensics.
            self.odb
                .sync_all()
                .with_context(|| format!("Error syncing odb to disk. Log offset: {offset}"))?;
            writer
                .mlog
                .sync_all()
                .with_context(|| format!("Error syncing mlog to disk. Log offset: {offset}"))?;
            log::trace!("DATABASE: FSYNC");
        } else {
            writer.mlog.flush()?;
        }

        Ok(bytes_written)
    }
}

struct MessageLogWriter {
    mlog: MessageLog,
    unwritten_commit: Commit,
    encode_buf: Vec<u8>,
}

impl MessageLogWriter {
    pub fn append(
        &mut self,
        odb: &dyn ObjectDB,
        ctx: &ExecutionContext,
        tx_data: &TxData,
    ) -> Result<Option<usize>, DBError> {
        if let Some(len) = self.generate_commit(odb, ctx, tx_data) {
            self.mlog.append(&self.encode_buf[..len])?;
            Ok(Some(len))
        } else {
            Ok(Some(0))
        }
    }

    fn generate_commit(&mut self, odb: &dyn ObjectDB, ctx: &ExecutionContext, tx_data: &TxData) -> Option<usize> {
        // We are not creating a commit for empty transactions.
        // The reason for this is that empty transactions get encoded as 0 bytes,
        // so a commit containing an empty transaction contains no useful information.
        if tx_data.records.is_empty() {
            return None;
        }

        let unwritten_commit = &mut self.unwritten_commit;
        let mut writes = Vec::with_capacity(tx_data.records.len());

        let txn_type = &ctx.txn_type();
        let db = &ctx.database();
        let reducer = &ctx.reducer_name().unwrap_or_default();

        for record in &tx_data.records {
            let table_id: u32 = record.table_id.into();

            let operation = match record.op {
                TxOp::Insert(_) => {
                    // Increment rows inserted metric
                    DB_METRICS
                        .rdb_num_rows_inserted
                        .with_label_values(txn_type, db, reducer, &table_id)
                        .inc();
                    // Increment table rows gauge
                    DB_METRICS.rdb_num_table_rows.with_label_values(db, &table_id).inc();
                    Operation::Insert
                }
                TxOp::Delete => {
                    // Increment rows deleted metric
                    DB_METRICS
                        .rdb_num_rows_deleted
                        .with_label_values(txn_type, db, reducer, &table_id)
                        .inc();
                    // Decrement table rows gauge
                    DB_METRICS.rdb_num_table_rows.with_label_values(db, &table_id).dec();
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
            for record in &tx_data.records {
                match &record.op {
                    TxOp::Insert(bytes) => {
                        odb.add(bytes);
                    }
                    TxOp::Delete => continue,
                }
            }

            let encoded_len = unwritten_commit.encoded_len();
            if encoded_len > self.encode_buf.len() {
                self.encode_buf.resize(encoded_len, 0);
            }
            unwritten_commit.encode(&mut self.encode_buf.as_mut_slice());
            let encoded_bytes = &self.encode_buf[..encoded_len];

            unwritten_commit.parent_commit_hash = Some(hash_bytes(encoded_bytes));
            unwritten_commit.commit_offset += 1;
            unwritten_commit.min_tx_offset += unwritten_commit.transactions.len() as u64;
            unwritten_commit.transactions.clear();

            Some(encoded_len)
        } else {
            None
        }
    }
}

/// A read-only view of a [`CommitLog`].
pub struct CommitLogView {
    inner: CommitLog,
}

impl CommitLogView {
    /// The number of bytes on disk occupied by the [MessageLog].
    pub fn message_log_size_on_disk(&self) -> Result<u64, DBError> {
        if let Some(ref mlog) = self.mlog {
            let guard = mlog.lock().unwrap();
            Ok(guard.size())
        } else {
            Ok(0)
        }
    }

    /// The number of bytes on disk occupied by the [ObjectDB].
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
        let inner = self.inner.writer.read();
        inner.mlog.segments_from(offset)
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

        let odb = self.inner.odb.clone();
        commit.transactions.iter().flat_map(hashes).map(move |hash| {
            odb.get(hash)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("Missing object: {hash}")))
        })
    }
}

impl From<&CommitLog> for CommitLogView {
    fn from(log: &CommitLog) -> Self {
        Self { inner: log.clone() }
    }
}

#[must_use = "iterators are lazy and do nothing unless consumed"]
struct IterSegment {
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

/// Iterator over a [`CommitLogView`], yielding [`Commit`]s.
///
/// Created by [`CommitLogView::iter`] and [`CommitLogView::iter_from`]
/// respectively.
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

#[cfg(test)]
mod tests {
    use super::*;

    use spacetimedb_lib::data_key::InlineData;
    use tempfile::TempDir;

    use crate::db::ostorage::memory_object_db::MemoryObjectDB;

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

        let mut mlog = message_log::MessageLog::options()
            .max_segment_size(segment_size as u64)
            .open(tmp.path())
            .unwrap();
        {
            for _ in 0..TOTAL_MESSAGES {
                mlog.append(&commit_bytes).unwrap();
            }
            mlog.sync_all().unwrap();
        }
        let odb = MemoryObjectDB::default();

        let log = CommitLog::new(
            mlog,
            Box::new(odb),
            Commit {
                parent_commit_hash: None,
                commit_offset: 0,
                min_tx_offset: 0,
                transactions: Vec::new(),
            },
            false, // fsync
        );

        let view = CommitLogView::from(&log);
        let commits = view.iter().map(Result::unwrap).count();
        assert_eq!(TOTAL_MESSAGES, commits);

        let commits = view.iter_from(1_000_000).map(Result::unwrap).count();
        assert_eq!(0, commits);

        // No slicing yet, so offsets on segment boundaries yield an additional
        // COMMITS_PER_SEGMENT.
        let commits = view.iter_from(20_001).map(Result::unwrap).count();
        assert_eq!(9999, commits);

        let commits = view.iter_from(10_001).map(Result::unwrap).count();
        assert_eq!(19_999, commits);

        let commits = view.iter_from(10_000).map(Result::unwrap).count();
        assert_eq!(29_999, commits);
    }
}
