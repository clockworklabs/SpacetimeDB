use spacetimedb_lib::hash::hash_bytes;

use super::{
    datastore::{
        gitlike_tx_blobstore::{Gitlike, ScanIterator, TxId},
        traits::{Blob, BlobRow, MutTx, MutTxBlobstore},
    },
    message_log::MessageLog,
    messages::{transaction::Transaction, write::DataKey},
    ostorage::ObjectDB,
};
use crate::{
    db::messages::{commit::Commit, write::Operation},
    util::prometheus_handle::HistogramHandle,
};
use crate::{
    db::{
        datastore::traits::TableId,
        db_metrics::{TDB_COMMIT_TIME, TDB_DELETE_TIME, TDB_INSERT_TIME, TDB_SCAN_TIME, TDB_SEEK_TIME},
    },
    error::DBError,
    hash::Hash,
};
use std::{
    cell::RefCell,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex},
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct Read {
    set_id: u32,
    value: DataKey,
}

pub struct CommitResult {
    pub tx: Transaction,
    pub commit_bytes: Option<Vec<u8>>,
}

impl std::fmt::Debug for CommitResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommitResult").finish()
    }
}

#[derive(Debug)]
pub struct Tx {
    tx: TxId,
}

pub struct TxWrapper<Db: TxCtx> {
    tx: ManuallyDrop<Tx>,
    db: Db,
}

impl<Db: TxCtx> TxWrapper<Db> {
    pub fn begin(mut db: Db) -> Self {
        let tx = ManuallyDrop::new(db.begin_tx_raw());
        Self { tx, db }
    }
    pub fn get(&mut self) -> (&mut Tx, &mut Db) {
        (&mut self.tx, &mut self.db)
    }
    pub fn with<R>(&mut self, f: impl FnOnce(&mut Tx, &mut Db) -> R) -> R {
        f(&mut self.tx, &mut self.db)
    }
    #[inline]
    pub fn rollback(self) {}
    #[inline]
    pub fn commit(self) -> Result<Option<CommitResult>, DBError> {
        let (_, res) = self.commit_into_db()?;
        Ok(res)
    }
    #[inline]
    pub fn commit_into_db(self) -> Result<(Db, Option<CommitResult>), DBError> {
        let mut me = ManuallyDrop::new(self);
        // SAFETY: we're not calling Self::drop(), so it's okay to ManuallyDrop::take tx
        let tx = unsafe { ManuallyDrop::take(&mut me.tx) };
        let mut db = unsafe { std::ptr::read(&me.db) };
        let res = db.commit_tx(tx)?;
        Ok((db, res))
    }
}
impl<Db: TxCtx> Drop for TxWrapper<Db> {
    fn drop(&mut self) {
        // SAFETY: we're inside drop(), it's always safe to ManuallyDrop::take
        let tx = unsafe { ManuallyDrop::take(&mut self.tx) };
        self.db.rollback_tx(tx)
    }
}

impl<Db: TxCtx> Deref for TxWrapper<Db> {
    type Target = Tx;
    fn deref(&self) -> &Tx {
        &self.tx
    }
}

impl<Db: TxCtx> DerefMut for TxWrapper<Db> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tx
    }
}

pub trait TxCtx {
    fn begin_tx_raw(&mut self) -> Tx;
    fn rollback_tx(&mut self, tx: Tx);
    fn commit_tx(&mut self, tx: Tx) -> Result<Option<CommitResult>, DBError>;
}

impl<T: TxCtx> TxCtx for &mut T {
    fn begin_tx_raw(&mut self) -> Tx {
        T::begin_tx_raw(self)
    }

    fn rollback_tx(&mut self, tx: Tx) {
        T::rollback_tx(self, tx)
    }

    fn commit_tx(&mut self, tx: Tx) -> Result<Option<CommitResult>, DBError> {
        T::commit_tx(self, tx)
    }
}

impl<T: TxCtx> TxCtx for &RefCell<T> {
    fn begin_tx_raw(&mut self) -> Tx {
        self.borrow_mut().begin_tx_raw()
    }

    fn rollback_tx(&mut self, tx: Tx) {
        self.borrow_mut().rollback_tx(tx)
    }

    fn commit_tx(&mut self, tx: Tx) -> Result<Option<CommitResult>, DBError> {
        self.borrow_mut().commit_tx(tx)
    }
}

pub struct TransactionalDB {
    blobstore: Gitlike,
    odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
    unwritten_commit: Commit,
}

