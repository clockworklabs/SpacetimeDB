// NOTE: This file is thoughts on how we might use a generic KV store
// to implement transactional indexes of various types. This way
// indexes would be by default MVCC. Here be dragons, but it's an interesting
// direction

// use std::collections::{HashMap, BTreeMap};
// use crate::hash::Hash;

// struct Tx {

// }

// pub enum IndexType {
//     Hash,
//     BTree,
// }

// // aka Record
// // Must be atomically, durably written to disk
// #[derive(Debug)]
// pub struct Transaction {
//     pub writes: Vec<Write>
// }

// #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
// pub enum Value {
//     Data { len: u8, buf: [u8; 31] } ,
//     Hash(Hash)
// }

// #[derive(Debug, Copy, Clone)]
// #[repr(u8)]
// pub enum Operation {
//     Delete = 0,
//     Insert
// }

// #[derive(Debug, Copy, Clone)]
// pub struct Write {
//     pub operation: Operation,
//     pub set_id: u32,
//     pub value: Value
// }

// struct KVDB {
//     btree_indexes: HashMap<u32, BTreeMap<Vec<u8>, Vec<u8>>>
// }

// impl KVDB {
//     const METADATA_INDEX_ID: u32 = 0;

//     pub fn new() -> Self {
//         let mut btree_indexes = HashMap::new();
//         btree_indexes.insert(Self::METADATA_INDEX_ID, BTreeMap::new());

//         Self {
//             btree_indexes,
//         }
//     }

//     pub fn contains_key(&self) {

//     }

//     pub fn begin_tx(&mut self) -> Tx {
//         Tx {}
//     }

//     pub fn rollback_tx(&mut self, _tx: Tx) {
//         // TODO: clean up branched_transactions
//         unimplemented!();
//     }

//     pub fn commit_tx(&mut self, tx: Tx) {

//     }

//     pub fn create_index(&mut self, tx: Tx, index_type: IndexType) -> u32 {
//         // Get ID for new index
//         let bytes = self.get(tx, Self::METADATA_INDEX_ID, b"cur_index_id").unwrap();
//         let dst: [u8; 4] = [0; 4];
//         dst.copy_from_slice(&bytes[0..4]);
//         let id = u32::from_le_bytes(dst);
//         self.remove(tx, Self::METADATA_INDEX_ID, b"cur_index_id");
//         self.insert(tx, Self::METADATA_INDEX_ID, b"cur_index_id", (id + 1).to_le_bytes());

//         // Set index type metadata
//         // TODO: bug here "cur_"
//         let index_type: u8 = 0; // btree
//         let key = format!("create_{}", id).as_bytes();
//         self.insert(tx, Self::METADATA_INDEX_ID, key, [index_type]);

//         id
//     }

//     pub fn get(&self, tx: Tx, index_id: u32, key: impl AsRef<[u8]>) -> Option<&[u8]> {
//         //let x = HashMap::new();
//         None
//     }

//     pub fn insert(&mut self, tx: Tx, index_id: u32, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) {

//         if index_id == Self::METADATA_INDEX_ID && key.as_ref()[0..7] == &(b"create_")[0..7] {
//             // TODO: get actual type
//             let index_type = 0;
//             if index_type == 0 {
//                 self.btree_indexes.insert(id, BTreeMap::new());
//             } else {
//                 unimplemented!();
//             }
//         }

//     }

//     pub fn remove(&mut self, tx: Tx, index_id: u32, key: impl AsRef<[u8]>) {

//     }

// }
