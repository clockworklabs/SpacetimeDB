use super::{
    message_log::MessageLog,
    messages::{
        commit::Commit,
        transaction::Transaction,
        write::{Operation, Value, Write},
    },
    object_db::ObjectDB,
};
use crate::hash::{hash_bytes, Hash};
use std::{
    collections::{hash_set::Iter, HashMap, HashSet},
    path::Path,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct Read {
    set_id: u32,
    value: Value,
}

#[derive(Debug, Clone)]
pub struct Tx {
    parent_tx_offset: u64,
    writes: Vec<Write>,
    reads: Vec<Read>,
}

// TODO: implement some kind of tag/dataset/namespace system
// which allows the user to restrict the search for data
// to a tag or set of tags. Honestly, maybe forget the content
// addressing and just use a map like CockroachDB, idk.
pub struct ClosedHashSet {
    set: HashSet<Value>,
}

impl ClosedHashSet {
    fn contains(&self, value: Value) -> bool {
        let set = &self.set;
        set.contains(&value)
    }

    fn insert(&mut self, value: Value) {
        let set = &mut self.set;
        set.insert(value);
    }

    fn delete(&mut self, value: Value) {
        let set = &mut self.set;
        set.remove(&value);
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

    fn contains(&self, set_id: u32, value: Value) -> bool {
        if let Some(set) = self.hash_sets.get(&set_id) {
            set.contains(value)
        } else {
            false
        }
    }

    fn insert(&mut self, set_id: u32, value: Value) {
        if let Some(set) = self.hash_sets.get_mut(&set_id) {
            set.insert(value)
        } else {
            let mut set = HashSet::new();
            set.insert(value);
            self.hash_sets.insert(set_id, ClosedHashSet { set });
        }
    }

    fn delete(&mut self, set_id: u32, value: Value) {
        let sets = &mut self.hash_sets;
        if let Some(set) = sets.get_mut(&set_id) {
            // Not atomic, but also correct
            set.delete(value);
            if set.len() == 0 {
                drop(set);
                sets.remove(&set_id);
            }
        } else {
            // Do nothing
        }
    }

    fn iter(&self, set_id: u32) -> Option<Iter<Value>> {
        let sets = &self.hash_sets;
        if let Some(set) = sets.get(&set_id) {
            Some(set.set.iter())
        } else {
            None
        }
    }
}

pub struct TransactionalDB {
    pub odb: ObjectDB,
    pub message_log: MessageLog,
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
    pub fn open(root: &Path) -> Result<Self, anyhow::Error> {
        let odb = ObjectDB::open(root.to_path_buf().join("odb"))?;
        let message_log = MessageLog::open(root.to_path_buf().join("mlog"))?;

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
                        closed_state.delete(write.set_id, write.value);
                    } else {
                        closed_state.insert(write.set_id, write.value);
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
            message_log,
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

    fn latest_transaction_offset(&self) -> u64 {
        self.open_transaction_offsets
            .last()
            .map(|h| *h)
            .unwrap_or(self.closed_transaction_offset)
    }

    fn get_open_transaction(&self, offset: u64) -> &Transaction {
        let index = (offset - self.closed_transaction_offset) - 1;
        // Assumes open transactions are deleted from the beginning
        &self.open_transactions[index as usize]
    }

    pub fn begin_tx(&mut self) -> Tx {
        let parent = self.latest_transaction_offset();
        self.branched_transaction_offsets.push(parent);
        Tx {
            parent_tx_offset: parent,
            reads: Vec::new(),
            writes: Vec::new(),
        }
    }

    pub fn rollback_tx(&mut self, tx: Tx) {
        // Remove my branch
        let index = self
            .branched_transaction_offsets
            .iter()
            .position(|offset| *offset == tx.parent_tx_offset)
            .unwrap();
        self.branched_transaction_offsets.swap_remove(index);
        self.vacuum_open_transactions();
    }

    pub fn commit_tx(&mut self, tx: Tx) -> bool {
        if self.latest_transaction_offset() == tx.parent_tx_offset {
            self.finalize(tx);
            return true;
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
                    value: write.value,
                }) {
                    return false;
                }
            }

            transaction_offset -= 1;

            if transaction_offset == tx.parent_tx_offset {
                break;
            }
        }

        self.finalize(tx);
        true
    }

    fn finalize(&mut self, tx: Tx) {
        // Rebase on the last open transaction (or closed transaction if none open)
        let new_transaction = Transaction { writes: tx.writes };

        const COMMIT_SIZE: usize = 1;
        // TODO: avoid copy
        // TODO: use an estimated byte size to determine how much to put in here
        self.unwritten_commit.transactions.push(new_transaction.clone());
        if self.unwritten_commit.transactions.len() > COMMIT_SIZE {
            self.persist_commit();
        }

        self.open_transactions.push(new_transaction);
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
            let first_open_offset = self.open_transaction_offsets.first().map(|h| *h);
            if let Some(first_open_offset) = first_open_offset {
                // Assumes open transactions are deleted from the beginning
                let first_open = self.open_transactions.remove(0);
                self.open_transaction_offsets.remove(0);
                self.closed_transaction_offset = first_open_offset;

                for write in &first_open.writes {
                    if write.operation.to_u8() == 0 {
                        self.closed_state.delete(write.set_id, write.value);
                    } else {
                        self.closed_state.insert(write.set_id, write.value);
                    }
                }
            } else {
                // No more transactions to process
                break;
            }
        }
    }

    fn persist_commit(&mut self) {
        let mut bytes = Vec::new();
        self.unwritten_commit.encode(&mut bytes);

        let parent_commit_hash = Some(hash_bytes(&bytes));
        self.message_log.append(bytes).unwrap();

        let commit_offset = self.unwritten_commit.commit_offset + 1;
        let num_tx = self.unwritten_commit.transactions.len();
        let min_tx_offset = self.unwritten_commit.min_tx_offset + num_tx as u64;

        self.unwritten_commit = Commit {
            parent_commit_hash,
            commit_offset,
            min_tx_offset,
            transactions: Vec::new(),
        }
    }

    // TODO: copies
    pub fn data_from_value(&self, value: Value) -> Option<Vec<u8>> {
        let data = match value {
            Value::Data { len, buf } => Some(buf[0..(len as usize)].to_vec()),
            Value::Hash(hash) => Some(self.odb.get(hash).unwrap().to_vec()),
        };
        data
    }

    pub fn seek(&self, tx: &mut Tx, set_id: u32, value: Value) -> Option<Vec<u8>> {
        // I'm not sure if this just needs to track reads from the parent transaction
        // or reads from the transaction as well.
        // TODO: Replace this with relation, page, row level SIREAD locks
        // SEE: https://www.interdb.jp/pg/pgsql05.html
        tx.reads.push(Read { set_id, value });

        // Even uncommitted objects will be in the odb. This will accumulate garbage over time,
        // but we could also clear it if a transaction fails (or store uncommited changes in a different odb).
        let data = self.data_from_value(value);

        // Search back through this transaction
        for i in (0..tx.writes.len()).rev() {
            let write = &tx.writes[i];
            if write.operation.to_u8() == 0 {
                if set_id == write.set_id && write.value == value {
                    return None;
                }
            } else {
                if set_id == write.set_id && write.value == value {
                    return Some(data.unwrap());
                }
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
                    if set_id == write.set_id && write.value == value {
                        return None;
                    }
                } else {
                    if set_id == write.set_id && write.value == value {
                        return Some(data.unwrap());
                    }
                }
            }
            next_open_offset -= 1;
        }

        if self.closed_state.contains(set_id, value) {
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

    pub fn delete(&mut self, tx: &mut Tx, set_id: u32, value: Value) {
        // Search backwards in the transaction:
        // if not there: add delete
        // if delete there: do nothing
        // if insert there: replace with delete
        let mut found = false;
        for i in (0..tx.writes.len()).rev() {
            let write = tx.writes[i];
            if write.operation.to_u8() == 0 {
                if set_id == write.set_id && write.value == value {
                    found = true;
                    break;
                }
            } else {
                if set_id == write.set_id && write.value == value {
                    found = true;
                    tx.writes[i] = Write {
                        operation: Operation::Delete,
                        set_id,
                        value,
                    };
                    break;
                }
            }
        }
        if !found {
            tx.writes.push(Write {
                operation: Operation::Delete,
                set_id,
                value,
            });
        }
    }

    pub fn insert(&mut self, tx: &mut Tx, set_id: u32, bytes: Vec<u8>) -> Value {
        let value = if bytes.len() > 32 {
            let hash = self.odb.add(bytes);
            Value::Hash(hash)
        } else {
            let mut buf = [0; 32];
            buf[0..bytes.len()].copy_from_slice(&bytes[0..bytes.len()]);
            Value::Data {
                len: bytes.len() as u8,
                buf,
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
                if set_id == write.set_id && write.value == value {
                    found = true;
                    tx.writes[i] = Write {
                        operation: Operation::Insert,
                        set_id,
                        value,
                    };
                    break;
                }
            } else {
                if set_id == write.set_id && write.value == value {
                    found = true;
                    break;
                }
            }
        }

        if !found {
            tx.writes.push(Write {
                operation: Operation::Insert,
                set_id,
                value,
            });
        }

        value
    }
}

