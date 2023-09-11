use super::{
    datastore::traits::{MutTxDatastore, TxData},
    message_log::{self, MessageLog},
    messages::commit::Commit,
    ostorage::ObjectDB,
};
use crate::{
    db::{
        datastore::{locking_tx_datastore::RowId, traits::TxOp},
        messages::{
            transaction::Transaction,
            write::{Operation, Write},
        },
    },
    error::DBError,
};

use spacetimedb_lib::{
    hash::{hash_bytes, Hash},
    DataKey,
};

use std::io;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone)]
pub struct CommitLog {
    mlog: Option<Arc<Mutex<MessageLog>>>,
    odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
    unwritten_commit: Arc<Mutex<Commit>>,
    fsync: bool,
}

impl CommitLog {
    pub fn new(
        mlog: Option<Arc<Mutex<MessageLog>>>,
        odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
        unwritten_commit: Commit,
        fsync: bool,
    ) -> Self {
        Self {
            mlog,
            odb,
            unwritten_commit: Arc::new(Mutex::new(unwritten_commit)),
            fsync,
        }
    }

    /// Persist to disk the [Tx] result into the [MessageLog].
    ///
    /// Returns `Some(n_bytes_written)` if `commit_result` was persisted, `None` if it doesn't have bytes to write.
    #[tracing::instrument(skip_all)]
    pub fn append_tx<D>(&self, tx_data: &TxData, datastore: &D) -> Result<Option<usize>, DBError>
    where
        D: MutTxDatastore<RowId = RowId>,
    {
        if let Some(bytes) = self.generate_commit(tx_data, datastore) {
            if let Some(mlog) = &self.mlog {
                let mut mlog = mlog.lock().unwrap();
                mlog.append(&bytes)?;
                if self.fsync {
                    mlog.sync_all()?;
                    let mut odb = self.odb.lock().unwrap();
                    odb.sync_all()?;
                    log::trace!("DATABASE: FSYNC");
                } else {
                    mlog.flush()?;
                }
            }
            Ok(Some(bytes.len()))
        } else {
            Ok(None)
        }
    }

    fn generate_commit<D: MutTxDatastore<RowId = RowId>>(&self, tx_data: &TxData, _datastore: &D) -> Option<Vec<u8>> {
        // We are not creating a commit for empty transactions.
        // The reason for this is that empty transactions get encoded as 0 bytes,
        // so a commit containing an empty transaction contains no useful information.
        if tx_data.records.is_empty() {
            return None;
        }

        let mut unwritten_commit = self.unwritten_commit.lock().unwrap();
        let writes = tx_data
            .records
            .iter()
            .map(|record| Write {
                operation: match record.op {
                    TxOp::Insert(_) => Operation::Insert,
                    TxOp::Delete => Operation::Delete,
                },
                set_id: record.table_id.0,
                data_key: record.key,
            })
            .collect();
        let transaction = Transaction { writes };
        unwritten_commit.transactions.push(Arc::new(transaction));

        const COMMIT_SIZE: usize = 1;

        if unwritten_commit.transactions.len() >= COMMIT_SIZE {
            {
                let mut guard = self.odb.lock().unwrap();
                for record in &tx_data.records {
                    match &record.op {
                        TxOp::Insert(bytes) => {
                            guard.add(Vec::clone(bytes));
                        }
                        TxOp::Delete => continue,
                    }
                }
            }

            let mut bytes = Vec::new();
            unwritten_commit.encode(&mut bytes);

            unwritten_commit.parent_commit_hash = Some(hash_bytes(&bytes));
            unwritten_commit.commit_offset += 1;
            unwritten_commit.min_tx_offset += unwritten_commit.transactions.len() as u64;
            unwritten_commit.transactions.clear();

            Some(bytes)
        } else {
            None
        }
    }
}

/// A read-only view of a [`CommitLog`].
pub struct CommitLogView {
    mlog: Option<Arc<Mutex<MessageLog>>>,
    odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
}

impl CommitLogView {
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
        if let Some(mlog) = &self.mlog {
            let mlog = mlog.lock().unwrap();
            mlog.segments_from(offset)
        } else {
            message_log::Segments::empty()
        }
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

impl From<&CommitLog> for CommitLogView {
    fn from(log: &CommitLog) -> Self {
        Self {
            mlog: log.mlog.clone(),
            odb: log.odb.clone(),
        }
    }
}

#[must_use = "iterators are lazy and do nothing unless consumed"]
struct IterSegment {
    inner: message_log::IterSegment,
}

impl Iterator for IterSegment {
    type Item = io::Result<Commit>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.inner.next()?;
        Some(next.map(|bytes| {
            // It seems very improbable that `decode` is infallible...
            let (commit, _) = Commit::decode(bytes);
            commit
        }))
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
            if let Some(mut commits) = self.commits.take() {
                if let Some(commit) = commits.next() {
                    self.commits = Some(commits);
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