impl TransactionalDB {
    pub fn open(
        message_log: Arc<Mutex<MessageLog>>,
        odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
    ) -> Result<Self, DBError> {
        let blobstore = Gitlike::new();
        let unwritten_commit = {
            let message_log = message_log.lock().unwrap();
            let mut transaction_offset = 0;
            let mut last_commit_offset = None;
            let mut last_hash: Option<Hash> = None;
            for message in message_log.iter() {
                let (commit, _) = Commit::decode(message);
                last_hash = commit.parent_commit_hash;
                last_commit_offset = Some(commit.commit_offset);
                for transaction in commit.transactions {
                    transaction_offset += 1;
                    // NOTE: Although I am creating a blobstore transaction in a
                    // one to one fashion for each message log transaction, this
                    // is just to reduce memory usage while inserting. We don't
                    // really care about inserting these transactionally as long
                    // as all of the writes get inserted.
                    let mut tx = blobstore.begin_mut_tx();
                    for write in &transaction.writes {
                        let table_id = TableId(write.set_id);
                        match write.operation {
                            Operation::Delete => {
                                blobstore
                                    .delete_row_blob_mut_tx(&mut tx, table_id, write.data_key)
                                    .unwrap();
                            }
                            Operation::Insert => {
                                match write.data_key {
                                    DataKey::Data(data) => {
                                        blobstore.insert_row_blob_mut_tx(&mut tx, table_id, &data).unwrap();
                                    }
                                    DataKey::Hash(hash) => {
                                        let data = odb.lock().unwrap().get(hash).unwrap();
                                        blobstore.insert_row_blob_mut_tx(&mut tx, table_id, &data).unwrap();
                                    }
                                };
                            }
                        }
                    }
                    blobstore.commit_mut_tx(tx).unwrap();
                }
            }

            let commit_offset = if let Some(last_commit_offset) = last_commit_offset {
                last_commit_offset + 1
            } else {
                0
            };

            log::debug!(
                "Initialized with {} commits and tx offset {}",
                commit_offset,
                transaction_offset
            );

            Commit {
                parent_commit_hash: last_hash,
                commit_offset,
                min_tx_offset: transaction_offset,
                transactions: Vec::new(),
            }
        };

        Ok(Self {
            blobstore,
            odb,
            unwritten_commit,
        })
    }

    pub fn reset_hard(&mut self, message_log: Arc<Mutex<MessageLog>>) -> Result<(), anyhow::Error> {
        *self = Self::open(message_log, self.odb.clone())?;
        Ok(())
    }

    pub fn begin_tx(&mut self) -> TxWrapper<&mut Self> {
        TxWrapper::begin(self)
    }

    fn generate_commit(&mut self, transaction: Arc<Transaction>) -> CommitResult {
        // TODO(george) Don't clone the data, just the Arc.
        let tx: Transaction = (*transaction).clone();
        self.unwritten_commit.transactions.push(tx.clone());

        const COMMIT_SIZE: usize = 1;

        let commit_bytes = if self.unwritten_commit.transactions.len() >= COMMIT_SIZE {
            let mut datas = Vec::new();
            for write in tx.writes.iter() {
                let data = match write.data_key {
                    DataKey::Data(data) => data.to_vec(),
                    DataKey::Hash(_) => {
                        let blob_ref = self.blobstore.from_data_key(&write.data_key).unwrap();
                        let blob = self.blobstore.blob_to_owned(blob_ref);
                        // TODO(george) More copying!
                        blob.view().to_vec()
                    }
                };
                datas.push(data)
            }
            {
                let mut guard = self.odb.lock().unwrap();
                for data in datas.into_iter() {
                    guard.add(data);
                }
            }

            let mut bytes = Vec::new();
            self.unwritten_commit.encode(&mut bytes);

            self.unwritten_commit.parent_commit_hash = Some(hash_bytes(&bytes));
            self.unwritten_commit.commit_offset += 1;
            self.unwritten_commit.min_tx_offset += self.unwritten_commit.transactions.len() as u64;
            self.unwritten_commit.transactions.clear();

            Some(bytes)
        } else {
            None
        };

        CommitResult { tx, commit_bytes }
    }
}

impl TxCtx for TransactionalDB {
    fn begin_tx_raw(&mut self) -> Tx {
        Tx {
            tx: self.blobstore.begin_mut_tx(),
        }
    }

    fn rollback_tx(&mut self, tx: Tx) {
        self.blobstore.rollback_mut_tx(tx.tx)
    }

    fn commit_tx(&mut self, tx: Tx) -> Result<Option<CommitResult>, DBError> {
        let mut measure = HistogramHandle::new(&TDB_COMMIT_TIME);

        // Start timing this whole function; `Drop` will measure time spent.
        measure.start();

        let transaction = match self.blobstore.commit_mut_tx(tx.tx)? {
            Some(transaction) => transaction,
            None => return Ok(None),
        };

        Ok(Some(self.generate_commit(transaction)))
    }
}

