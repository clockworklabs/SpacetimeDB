use super::{
    datastore::traits::{MutTxDatastore, TxData},
    message_log::MessageLog,
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
use spacetimedb_lib::hash::hash_bytes;
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
