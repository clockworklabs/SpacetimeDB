use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use parking_lot::{RwLock, RwLockReadGuard};
use spacetimedb_lib::DataKey;

use crate::db::datastore::Result;
use crate::db::messages::write::Operation;
use crate::db::messages::{transaction::Transaction, write::Write};

use super::{
    freelist_allocator::{Allocator, Handle},
    memory::{Blob, BlobRef, Memory},
    traits::{self, TableId},
};

pub struct ScanIterator<'a> {
    memory: &'a Memory,
    table_id: TableId,
    tx: &'a RefCell<Tx>,
    sausage: RwLockReadGuard<'a, Sausage>,
    scanned: HashSet<DataKey>,
    stage: ScanStage<'a>,
}

enum ScanStage<'a> {
    CurTx {
        iter: std::collections::btree_map::Iter<'a, RowId, Operation>,
    },
    UnsquashedTransactions {
        transaction_offset: u64,
        write_index: Option<i32>,
    },
    SquashedSet(DataKeys),
    Done,
}

struct DataKeys {
    data_keys: Vec<DataKey>,
    idx: usize,
}

impl DataKeys {
    fn new(data_keys: Vec<DataKey>) -> Self {
        Self { data_keys, idx: 0 }
    }
}

impl Iterator for DataKeys {
    type Item = DataKey;

    fn next(&mut self) -> Option<Self::Item> {
        match self.data_keys.get(self.idx) {
            Some(data_key) => {
                self.idx += 1;
                Some(*data_key)
            }
            None => None,
        }
    }
}

impl<'a> Iterator for ScanIterator<'a> {
    type Item = BlobRef;

    fn next(&mut self) -> Option<BlobRef> {
        loop {
            match std::mem::replace(&mut self.stage, ScanStage::Done) {
                ScanStage::CurTx { mut iter } => {
                    while let Some((row_id, op)) = iter.next() {
                        if row_id.table_id.0 != self.table_id.0 {
                            continue;
                        }
                        match op {
                            Operation::Insert => {
                                if !self.scanned.contains(&row_id.data_key) {
                                    self.tx.borrow_mut().reads.insert(*row_id);
                                    self.scanned.insert(row_id.data_key);
                                    self.stage = ScanStage::CurTx { iter };
                                    return Some(self.memory.get(&row_id.data_key));
                                }
                            }
                            Operation::Delete => {
                                self.tx.borrow_mut().reads.insert(*row_id);
                                self.scanned.insert(row_id.data_key);
                            }
                        }
                    }
                    self.stage = ScanStage::UnsquashedTransactions {
                        transaction_offset: self.tx.borrow().parent_tx_offset,
                        write_index: None,
                    };
                    continue;
                }
                ScanStage::UnsquashedTransactions {
                    transaction_offset,
                    write_index,
                } => {
                    // Search backwards through all unsquashed transactions that are parents of this transaction.
                    // if you find a delete it's not there.
                    // if you find an insert it is there. If you find no mention of it, then whether
                    // it's there or not is dependent on the squashed_state.

                    // There are no unsquashed transactions, go next
                    let transaction = {
                        match self.sausage.scan(transaction_offset, self.table_id) {
                            SausageScan::SquashedIter(squashed_set) => {
                                self.stage = ScanStage::SquashedSet(DataKeys::new(squashed_set));
                                continue;
                            }
                            SausageScan::Done => return None,
                            SausageScan::UnsquashedTx(transaction) => transaction,
                        }
                    };

                    // Loop through cur transaction
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
                        if write.set_id == self.table_id.0 {
                            match write.operation {
                                Operation::Insert => {
                                    if !self.scanned.contains(&write.data_key) {
                                        self.tx.borrow_mut().reads.insert(RowId {
                                            table_id: self.table_id,
                                            data_key: write.data_key,
                                        });
                                        self.scanned.insert(write.data_key);
                                        self.stage = ScanStage::UnsquashedTransactions {
                                            transaction_offset,
                                            write_index: opt_index,
                                        };
                                        return Some(self.memory.get(&write.data_key));
                                    }
                                }
                                Operation::Delete => {
                                    self.tx.borrow_mut().reads.insert(RowId {
                                        table_id: self.table_id,
                                        data_key: write.data_key,
                                    });
                                    self.scanned.insert(write.data_key);
                                }
                            }
                        }

                        write_index -= 1;
                    }

                    // Move to next transaction
                    self.stage = ScanStage::UnsquashedTransactions {
                        transaction_offset: transaction_offset - 1,
                        write_index: None,
                    };
                    continue;
                }
                ScanStage::SquashedSet(mut squashed_set_iter) => {
                    if let Some(v) = squashed_set_iter.next() {
                        self.stage = ScanStage::SquashedSet(squashed_set_iter);
                        if !self.scanned.contains(&v) {
                            self.tx.borrow_mut().reads.insert(RowId {
                                table_id: self.table_id,
                                data_key: v,
                            });
                            return Some(self.memory.get(&v));
                        }
                    } else {
                        return None;
                    }
                }
                ScanStage::Done => {
                    return None;
                }
            }
        }
    }
}