impl TransactionalDB {
    pub fn from_data_key<T, F: Fn(&[u8]) -> T>(&self, data_key: &DataKey, f: F) -> Option<T> {
        let blob_ref = self.blobstore.from_data_key(data_key)?;
        let blob = self.blobstore.blob_to_owned(blob_ref);
        Some(f(blob.view()))
    }

    pub fn seek(&self, tx: &Tx, set_id: u32, data_key: DataKey) -> Option<Vec<u8>> {
        let mut measure = HistogramHandle::new(&TDB_SEEK_TIME);

        // Start timing this whole function; `Drop` will measure time spent.
        measure.start();

        let blob_ref = self
            .blobstore
            .get_row_blob_mut_tx(&tx.tx, TableId::raw(set_id), data_key)
            .unwrap()?;
        let blob = self.blobstore.blob_to_owned(blob_ref);
        Some(blob.view().to_vec())
    }

    pub fn scan<'a>(&'a self, tx: &'a Tx, set_id: u32) -> ScanIter<'a> {
        let inner = self.blobstore.scan_blobs_mut_tx(&tx.tx, TableId::raw(set_id)).unwrap();
        ScanIter::new(inner, &self.blobstore)
    }

    pub fn delete(&self, tx: &mut Tx, set_id: u32, data_key: DataKey) {
        let mut measure = HistogramHandle::new(&TDB_DELETE_TIME);

        // Start timing this whole function; `Drop` will measure time spent.
        measure.start();

        self.blobstore
            .delete_row_blob_mut_tx(&mut tx.tx, TableId::raw(set_id), data_key)
            .unwrap();
    }

    pub fn insert(&self, tx: &mut Tx, set_id: u32, bytes: Vec<u8>) -> DataKey {
        let mut measure = HistogramHandle::new(&TDB_INSERT_TIME);

        // Start timing this whole function; `Drop` will measure time spent.
        measure.start();

        self.blobstore
            .insert_row_blob_mut_tx(&mut tx.tx, TableId::raw(set_id), &bytes)
            .unwrap()
    }
}

pub struct ScanIter<'a> {
    inner: ScanIterator<'a>,
    gitlike: &'a Gitlike,
}

impl<'a> ScanIter<'a> {
    fn new(inner: ScanIterator<'a>, gitlike: &'a Gitlike) -> Self {
        Self { inner, gitlike }
    }
}

