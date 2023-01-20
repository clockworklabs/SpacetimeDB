use spacetimedb_lib::data_key;

use super::{
    message_log::MessageLog,
    messages::{
        commit::Commit,
        transaction::Transaction,
        write::{DataKey, Operation, Write},
    },
};
use crate::db::db_metrics::{TDB_COMMIT_TIME, TDB_DELETE_TIME, TDB_INSERT_TIME, TDB_SCAN_TIME, TDB_SEEK_TIME};
use crate::db::ostorage::ObjectDB;
use crate::error::DBError;
use crate::hash::{hash_bytes, Hash};
use crate::util::prometheus_handle::HistogramHandle;
use std::{
    cell::RefCell,
    collections::{hash_set::Iter, HashMap, HashSet},
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

#[derive(Debug, Clone)]
pub struct Tx {
    parent_tx_offset: u64,
    writes: Vec<Write>,
    reads: Vec<Read>,
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

// TODO: implement some kind of tag/dataset/namespace system
// which allows the user to restrict the search for data
// to a tag or set of tags. Honestly, maybe forget the content
// addressing and just use a map like CockroachDB, idk.
pub struct ClosedHashSet {
    set: HashSet<DataKey>,
}

impl ClosedHashSet {
    fn contains(&self, data_key: DataKey) -> bool {
        let set = &self.set;
        set.contains(&data_key)
    }

    fn insert(&mut self, data_key: DataKey) {
        let set = &mut self.set;
        set.insert(data_key);
    }

    fn delete(&mut self, data_key: DataKey) {
        let set = &mut self.set;
        set.remove(&data_key);
    }

    fn len(&self) -> usize {
        let set = &self.set;
        set.len()
    }
}

pub struct ClosedState {
    hash_sets: HashMap<u32, ClosedHashSet>,
}

impl ClosedState {
    fn new() -> Self {
        Self {
            hash_sets: HashMap::new(),
        }
    }

    fn contains(&self, set_id: u32, value: DataKey) -> bool {
        if let Some(set) = self.hash_sets.get(&set_id) {
            set.contains(value)
        } else {
            false
        }
    }

    fn insert(&mut self, set_id: u32, data_key: DataKey) {
        if let Some(set) = self.hash_sets.get_mut(&set_id) {
            set.insert(data_key)
        } else {
            let mut set = HashSet::new();
            set.insert(data_key);
            self.hash_sets.insert(set_id, ClosedHashSet { set });
        }
    }

    fn delete(&mut self, set_id: u32, data_key: DataKey) {
        let sets = &mut self.hash_sets;
        if let Some(set) = sets.get_mut(&set_id) {
            // Not atomic, but also correct
            set.delete(data_key);
            if set.len() == 0 {
                //TODO: Is this necessary?
                //drop(set);
                sets.remove(&set_id);
            }
        } else {
            // Do nothing
        }
    }

    fn iter(&self, set_id: u32) -> Option<Iter<DataKey>> {
        let sets = &self.hash_sets;
        sets.get(&set_id).map(|set| set.set.iter())
    }
}

pub struct TransactionalDB {
    pub odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
    closed_state: ClosedState,
    unwritten_commit: Commit,

    // TODO: it may be possible to move all this logic directly
    // into the values themselves which might make it easier to
    // index open transaction values
    closed_transaction_offset: u64,
    open_transactions: Vec<Transaction>,
    open_transaction_offsets: Vec<u64>,
    branched_transaction_offsets: Vec<u64>,
}

impl TransactionalDB {
    pub fn open(
        message_log: Arc<Mutex<MessageLog>>,
        odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
    ) -> Result<Self, DBError> {
        let message_log = message_log.lock().unwrap();
        let mut closed_state = ClosedState::new();
        let mut closed_transaction_offset = 0;
        let mut last_commit_offset = None;
        let mut last_hash: Option<Hash> = None;
        for message in message_log.iter() {
            let (commit, _) = Commit::decode(message);
            last_hash = commit.parent_commit_hash;
            last_commit_offset = Some(commit.commit_offset);
            for transaction in commit.transactions {
                closed_transaction_offset += 1;
                for write in &transaction.writes {
                    if write.operation.to_u8() == 0 {
                        closed_state.delete(write.set_id, write.data_key);
                    } else {
                        closed_state.insert(write.set_id, write.data_key);
                    }
                }
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
            closed_transaction_offset
        );

        let txdb = Self {
            odb,
            closed_state,
            unwritten_commit: Commit {
                parent_commit_hash: last_hash,
                commit_offset,
                min_tx_offset: closed_transaction_offset,
                transactions: Vec::new(),
            },
            closed_transaction_offset,
            open_transactions: Vec::new(),
            open_transaction_offsets: Vec::new(),
            branched_transaction_offsets: Vec::new(),
        };

        Ok(txdb)
    }

    pub fn reset_hard(&mut self, message_log: Arc<Mutex<MessageLog>>) -> Result<(), anyhow::Error> {
        *self = Self::open(message_log, self.odb.clone())?;
        Ok(())
    }

    fn latest_transaction_offset(&self) -> u64 {
        self.open_transaction_offsets
            .last()
            .copied()
            .unwrap_or(self.closed_transaction_offset)
    }

    fn get_open_transaction(&self, offset: u64) -> &Transaction {
        let index = (offset - self.closed_transaction_offset) - 1;
        // Assumes open transactions are deleted from the beginning
        &self.open_transactions[index as usize]
    }
    pub fn begin_tx(&mut self) -> TxWrapper<&mut Self> {
        TxWrapper::begin(self)
    }
}

impl TxCtx for TransactionalDB {
    fn begin_tx_raw(&mut self) -> Tx {
        if self.open_transactions.len() > 100 {
            log::warn!(
                "Open transactions len is {}. Be sure to commit or rollback transactions.",
                self.open_transactions.len()
            );
        }
        let parent = self.latest_transaction_offset();
        self.branched_transaction_offsets.push(parent);
        Tx {
            parent_tx_offset: parent,
            reads: Vec::new(),
            writes: Vec::new(),
        }
    }

    fn rollback_tx(&mut self, tx: Tx) {
        // Remove my branch
        if let Some(index) = self
            .branched_transaction_offsets
            .iter()
            .position(|offset| *offset == tx.parent_tx_offset)
        {
            self.branched_transaction_offsets.swap_remove(index);
        }
        self.vacuum_open_transactions();
    }

    fn commit_tx(&mut self, tx: Tx) -> Result<Option<CommitResult>, DBError> {
        let mut measure = HistogramHandle::new(&TDB_COMMIT_TIME);

        // Start timing this whole function; `Drop` will measure time spent.
        measure.start();

        if self.latest_transaction_offset() == tx.parent_tx_offset {
            return Ok(Some(self.finalize(tx)?));
        }

        // If not, we need to merge.
        // - If I did not read something that someone else wrote to, we're good to just merge
        // in my changes, because the part of the DB I read from is still current.
        // - If I did read something someone else wrote to, then the issue is that I am
        // potentially basing my changes off of stuff that has changed since I last pulled.
        // I should pull and then try to reapply my changes based on the most current state
        // of the database (basically rebase off latest_transaction). Really this just means the
        // transaction has failed, and the transaction should be tried again with a a new
        // parent transaction.
        let mut read_set: HashSet<Read> = HashSet::new();
        for read in &tx.reads {
            read_set.insert(*read);
        }

        // NOTE: latest transaction cannot be a closed transaction and not also be the parent
        let mut transaction_offset = self.latest_transaction_offset();
        loop {
            let transaction = self.get_open_transaction(transaction_offset);
            for write in &transaction.writes {
                if read_set.contains(&Read {
                    set_id: write.set_id,
                    value: write.data_key,
                }) {
                    return Ok(None);
                }
            }

            transaction_offset -= 1;

            if transaction_offset == tx.parent_tx_offset {
                break;
            }
        }

        Ok(Some(self.finalize(tx)?))
    }
}

impl TransactionalDB {
    fn finalize(&mut self, tx: Tx) -> Result<CommitResult, DBError> {
        // TODO: This is a gross hack, need a better way to do this.
        // Essentially what this is doing is searching the database to
        // see if any of the inserts in this tx are already in the database
        // so that we don't actually have to write any of the ones that are
        // already there. Probably better to do in the insert function, but
        // there are performance considerations.
        let writes = tx
            .writes
            .into_iter()
            .filter(|write| {
                let mut tx_temp = Tx {
                    parent_tx_offset: tx.parent_tx_offset,
                    writes: Vec::new(),
                    reads: Vec::new(),
                };
                match write.operation {
                    Operation::Delete => self.seek(&mut tx_temp, write.set_id, write.data_key).is_some(),
                    Operation::Insert => self.seek(&mut tx_temp, write.set_id, write.data_key).is_none(),
                }
            })
            .collect::<Vec<_>>();

        // Rebase on the last open transaction (or closed transaction if none open)
        let new_transaction = Transaction { writes };

        const COMMIT_SIZE: usize = 1;
        let mut commit_bytes = None;
        // TODO: avoid copy
        // TODO: use an estimated byte size to determine how much to put in here
        self.unwritten_commit.transactions.push(new_transaction.clone());
        if self.unwritten_commit.transactions.len() >= COMMIT_SIZE {
            commit_bytes = Some(self.generate_commit());
        }

        // TODO: avoid copy
        self.open_transactions.push(new_transaction.clone());
        self.open_transaction_offsets.push(tx.parent_tx_offset + 1);

        // Remove my branch
        let index = self
            .branched_transaction_offsets
            .iter()
            .position(|offset| *offset == tx.parent_tx_offset)
            .unwrap();
        self.branched_transaction_offsets.swap_remove(index);

        if tx.parent_tx_offset == self.closed_transaction_offset {
            self.vacuum_open_transactions();
        }

        Ok(CommitResult {
            tx: new_transaction,
            commit_bytes,
        })
    }

    fn vacuum_open_transactions(&mut self) {
        loop {
            // If someone branched off of the closed transaction, we're done otherwise continue
            if self
                .branched_transaction_offsets
                .contains(&self.closed_transaction_offset)
            {
                break;
            }

            // No one is branched off of the closed transaction so close the first open
            // transaction and make it the new closed transaction
            let first_open_offset = self.open_transaction_offsets.first().copied();
            if let Some(first_open_offset) = first_open_offset {
                // Assumes open transactions are deleted from the beginning
                let first_open = self.open_transactions.remove(0);
                self.open_transaction_offsets.remove(0);
                self.closed_transaction_offset = first_open_offset;

                for write in &first_open.writes {
                    if write.operation.to_u8() == 0 {
                        self.closed_state.delete(write.set_id, write.data_key);
                    } else {
                        self.closed_state.insert(write.set_id, write.data_key);
                    }
                }
            } else {
                // No more transactions to process
                break;
            }
        }
    }

    fn generate_commit(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.unwritten_commit.encode(&mut bytes);

        let parent_commit_hash = Some(hash_bytes(&bytes));

        let commit_offset = self.unwritten_commit.commit_offset + 1;
        let num_tx = self.unwritten_commit.transactions.len();
        let min_tx_offset = self.unwritten_commit.min_tx_offset + num_tx as u64;

        self.unwritten_commit = Commit {
            parent_commit_hash,
            commit_offset,
            min_tx_offset,
            transactions: Vec::new(),
        };
        bytes
    }

    pub fn from_data_key<T, F: Fn(&[u8]) -> T>(&self, data_key: &DataKey, f: F) -> Option<T> {
        let data = match data_key {
            DataKey::Data(data) => Some(f(&data)),
            DataKey::Hash(hash) => {
                let odb = self.odb.lock().unwrap();
                let t = f(odb.get(Hash::from_arr(&hash.data)).unwrap().to_vec().as_slice());
                Some(t)
            }
        };
        data
    }

    pub fn seek(&self, tx: &mut Tx, set_id: u32, data_key: DataKey) -> Option<Vec<u8>> {
        let mut measure = HistogramHandle::new(&TDB_SEEK_TIME);

        // Start timing this whole function; `Drop` will measure time spent.
        measure.start();

        // I'm not sure if this just needs to track reads from the parent transaction
        // or reads from the transaction as well.
        // TODO: Replace this with relation, page, row level SIREAD locks
        // SEE: https://www.interdb.jp/pg/pgsql05.html
        tx.reads.push(Read {
            set_id,
            value: data_key,
        });

        // Even uncommitted objects will be in the odb. This will accumulate garbage over time,
        // but we could also clear it if a transaction fails (or store uncommited changes in a different odb).
        let data = self.from_data_key(&data_key, |data| data.to_vec());

        // Search back through this transaction
        for i in (0..tx.writes.len()).rev() {
            let write = &tx.writes[i];
            if write.operation.to_u8() == 0 {
                if set_id == write.set_id && write.data_key == data_key {
                    return None;
                }
            } else if set_id == write.set_id && write.data_key == data_key {
                return Some(data.unwrap());
            }
        }

        // Search backwards through all open transactions that are parents of this transaction.
        // if you find a delete it's not there.
        // if you find an insert it is there. If you find no mention of it, then whether
        // it's there or not is dependent on the closed_state.
        let mut next_open_offset = tx.parent_tx_offset;
        loop {
            if next_open_offset == self.closed_transaction_offset {
                break;
            }
            let next_open = self.get_open_transaction(next_open_offset);
            for write in &next_open.writes {
                if write.operation.to_u8() == 0 {
                    if set_id == write.set_id && write.data_key == data_key {
                        return None;
                    }
                } else if set_id == write.set_id && write.data_key == data_key {
                    return Some(data.unwrap());
                }
            }
            next_open_offset -= 1;
        }

        if self.closed_state.contains(set_id, data_key) {
            return Some(data.unwrap());
        }

        None
    }

    pub fn scan<'a>(&'a self, tx: &'a mut Tx, set_id: u32) -> ScanIter<'a> {
        let tx_writes_index = tx.writes.len() as i32 - 1;
        ScanIter {
            txdb: self,
            tx,
            set_id,
            scanned: HashSet::new(),
            scan_stage: Some(ScanStage::CurTx { index: tx_writes_index }),
        }
    }

    pub fn delete(&mut self, tx: &mut Tx, set_id: u32, data_key: DataKey) {
        let mut measure = HistogramHandle::new(&TDB_DELETE_TIME);

        // Start timing this whole function; `Drop` will measure time spent.
        measure.start();

        // Search backwards in the transaction:
        // if not there: add delete
        // if delete there: do nothing
        // if insert there: replace with delete
        let mut found = false;
        for i in (0..tx.writes.len()).rev() {
            let write = tx.writes[i];
            if write.operation.to_u8() == 0 {
                if set_id == write.set_id && write.data_key == data_key {
                    found = true;
                    break;
                }
            } else if set_id == write.set_id && write.data_key == data_key {
                found = true;
                tx.writes[i] = Write {
                    operation: Operation::Delete,
                    set_id,
                    data_key,
                };
                break;
            }
        }
        if !found {
            tx.writes.push(Write {
                operation: Operation::Delete,
                set_id,
                data_key,
            });
        }
    }

    pub fn insert(&mut self, tx: &mut Tx, set_id: u32, bytes: Vec<u8>) -> DataKey {
        let mut measure = HistogramHandle::new(&TDB_INSERT_TIME);

        // Start timing this whole function; `Drop` will measure time spent.
        measure.start();

        let value = match data_key::InlineData::from_bytes(&bytes) {
            Some(inline) => DataKey::Data(inline),
            None => {
                let mut odb = self.odb.lock().unwrap();
                let hash = odb.add(bytes);
                DataKey::Hash(spacetimedb_lib::Hash::from_arr(&hash.data))
            }
        };

        // Search backwards in the transaction:
        // if not there: add insert
        // if delete there: overwrite as insert
        // if insert there: do nothing
        let mut found = false;
        for i in (0..tx.writes.len()).rev() {
            let write = tx.writes[i];
            if write.operation.to_u8() == 0 {
                if set_id == write.set_id && write.data_key == value {
                    found = true;
                    tx.writes[i] = Write {
                        operation: Operation::Insert,
                        set_id,
                        data_key: value,
                    };
                    break;
                }
            } else if set_id == write.set_id && write.data_key == value {
                found = true;
                break;
            }
        }

        if !found {
            tx.writes.push(Write {
                operation: Operation::Insert,
                set_id,
                data_key: value,
            });
        }

        value
    }
}

pub struct ScanIter<'a> {
    txdb: &'a TransactionalDB,
    tx: &'a mut Tx,
    set_id: u32,
    scanned: HashSet<DataKey>,
    scan_stage: Option<ScanStage<'a>>,
}

enum ScanStage<'a> {
    CurTx {
        index: i32,
    },
    OpenTransactions {
        transaction_offset: u64,
        write_index: Option<i32>,
    },
    ClosedSet(std::collections::hash_set::Iter<'a, DataKey>),
}

impl<'a> Iterator for ScanIter<'a> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut measure = HistogramHandle::new(&TDB_SCAN_TIME);

        // Start timing this whole function; `Drop` will measure time spent.
        measure.start();

        let scanned = &mut self.scanned;
        let set_id = self.set_id;

        loop {
            match self.scan_stage.take() {
                Some(ScanStage::CurTx { index }) => {
                    if index == -1 {
                        self.scan_stage = Some(ScanStage::OpenTransactions {
                            transaction_offset: self.tx.parent_tx_offset,
                            write_index: None,
                        });
                        continue;
                    } else {
                        self.scan_stage = Some(ScanStage::CurTx { index: index - 1 });
                    }
                    // Search back through this transaction
                    let write = &self.tx.writes[index as usize];
                    match write.operation {
                        Operation::Insert => {
                            if write.set_id == set_id {
                                self.tx.reads.push(Read {
                                    set_id: write.set_id,
                                    value: write.data_key,
                                });
                                let data = self.txdb.from_data_key(&write.data_key, |data| data.to_vec());
                                scanned.insert(write.data_key);
                                return Some(data.unwrap());
                            }
                        }
                        Operation::Delete => {
                            if write.set_id == set_id {
                                self.tx.reads.push(Read {
                                    set_id: write.set_id,
                                    value: write.data_key,
                                });
                                scanned.insert(write.data_key);
                            }
                        }
                    };
                }
                Some(ScanStage::OpenTransactions {
                    transaction_offset,
                    write_index,
                }) => {
                    // Search backwards through all open transactions that are parents of this transaction.
                    // if you find a delete it's not there.
                    // if you find an insert it is there. If you find no mention of it, then whether
                    // it's there or not is dependent on the closed_state.

                    // There are no open transactions, go next
                    if self.txdb.closed_transaction_offset == transaction_offset {
                        if let Some(closed_set) = self.txdb.closed_state.iter(set_id) {
                            self.scan_stage = Some(ScanStage::ClosedSet(closed_set));
                        } else {
                            return None;
                        }
                        continue;
                    }

                    // Loop through cur transaction
                    let transaction = self.txdb.get_open_transaction(transaction_offset);
                    let mut write_index = if let Some(write_index) = write_index {
                        write_index
                    } else {
                        transaction.writes.len() as i32 - 1
                    };
                    loop {
                        if write_index == -1 {
                            break;
                        }

                        let write = &transaction.writes[write_index as usize];
                        let opt_index = Some(write_index - 1);
                        match write.operation {
                            Operation::Insert => {
                                if write.set_id == set_id && !scanned.contains(&write.data_key) {
                                    self.tx.reads.push(Read {
                                        set_id: write.set_id,
                                        value: write.data_key,
                                    });
                                    let data = self.txdb.from_data_key(&write.data_key, |data| data.to_vec()).unwrap();
                                    scanned.insert(write.data_key);
                                    self.scan_stage = Some(ScanStage::OpenTransactions {
                                        transaction_offset,
                                        write_index: opt_index,
                                    });
                                    return Some(data);
                                }
                            }
                            Operation::Delete => {
                                if write.set_id == set_id {
                                    self.tx.reads.push(Read {
                                        set_id: write.set_id,
                                        value: write.data_key,
                                    });
                                    scanned.insert(write.data_key);
                                }
                            }
                        }

                        write_index -= 1;
                    }

                    // Move to next transaction
                    self.scan_stage = Some(ScanStage::OpenTransactions {
                        transaction_offset: transaction_offset - 1,
                        write_index: None,
                    });
                    continue;
                }
                Some(ScanStage::ClosedSet(mut closed_set_iter)) => {
                    let v = closed_set_iter.next();
                    if let Some(v) = v {
                        self.scan_stage = Some(ScanStage::ClosedSet(closed_set_iter));
                        if !scanned.contains(v) {
                            self.tx.reads.push(Read { set_id, value: *v });
                            let data = self.txdb.from_data_key(v, |data| data.to_vec()).unwrap();
                            return Some(data);
                        }
                    } else {
                        return None;
                    }
                }
                None => {
                    return None;
                }
            }
        }
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

        let mut scan_1 = db.scan(tx_1, 0).map(|b| b.to_owned()).collect::<Vec<Vec<u8>>>();
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

        let mut tx_2 = TxWrapper::begin(&db);
        let row = db.borrow_mut().seek(&mut tx_2, 0, row_key_1);
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
        let row = db.borrow_mut().seek(&mut tx_2, 0, row_key_1);
        assert!(row.is_some());
        db.borrow_mut().delete(&mut tx_2, 0, row_key_2);

        let mut tx_3 = TxWrapper::begin(&db);
        db.borrow_mut().delete(&mut tx_3, 0, row_key_1);

        assert!(tx_2.commit()?.is_some());
        assert!(tx_3.commit()?.is_some());
        Ok(())
    }
}