// TODO: implement some kind of tag/dataset/namespace system
// which allows the user to restrict the search for data
// to a tag or set of tags. Honestly, maybe forget the content
// addressing and just use a map like CockroachDB, idk.
struct SquashedTable {
    set: HashSet<DataKey>,
}

impl SquashedTable {
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
struct SquashedState {
    tables: HashMap<TableId, SquashedTable>,
}

impl SquashedState {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { tables: HashMap::new() }
    }

    fn contains(&self, row_id: &RowId) -> bool {
        if let Some(set) = self.tables.get(&row_id.table_id) {
            set.contains(row_id.data_key)
        } else {
            false
        }
    }

    pub fn insert(&mut self, table_id: TableId, data_key: DataKey) {
        if let Some(set) = self.tables.get_mut(&table_id) {
            set.insert(data_key)
        } else {
            let mut set = HashSet::new();
            set.insert(data_key);
            self.tables.insert(table_id, SquashedTable { set });
        }
    }

    pub fn delete(&mut self, table_id: TableId, data_key: DataKey) {
        let sets = &mut self.tables;
        if let Some(set) = sets.get_mut(&table_id) {
            // Not atomic, but also correct
            set.delete(data_key);
            if set.len() == 0 {
                //TODO: Is this necessary?
                //drop(set);
                sets.remove(&table_id);
            }
        } else {
            // Do nothing
        }
    }

    fn iter(&self, table_id: TableId) -> Option<std::collections::hash_set::Iter<DataKey>> {
        let sets = &self.tables;
        sets.get(&table_id).map(|set| set.set.iter())
    }
}

struct UnsquashedState {
    // TODO: it may be possible to move all this logic directly
    // into the values themselves which might make it easier to
    // index unsquashed transaction values
    squashed_transaction_offset: u64,
    unsquashed_transactions: Vec<Arc<Transaction>>,
    branched_transaction_offsets: Vec<u64>,
}

impl UnsquashedState {
    fn new() -> Self {
        Self {
            squashed_transaction_offset: 0,
            unsquashed_transactions: Vec::new(),
            branched_transaction_offsets: Vec::new(),
        }
    }

    fn latest_transaction_offset(&self) -> u64 {
        self.squashed_transaction_offset + self.unsquashed_transactions.len() as u64
    }

    fn get_unsquashed_transaction(&self, offset: u64) -> Option<&Transaction> {
        if offset <= self.squashed_transaction_offset {
            log::warn!(
                "Tried to find an unsquashed transaction ({}) that occurred before the squashed transaction ({}).",
                offset,
                self.squashed_transaction_offset
            );
            return None;
        }
        let index = (offset - self.squashed_transaction_offset) - 1;
        // Assumes unsquashed transactions are deleted from the beginning
        Some(&self.unsquashed_transactions[index as usize])
    }

    fn first_unsquashed_transaction(&mut self) -> Option<Arc<Transaction>> {
        // If someone branched off of the squashed transaction, we're done otherwise continue
        if self
            .branched_transaction_offsets
            .contains(&self.squashed_transaction_offset)
        {
            return None;
        }

        if self.unsquashed_transactions.is_empty() {
            // No more transactions to process
            return None;
        }

        // No one is branched off of the squashed transaction so close the first unsquashed
        // transaction and make it the new squashed transaction
        // Assumes unsquashed transactions are deleted from the beginning
        let first_unsquashed = self.unsquashed_transactions.remove(0);
        self.squashed_transaction_offset += 1;
        Some(first_unsquashed)
    }