pub struct ScanIter<'a> {
    txdb: &'a TransactionalDB,
    tx: &'a mut Tx,
    set_id: u32,
    scanned: HashSet<Value>,
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
    ClosedSet(std::collections::hash_set::Iter<'a, Value>),
}

impl<'a> Iterator for ScanIter<'a> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
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
                                    value: write.value,
                                });
                                let data = self.txdb.data_from_value(write.value);
                                scanned.insert(write.value);
                                return Some(data.unwrap());
                            }
                        }
                        Operation::Delete => {
                            if write.set_id == set_id {
                                self.tx.reads.push(Read {
                                    set_id: write.set_id,
                                    value: write.value,
                                });
                                scanned.insert(write.value);
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
                    loop {
                        let transaction = self.txdb.get_open_transaction(transaction_offset);
                        let write_index = if let Some(write_index) = write_index {
                            write_index
                        } else {
                            transaction.writes.len() as i32 - 1
                        };

                        if write_index == -1 {
                            break;
                        }

                        let index = write_index as usize;
                        let write = &transaction.writes[index];
                        let opt_index = Some(write_index - 1);
                        match write.operation {
                            Operation::Insert => {
                                if write.set_id == set_id && !scanned.contains(&write.value) {
                                    self.tx.reads.push(Read {
                                        set_id: write.set_id,
                                        value: write.value,
                                    });
                                    let data = self.txdb.data_from_value(write.value).unwrap();
                                    scanned.insert(write.value);
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
                                        value: write.value,
                                    });
                                    scanned.insert(write.value);
                                }
                            }
                        }
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
                            let data = self.txdb.data_from_value(*v).unwrap();
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
    use tempdir::TempDir;

    use super::TransactionalDB;
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
    fn test_insert_and_seek_bytes() {
        let tmp_dir = TempDir::new("txdb_test").unwrap();
        let mut db = TransactionalDB::open(tmp_dir.path()).unwrap();
        let mut tx = db.begin_tx();
        let row_key_1 = db.insert(&mut tx, 0, b"this is a byte string".to_vec());
        db.commit_tx(tx);

        let mut tx = db.begin_tx();
        let row = db.seek(&mut tx, 0, row_key_1).unwrap();

        assert_eq!(b"this is a byte string", row.as_slice());
    }

    #[test]
    fn test_insert_and_seek_struct() {
        let tmp_dir = TempDir::new("txdb_test").unwrap();
        let mut db = TransactionalDB::open(tmp_dir.path()).unwrap();
        let mut tx = db.begin_tx();
        let row_key_1 = db.insert(
            &mut tx,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -1,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );
        db.commit_tx(tx);

        let mut tx = db.begin_tx();
        let row = MyStruct::decode(db.seek(&mut tx, 0, row_key_1).unwrap().as_slice());

        let i = row.my_i32;
        assert_eq!(i, -1);
    }

    #[test]
    fn test_read_isolation() {
        let tmp_dir = TempDir::new("txdb_test").unwrap();
        let mut db = TransactionalDB::open(tmp_dir.path()).unwrap();
        let mut tx_1 = db.begin_tx();
        let row_key_1 = db.insert(
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

        let mut tx_2 = db.begin_tx();
        let row = db.seek(&mut tx_2, 0, row_key_1);
        assert!(row.is_none());

        db.commit_tx(tx_1);

        let mut tx_3 = db.begin_tx();
        let row = db.seek(&mut tx_3, 0, row_key_1);
        assert!(row.is_some());
    }

    #[test]
    fn test_scan() {
        let tmp_dir = TempDir::new("txdb_test").unwrap();
        let mut db = TransactionalDB::open(tmp_dir.path()).unwrap();
        let mut tx_1 = db.begin_tx();
        let _row_key_1 = db.insert(
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
        let _row_key_2 = db.insert(
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

        let mut scan_1 = db.scan(&mut tx_1, 0).map(|b| b.to_owned()).collect::<Vec<Vec<u8>>>();
        scan_1.sort();

        db.commit_tx(tx_1);

        let mut tx_2 = db.begin_tx();
        let mut scan_2 = db.scan(&mut tx_2, 0).collect::<Vec<Vec<u8>>>();
        scan_2.sort();

        assert_eq!(scan_1.len(), scan_2.len());

        for (i, _) in scan_1.iter().enumerate() {
            let val_1 = &scan_1[i];
            let val_2 = &scan_2[i];
            for i in 0..val_1.len() {
                assert_eq!(val_1[i], val_2[i]);
            }
        }
    }

    #[test]
    fn test_write_skew_conflict() {
        let tmp_dir = TempDir::new("txdb_test").unwrap();
        let mut db = TransactionalDB::open(tmp_dir.path()).unwrap();
        let mut tx_1 = db.begin_tx();
        let row_key_1 = db.insert(
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

        let mut tx_2 = db.begin_tx();
        let row = db.seek(&mut tx_2, 0, row_key_1);
        assert!(row.is_none());

        assert!(db.commit_tx(tx_1));
        assert!(!db.commit_tx(tx_2));
    }

    #[test]
    fn test_write_skew_no_conflict() {
        let tmp_dir = TempDir::new("txdb_test").unwrap();
        let mut db = TransactionalDB::open(tmp_dir.path()).unwrap();
        let mut tx_1 = db.begin_tx();
        let row_key_1 = db.insert(
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
        let row_key_2 = db.insert(
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
        assert!(db.commit_tx(tx_1));

        let mut tx_2 = db.begin_tx();
        let row = db.seek(&mut tx_2, 0, row_key_1);
        assert!(row.is_some());
        db.delete(&mut tx_2, 0, row_key_2);

        let mut tx_3 = db.begin_tx();
        db.delete(&mut tx_3, 0, row_key_1);

        assert!(db.commit_tx(tx_2));
        assert!(db.commit_tx(tx_3));
    }

    #[test]
    fn test_size() {
        let tmp_dir = TempDir::new("txdb_test").unwrap();
        let mut db = TransactionalDB::open(tmp_dir.path()).unwrap();
        let start = std::time::Instant::now();
        let iterations: u128 = 1000;
        println!("{} odb base size bytes", db.odb.total_mem_size_bytes());

        let mut raw_data_size = 0;
        for i in 0..iterations {
            let mut tx_1 = db.begin_tx();
            let val_1 = MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -(i as i32),
                my_u64: i as u64,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode();
            let val_2 = MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -2 * (i as i32),
                my_u64: i as u64,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode();

            raw_data_size += val_1.len() as u64;
            raw_data_size += val_2.len() as u64;

            db.insert(&mut tx_1, 0, val_1);
            db.insert(&mut tx_1, 0, val_2);

            assert!(db.commit_tx(tx_1));
        }
        let duration = start.elapsed();
        println!("{} odb after size bytes", db.odb.total_mem_size_bytes());

        // each key is this long: "qwertyuiopasdfghjklzxcvbnm123456";
        // key x2: 64 bytes
        // commit key: 32 bytes
        // commit: 98 bytes <parent(32)><write(<type(1)><hash(32)>)><write(<type(1)><hash(32)>)>
        // total: 194
        let data_overhead = db.odb.total_mem_size_bytes() - raw_data_size;
        println!("{} overhead bytes per tx", data_overhead / iterations as u64);
        println!("{} us per tx", duration.as_micros() / iterations);
    }
}
