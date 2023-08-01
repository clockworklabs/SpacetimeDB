use super::{
    datastore::traits::MutTxDatastore,
    message_log::MessageLog,
    messages::{commit::Commit, transaction::Transaction},
    ostorage::ObjectDB,
};
use crate::{
    db::{datastore::locking_tx_datastore::RowId, messages::write::Operation},
    error::DBError,
};
use spacetimedb_lib::{hash::hash_bytes, DataKey};
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone)]
pub struct CommitLog {
    mlog: Arc<Mutex<MessageLog>>,
    odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
    unwritten_commit: Arc<Mutex<Commit>>,
}

impl CommitLog {
    pub fn new(
        mlog: Arc<Mutex<MessageLog>>,
        odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
        unwritten_commit: Commit,
    ) -> Self {
        Self {
            mlog,
            odb,
            unwritten_commit: Arc::new(Mutex::new(unwritten_commit)),
        }
    }

    /// Persist to disk the [Tx] result into the [MessageLog].
    ///
    /// Returns `Some(n_bytes_written)` if `commit_result` was persisted, `false` if it doesn't have bytes to write.
    #[tracing::instrument(skip_all)]
    pub fn append_tx<D>(&self, transaction: Arc<Transaction>, datastore: &D) -> Result<Option<usize>, DBError>
    where
        D: MutTxDatastore<RowId = RowId>,
    {
        if let Some(bytes) = self.generate_commit(transaction, datastore) {
            let mut mlog = self.mlog.lock().unwrap();
            mlog.append(&bytes)?;
            mlog.sync_all()?;
            log::trace!("DATABASE: FSYNC");
            Ok(Some(bytes.len()))
        } else {
            Ok(None)
        }
    }

    fn generate_commit<D>(&self, transaction: Arc<Transaction>, datastore: &D) -> Option<Vec<u8>>
    where
        D: MutTxDatastore<RowId = RowId>,
    {
        // TODO(george) Don't clone the data, just the Arc.
        let mut unwritten_commit = self.unwritten_commit.lock().unwrap();
        unwritten_commit.transactions.push(transaction.clone());

        const COMMIT_SIZE: usize = 1;

        let tx = datastore.begin_mut_tx();
        if unwritten_commit.transactions.len() >= COMMIT_SIZE {
            let mut datas = Vec::new();
            for write in transaction.writes.iter() {
                match write.operation {
                    Operation::Delete => continue, // if we deleted a value, then the data is not in the datastore
                    Operation::Insert => (),
                }
                let data = match write.data_key {
                    DataKey::Data(data) => Arc::new(data.to_vec()),
                    DataKey::Hash(_) => datastore
                        .resolve_data_key_mut_tx(&tx, &write.data_key)
                        .unwrap()
                        .unwrap(),
                };
                datas.push(data)
            }
            {
                let mut guard = self.odb.lock().unwrap();
                for data in datas.into_iter() {
                    guard.add((*data).clone());
                }
            }

            let mut bytes = Vec::new();
            unwritten_commit.encode(&mut bytes);

            unwritten_commit.parent_commit_hash = Some(hash_bytes(&bytes));
            unwritten_commit.commit_offset += 1;
            unwritten_commit.min_tx_offset += unwritten_commit.transactions.len() as u64;
            unwritten_commit.transactions.clear();

            datastore.rollback_mut_tx(tx);

            Some(bytes)
        } else {
            None
        }
    }
}