    fn tx(&self, transaction_offset: u64) -> Option<&Transaction> {
        if self.squashed_transaction_offset == transaction_offset {
            None
        } else {
            self.get_unsquashed_transaction(transaction_offset)
        }
    }

    fn contains(&self, row_id: &RowId, parent_tx_offset: u64) -> Option<bool> {
        // Search backwards through all unsquashed transactions that are parents of this transaction.
        // if you find a delete it's not there.
        // if you find an insert it is there. If you find no mention of it, then whether
        // it's there or not is dependent on the squashed_state.
        let mut next_unsquashed_offset = parent_tx_offset;
        while next_unsquashed_offset != self.squashed_transaction_offset {
            let next_unsquashed = self.get_unsquashed_transaction(next_unsquashed_offset).unwrap();
            for write in &next_unsquashed.writes {
                if row_id.table_id.0 == write.set_id && write.data_key == row_id.data_key {
                    match write.operation {
                        Operation::Delete => return Some(false),
                        Operation::Insert => return Some(true),
                    }
                }
            }
            next_unsquashed_offset -= 1;
        }

        None
    }

    fn branch(&mut self) -> Tx {
        if self.unsquashed_transactions.len() > 100 {
            log::warn!(
                "Open transactions len is {}. Be sure to commit or rollback transactions.",
                self.unsquashed_transactions.len()
            );
        }
        let parent_tx_offset = self.latest_transaction_offset();
        self.branched_transaction_offsets.push(parent_tx_offset);
        Tx {
            parent_tx_offset,
            reads: BTreeSet::new(),
            writes: BTreeMap::new(),
        }
    }

    fn commit(&mut self, tx: Tx) -> Option<Tx> {
        if self.latest_transaction_offset() == tx.parent_tx_offset {
            return Some(tx);
        }

        // If not, we need to merge.
        // - If I did not read something that someone else wrote to, we're good to just merge
        // in my changes, because the part of the DB I read from is still current.
        // - If I did read something someone else wrote to, then the issue is that I am
        // potentially basing my changes off of stuff that has changed since I last pulled.
        // I should pull and then try to reapply my changes based on the most current state
        // of the database (basically rebase off latest_transaction). Really this just means the
        // transaction has failed, and the transaction should be tried again with a new
        // parent transaction.
        // NOTE: latest transaction cannot be a squashed transaction and not also be the parent
        let mut transaction_offset = self.latest_transaction_offset();
        loop {
            let transaction = self.get_unsquashed_transaction(transaction_offset).unwrap();
            for write in &transaction.writes {
                if tx.reads.contains(&RowId {
                    table_id: TableId(write.set_id),
                    data_key: write.data_key,
                }) {
                    return None;
                }
            }

            transaction_offset -= 1;

            if transaction_offset == tx.parent_tx_offset {
                break;
            }
        }

        Some(tx)
    }

    fn rollback(&mut self, parent_tx_offset: u64) {
        // Remove my branch
        if let Some(index) = self
            .branched_transaction_offsets
            .iter()
            .position(|offset| *offset == parent_tx_offset)
        {
            self.branched_transaction_offsets.swap_remove(index);
        }
    }

    fn finalize(&mut self, writes: Vec<Write>, parent_tx_offset: u64) -> Result<(Arc<Transaction>, bool)> {
        // Rebase on the last unsquashed transaction (or squashed transaction if none unsquashed)
        let new_transaction = Arc::new(Transaction { writes });

        // TODO: avoid copy
        self.unsquashed_transactions.push(new_transaction.clone());

        // Remove my branch
        let index = self
            .branched_transaction_offsets
            .iter()
            .position(|offset| *offset == parent_tx_offset)
            .unwrap();
        self.branched_transaction_offsets.swap_remove(index);

        let should_vacuum = self.squashed_transaction_offset == parent_tx_offset;

        Ok((new_transaction, should_vacuum))
    }
}

