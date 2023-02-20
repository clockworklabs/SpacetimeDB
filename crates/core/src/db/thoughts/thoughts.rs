

pub struct ClosedState {
    //open_set: BTreeSet<[u8; 46]>,
    page_sets: RwLock<HashMap<u32, ClosedPageSet>>,
    hash_sets: HashMap<u32, ClosedHashSet>,
}

impl ClosedState {
    fn contains(&self, set_id: u32, value: Value) -> bool {
        // TODO: thoughts
        // let mut min_bytes = Vec::with_capacity(46);
        // let min_record = OpenSetRecord {
        //     write: Write { operation: Operation::Delete, set_id, value },
        //     tx_id: 0,
        // };
        // min_record.encode(&mut min_bytes);
        // let min_bytes_buf = [0; 46];
        // min_bytes_buf.copy_from_slice(&min_bytes);

        // let max_bytes = Vec::with_capacity(46);
        // let max_record = OpenSetRecord {
        //     write: Write { operation: Operation::Insert, set_id, value },
        //     tx_id: u64::max_value(),
        // };
        // max_record.encode(&mut max_bytes);
        // let max_bytes_buf = [0; 46];
        // max_bytes_buf.copy_from_slice(&max_bytes);

        // let start = std::ops::Bound::Included(min_bytes_buf);
        // let end = std::ops::Bound::Included(max_bytes_buf);

        // for record_buf in self.open_set.range((start, end)) {
        //     let (record, _) = OpenSetRecord::decode(record_buf);
        //     if record.tx_id <
        // }
    }
}

pub struct OpenSetRecord {
    write: Write,
    tx_id: u64,
}

impl OpenSetRecord {
    // <set_id(4)><value(33)><tx_id(8)><write_flags(1)>
    pub fn encode(&self, bytes: &mut Vec<u8>) {
        // Maybe should be big endian but doesn't matter based on how we're using it.
        bytes.extend(self.write.set_id.to_le_bytes());

        let written = self.write.value.encode(bytes);
        // Pad value
        for _ in 0..(33 - written) {
            bytes.push(0);
        }

        // NOTE: big endian for lex sorting
        bytes.extend(self.tx_id.to_be_bytes());

        // NOTE: if write flags change this needs to be updated
        // TODO: fix this
        // Also note that this is not serializing the flags correctly
        // This will break at some point and some poor engineer will
        // be very confused for probably a whole day.
        bytes.push(self.write.operation.to_u8());
    }

    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let bytes = bytes.as_ref();
        let mut read_count = 0;

        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[read_count..read_count + 4]);
        let set_id = u32::from_le_bytes(dst);
        read_count += 4;

        let (value, rc) = Value::decode(bytes);
        read_count += 33;

        let mut dst = [0u8; 8];
        dst.copy_from_slice(&bytes[read_count..read_count + 4]);
        let tx_id = u64::from_le_bytes(dst);
        read_count += 8;

        let flags = bytes[read_count];
        read_count += 1;

        let operation = Operation::from_u8(flags);

        (
            Self {
                write: Write {
                    operation,
                    set_id,
                    value,
                },
                tx_id,
            },
            read_count,
        )
    }
}

pub struct TransactionalDBThreadSafe {
    inner: RwLock<TransactionalDB>,
}

pub struct PageTuplePointer {
    page_offset: u32,
    tuple_offset: u8,
}

pub struct Page {
    tuples: Vec<Tuple>,
}

pub struct Tuple {
    tx_id: u64,
    value: [u8; 32],
}

// A page based structure would allow us to do soa vs aos
// but in that case the TxDB would have to know something
// about the structure of the tuples
pub struct ClosedPageSet {
    pages: Vec<RwLock<Page>>,
}

// NOTE: This file is thoughts on how we might use a generic KV store
// to implement transactional indexes of various types. This way
// indexes would be by default MVCC. Here be dragons, but it's an interesting
// direction

use std::collections::{HashMap, BTreeMap};
use crate::hash::Hash;

struct Tx {

}

pub enum IndexType {
    Hash,
    BTree,
}

// aka Record
// Must be atomically, durably written to disk
#[derive(Debug)]
pub struct Transaction {
    pub writes: Vec<Write>
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    Data { len: u8, buf: [u8; 31] } ,
    Hash(Hash)
}

#[derive(Debug, Copy, Clone)]
#[repr(u8)]
pub enum Operation {
    Delete = 0,
    Insert
}

#[derive(Debug, Copy, Clone)]
pub struct Write {
    pub operation: Operation,
    pub set_id: u32,
    pub value: Value
}

struct KVDB {
    btree_indexes: HashMap<u32, BTreeMap<Vec<u8>, Vec<u8>>>
}

impl KVDB {
    const METADATA_INDEX_ID: u32 = 0;

    pub fn new() -> Self {
        let mut btree_indexes = HashMap::new();
        btree_indexes.insert(Self::METADATA_INDEX_ID, BTreeMap::new());

        Self {
            btree_indexes,
        }
    }

    pub fn contains_key(&self) {

    }

    pub fn begin_tx(&mut self) -> Tx {
        Tx {}
    }

    pub fn rollback_tx(&mut self, _tx: Tx) {
        // TODO: clean up branched_transactions
        unimplemented!();
    }

    pub fn commit_tx(&mut self, tx: Tx) {

    }

    pub fn create_index(&mut self, tx: Tx, index_type: IndexType) -> u32 {
        // Get ID for new index
        let bytes = self.get(tx, Self::METADATA_INDEX_ID, b"cur_index_id").unwrap();
        let dst: [u8; 4] = [0; 4];
        dst.copy_from_slice(&bytes[0..4]);
        let id = u32::from_le_bytes(dst);
        self.remove(tx, Self::METADATA_INDEX_ID, b"cur_index_id");
        self.insert(tx, Self::METADATA_INDEX_ID, b"cur_index_id", (id + 1).to_le_bytes());

        // Set index type metadata
        // TODO: bug here "cur_"
        let index_type: u8 = 0; // btree
        let key = format!("create_{}", id).as_bytes();
        self.insert(tx, Self::METADATA_INDEX_ID, key, [index_type]);

        id
    }

    pub fn get(&self, tx: Tx, index_id: u32, key: impl AsRef<[u8]>) -> Option<&[u8]> {
        //let x = HashMap::new();
        None
    }

    pub fn insert(&mut self, tx: Tx, index_id: u32, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) {

        if index_id == Self::METADATA_INDEX_ID && key.as_ref()[0..7] == &(b"create_")[0..7] {
            // TODO: get actual type
            let index_type = 0;
            if index_type == 0 {
                self.btree_indexes.insert(id, BTreeMap::new());
            } else {
                unimplemented!();
            }
        }

    }

    pub fn remove(&mut self, tx: Tx, index_id: u32, key: impl AsRef<[u8]>) {

    }

}