impl<'a> Iterator for ScanIter<'a> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut measure = HistogramHandle::new(&TDB_SCAN_TIME);

        // Start timing this whole function; `Drop` will measure time spent.
        measure.start();

        let blob_ref = self.inner.next()?;
        let blob = self.gitlike.blob_to_owned(blob_ref);
        Some(blob.view().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::Arc;
    use std::sync::Mutex;

    use crate::db::message_log::MessageLog;
    use crate::db::relational_db::make_default_ostorage;
    use spacetimedb_lib::error::ResultTest;
    use tempdir::TempDir;

    use super::{TransactionalDB, TxWrapper};
    use crate::hash::hash_bytes;
    use crate::hash::Hash;

    unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
        ::std::slice::from_raw_parts((p as *const T) as *const u8, ::std::mem::size_of::<T>())
    }

    #[repr(C, packed)]
    #[derive(Debug, Copy, Clone)]
    pub struct MyStruct {
        my_name: Hash,
        my_i32: i32,
        my_u64: u64,
        my_hash: Hash,
    }

    impl MyStruct {
        fn encode(&self) -> Vec<u8> {
            unsafe { any_as_u8_slice(self) }.to_vec()
        }

        fn decode(bytes: &[u8]) -> Self {
            unsafe { std::ptr::read(bytes.as_ptr() as *const _) }
        }
    }

    #[test]
    fn test_insert_and_seek_bytes() -> ResultTest<()> {
        let tmp_dir = TempDir::new("txdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut db = TransactionalDB::open(mlog, odb)?;
        let mut tx_ = db.begin_tx();
        let (tx, db) = tx_.get();
        let row_key_1 = db.insert(tx, 0, b"this is a byte string".to_vec());
        let (db, _) = tx_.commit_into_db()?;

        let mut tx_ = db.begin_tx();
        let (tx, db) = tx_.get();
        let row = db.seek(tx, 0, row_key_1).unwrap();

        assert_eq!(b"this is a byte string", row.as_slice());
        Ok(())
    }

    #[test]
    fn test_insert_and_seek_struct() -> ResultTest<()> {
        let tmp_dir = TempDir::new("txdb_test").unwrap();
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut db = TransactionalDB::open(mlog, odb)?;
        let mut tx_ = db.begin_tx();
        let (tx, db) = tx_.get();
        let row_key_1 = db.insert(
            tx,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -1,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );
        let (db, _) = tx_.commit_into_db()?;

        let mut tx_ = db.begin_tx();
        let (tx, db) = tx_.get();
        let row = MyStruct::decode(db.seek(tx, 0, row_key_1).unwrap().as_slice());

        let i = row.my_i32;
        assert_eq!(i, -1);
        Ok(())
    }

    #[test]

    fn test_read_isolation() -> ResultTest<()> {
        let tmp_dir = TempDir::new("txdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut db = TransactionalDB::open(mlog, odb)?;
        let mut tx_1 = db.begin_tx();
        let row_key_1 = tx_1.with(|tx_1, db| {
            let row_key_1 = db.insert(
                tx_1,
                0,
                MyStruct {
                    my_name: hash_bytes(b"This is a byte string."),
                    my_i32: -1,
                    my_u64: 1,
                    my_hash: hash_bytes(b"This will be turned into a hash."),
                }
                .encode(),
            );

            let mut tx_2 = db.begin_tx();
            tx_2.with(|tx_2, db| {
                let row = db.seek(tx_2, 0, row_key_1);
                assert!(row.is_none());
            });
            row_key_1
        });

        tx_1.commit()?;

        let mut tx_3 = db.begin_tx();
        let (tx_3, db) = tx_3.get();
        let row = db.seek(tx_3, 0, row_key_1);
        assert!(row.is_some());
        Ok(())
    }

    #[test]
    fn test_scan() -> ResultTest<()> {
        let tmp_dir = TempDir::new("txdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut db = TransactionalDB::open(mlog, odb)?;
        let mut tx_1_ = db.begin_tx();
        let (tx_1, db) = tx_1_.get();
        let _row_key_1 = db.insert(
            tx_1,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -1,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );
        let _row_key_2 = db.insert(
            tx_1,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -2,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );

        let mut scan_1 = db.scan(tx_1, 0).collect::<Vec<Vec<u8>>>();
        scan_1.sort();

        let (db, _) = tx_1_.commit_into_db()?;

        let mut tx_2 = db.begin_tx();
        let (tx_2, db) = tx_2.get();
        let mut scan_2 = db.scan(tx_2, 0).collect::<Vec<Vec<u8>>>();
        scan_2.sort();

        assert_eq!(scan_1.len(), scan_2.len());

        for (i, _) in scan_1.iter().enumerate() {
            let val_1 = &scan_1[i];
            let val_2 = &scan_2[i];
            for i in 0..val_1.len() {
                assert_eq!(val_1[i], val_2[i]);
            }
        }

        Ok(())
    }

    #[test]
    fn test_write_skew_conflict() -> ResultTest<()> {
        let tmp_dir = TempDir::new("txdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let db = TransactionalDB::open(mlog, odb)?;
        let db = RefCell::new(db);
        let mut tx_1 = TxWrapper::begin(&db);
        let row_key_1 = db.borrow_mut().insert(
            &mut tx_1,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -1,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );

        let tx_2 = TxWrapper::begin(&db);
        let row = db.borrow_mut().seek(&tx_2, 0, row_key_1);
        assert!(row.is_none());

        assert!(tx_1.commit()?.is_some());
        assert!(tx_2.commit()?.is_none());
        Ok(())
    }

    #[test]
    fn test_write_skew_no_conflict() -> ResultTest<()> {
        let tmp_dir = TempDir::new("txdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let db = TransactionalDB::open(mlog, odb)?;
        let db = RefCell::new(db);
        let mut tx_1 = TxWrapper::begin(&db);
        let row_key_1 = db.borrow_mut().insert(
            &mut tx_1,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -1,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );
        let row_key_2 = db.borrow_mut().insert(
            &mut tx_1,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -2,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );
        assert!(tx_1.commit()?.is_some());

        let mut tx_2 = TxWrapper::begin(&db);
        let row = db.borrow_mut().seek(&tx_2, 0, row_key_1);
        assert!(row.is_some());
        db.borrow_mut().delete(&mut tx_2, 0, row_key_2);

        let mut tx_3 = TxWrapper::begin(&db);
        db.borrow_mut().delete(&mut tx_3, 0, row_key_1);

        assert!(tx_2.commit()?.is_some());
        assert!(tx_3.commit()?.is_some());
        Ok(())
    }
}