enum SausageScan<'a> {
    SquashedIter(Vec<DataKey>),
    Done,
    UnsquashedTx(&'a Transaction),
}

// I had abbreviate "SquashedAndUnsquashed" to "saus" at some point in this refactor,
// and this just felt a fitting name for now.
struct Sausage {
    squashed_state: SquashedState,
    unsquashed_state: UnsquashedState,
}

impl Sausage {
    fn new() -> Self {
        Self {
            squashed_state: SquashedState::new(),
            unsquashed_state: UnsquashedState::new(),
        }
    }

    fn vacuum_unsquashed_transactions(&mut self) {
        while let Some(first_unsquashed) = self.unsquashed_state.first_unsquashed_transaction() {
            for write in &first_unsquashed.writes {
                match write.operation {
                    Operation::Delete => self.squashed_state.delete(TableId(write.set_id), write.data_key),
                    Operation::Insert => self.squashed_state.insert(TableId(write.set_id), write.data_key),
                }
            }
        }
    }

    fn scan(&self, transaction_offset: u64, set_id: TableId) -> SausageScan {
        match self.unsquashed_state.tx(transaction_offset) {
            Some(tx) => SausageScan::UnsquashedTx(tx),
            None => match self.squashed_state.iter(set_id) {
                Some(iter) => {
                    // TODO(cloutiertyler): @George, what's the reason that we
                    // need to collect the iter here?
                    let items = iter.copied().collect();
                    SausageScan::SquashedIter(items)
                }
                None => SausageScan::Done,
            },
        }
    }

    fn branch(&mut self) -> Tx {
        self.unsquashed_state.branch()
    }

    fn rollback(&mut self, tx: Tx) {
        self.unsquashed_state.rollback(tx.parent_tx_offset);
        self.vacuum_unsquashed_transactions();
    }

    fn finalize(&mut self, tx: Tx) -> Result<Arc<Transaction>> {
        // Essentially what this is doing is searching the database to
        // see if any of the inserts in this tx are already in the database
        // so that we don't actually have to write any of the ones that are
        // already there. Probably better to do in the insert function, but
        // there are performance considerations.
        let writes = tx
            .writes
            .into_iter()
            .filter(|(row_id, operation)| {
                // I'm not sure if this just needs to track reads from the parent transaction
                // or reads from the transaction as well.
                // TODO: Replace this with relation, page, row level SIREAD locks
                // SEE: https://www.interdb.jp/pg/pgsql05.html

                let get_row = self.contains(row_id, tx.parent_tx_offset);
                match operation {
                    Operation::Delete => get_row,
                    Operation::Insert => !get_row,
                }
            })
            .map(|(row_id, op)| Write {
                operation: op,
                set_id: row_id.table_id.0,
                data_key: row_id.data_key,
            })
            .collect::<Vec<_>>();

        let (tx, should_vacuum) = self.unsquashed_state.finalize(writes, tx.parent_tx_offset)?;

        if should_vacuum {
            self.vacuum_unsquashed_transactions();
        }

        Ok(tx)
    }

    fn commit(&mut self, tx: Tx) -> Result<Option<Arc<Transaction>>> {
        Ok(match self.unsquashed_state.commit(tx) {
            Some(tx) => Some(self.finalize(tx)?),
            None => None,
        })
    }

    fn contains(&self, row_id: &RowId, parent_tx_offset: u64) -> bool {
        if let Some(contains) = self.unsquashed_state.contains(row_id, parent_tx_offset) {
            return contains;
        }

        self.squashed_state.contains(row_id)
    }
}

enum GetRow {
    Deleted,
    Inserted,
    Absent(u64),
}

struct Txs {
    txs: Allocator<Tx>,
}

impl Txs {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { txs: Allocator::new() }
    }

    fn begin_mut_tx(&self, tx: Tx) -> TxId {
        TxId(self.txs.alloc(tx))
    }

    fn rollback_mut_tx(&self, tx: TxId) -> Tx {
        self.txs.free(tx.0)
    }

    fn commit_mut_tx(&self, tx: TxId) -> Tx {
        self.txs.free(tx.0)
    }

    fn scan_blobs_mut_tx<'a>(
        &'a self,
        tx: &'a TxId,
    ) -> (
        &RefCell<Tx>,
        std::collections::btree_map::Iter<'static, RowId, Operation>,
    ) {
        let handle = &tx.0;
        let tx = self.txs.get(handle);
        let tx_ref = tx.borrow_mut();
        let iter = tx_ref.writes.iter();
        let static_iter: std::collections::btree_map::Iter<'static, RowId, Operation>;
        unsafe {
            static_iter = std::mem::transmute(iter);
        }
        (tx, static_iter)
    }

    fn get_row_blob_mut_tx<'a>(&'a self, tx: &'a TxId, table_id: TableId, row_id: DataKey) -> GetRow {
        let data_key = row_id;
        let tx = self.txs.get(&tx.0);

        // println!("-Searching tx, read len {} write len {}", tx.borrow().reads.len(), tx.borrow().writes.len());

        // I'm not sure if this just needs to track reads from the parent transaction
        // or reads from the transaction as well.
        // TODO: Replace this with relation, page, row level SIREAD locks
        // SEE: https://www.interdb.jp/pg/pgsql05.html
        tx.borrow_mut().reads.insert(RowId { table_id, data_key });

        // Search this transaction
        let ref_tx = tx.borrow();
        let Some(op) = ref_tx.writes.get(&RowId { table_id, data_key }) else {
            return GetRow::Absent(ref_tx.parent_tx_offset);
        };

        match op {
            Operation::Delete => GetRow::Deleted,
            Operation::Insert => GetRow::Inserted,
        }
    }

    fn delete_row_blob_mut_tx<'a>(&'a self, tx: &'a TxId, table_id: TableId, row_id: DataKey) {
        let data_key = row_id;
        let tx = self.txs.get(&tx.0);

        // Search backwards in the transaction:
        // if not there: add delete
        // if delete there: do nothing
        // if insert there: replace with delete
        let writes = &mut tx.borrow_mut().writes;
        let Some(op) = writes.get_mut(&RowId { table_id, data_key }) else {
            writes.insert(RowId {
                table_id,
                data_key,
            }, Operation::Delete);
            return;
        };
        match op {
            Operation::Delete => (),
            Operation::Insert => *op = Operation::Delete,
        };
    }

    fn insert_row_blob_mut_tx<'a>(&'a self, tx: &'a TxId, table_id: TableId, data_key: DataKey) {
        let tx = self.txs.get(&tx.0);

        // Search backwards in the transaction:
        // if not there: add insert
        // if delete there: overwrite as insert
        // if insert there: do nothing
        let writes = &mut tx.borrow_mut().writes;
        let Some(op) = writes.get_mut(&RowId { table_id, data_key }) else {
            writes.insert(RowId {
                table_id,
                data_key,
            }, Operation::Insert);
            return;
        };
        match op {
            Operation::Delete => *op = Operation::Insert,
            Operation::Insert => (),
        };
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
struct RowId {
    table_id: TableId,
    data_key: DataKey,
}

struct Tx {
    parent_tx_offset: u64,
    writes: BTreeMap<RowId, Operation>,
    reads: BTreeSet<RowId>,
}

#[derive(Debug)]
pub struct TxId(Handle);

impl TableId {
    pub fn from_u32_for_testing(id: u32) -> Self {
        Self::raw(id)
    }

    pub fn raw(id: u32) -> Self {
        Self(id)
    }
}

pub struct Gitlike {
    txs: Txs,
    sausage: RwLock<Sausage>,
    memory: Memory,
}

impl Default for Gitlike {
    fn default() -> Self {
        Self::new()
    }
}

impl Gitlike {
    pub fn open_blobstore() -> impl traits::MutTxBlobstore<TableId = TableId> {
        Self::open()
    }

    pub fn open() -> Self {
        Self::new()
    }

    pub fn new() -> Self {
        Self {
            txs: Txs::new(),
            sausage: RwLock::new(Sausage::new()),
            memory: Memory::new(),
        }
    }

    pub fn from_data_key(&self, data_key: &DataKey) -> Option<BlobRef> {
        Some(self.memory.get(data_key))
    }
}

impl traits::BlobRow for Gitlike {
    type TableId = TableId;
    type RowId = DataKey;

    type Blob = Blob;
    type BlobRef = BlobRef;

    fn blob_to_owned(&self, blob_ref: Self::BlobRef) -> Self::Blob {
        self.memory.blob_to_owned(blob_ref)
    }
}

impl traits::Tx for Gitlike {
    type TxId = TxId;

    fn begin_tx(&self) -> Self::TxId {
        use traits::MutTx;
        self.begin_mut_tx()
    }

    fn release_tx(&self, tx: Self::TxId) {
        use traits::MutTx;
        self.rollback_mut_tx(tx)
    }
}

impl traits::TxBlobstore for Gitlike {
    type ScanIterator<'a> = ScanIterator<'a>
    where
        Self: 'a;

    fn scan_blobs_tx<'a>(&'a self, tx: &'a Self::TxId, table_id: TableId) -> Result<Self::ScanIterator<'a>> {
        use traits::MutTxBlobstore;
        self.scan_blobs_mut_tx(tx, table_id)
    }

    fn get_row_blob_tx<'a>(
        &'a self,
        tx: &'a Self::TxId,
        table_id: TableId,
        row_id: Self::RowId,
    ) -> Result<Option<Self::BlobRef>> {
        use traits::MutTxBlobstore;
        self.get_row_blob_mut_tx(tx, table_id, row_id)
    }
}

impl traits::MutTx for Gitlike {
    type MutTxId = TxId;

    fn begin_mut_tx(&self) -> Self::MutTxId {
        let branch = {
            let mut guard = self.sausage.write();
            guard.branch()
        };
        self.txs.begin_mut_tx(branch)
    }

    fn rollback_mut_tx(&self, tx: Self::MutTxId) {
        let branch = self.txs.rollback_mut_tx(tx);
        {
            let mut guard = self.sausage.write();
            guard.rollback(branch);
        }
    }

    fn commit_mut_tx(&self, tx: Self::MutTxId) -> Result<Option<Arc<Transaction>>> {
        let branch = self.txs.commit_mut_tx(tx);
        {
            let mut guard = self.sausage.write();
            guard.commit(branch)
        }
    }
}

impl traits::MutTxBlobstore for Gitlike {
    fn scan_blobs_mut_tx<'a>(
        &'a self,
        tx: &'a Self::MutTxId,
        table_id: TableId,
    ) -> super::Result<Self::ScanIterator<'a>> {
        let (tx, iter) = self.txs.scan_blobs_mut_tx(tx);
        Ok(ScanIterator {
            memory: &self.memory,
            table_id,
            tx,
            sausage: self.sausage.read(),
            scanned: HashSet::new(),
            stage: ScanStage::CurTx { iter },
        })
    }

    fn get_row_blob_mut_tx<'a>(
        &'a self,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        row_id: Self::RowId,
    ) -> super::Result<Option<BlobRef>> {
        let data_key = match self.txs.get_row_blob_mut_tx(tx, table_id, row_id) {
            GetRow::Deleted => None,
            GetRow::Inserted => Some(row_id),
            GetRow::Absent(parent_tx_offset) => {
                let guard = self.sausage.read();
                if guard.contains(
                    &RowId {
                        table_id,
                        data_key: row_id,
                    },
                    parent_tx_offset,
                ) {
                    Some(row_id)
                } else {
                    None
                }
            }
        };

        Ok(match data_key {
            Some(data_key) => self.from_data_key(&data_key),
            None => None,
        })
    }

    fn delete_row_blob_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTxId,
        table_id: TableId,
        row_id: Self::RowId,
    ) -> super::Result<()> {
        self.txs.delete_row_blob_mut_tx(tx, table_id, row_id);
        Ok(())
    }

    fn insert_row_blob_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTxId,
        table_id: TableId,
        row: &[u8],
    ) -> super::Result<Self::RowId> {
        let row_id = self.memory.insert(row);
        self.txs.insert_row_blob_mut_tx(tx, table_id, row_id);
        Ok(row_id)
    }
}
