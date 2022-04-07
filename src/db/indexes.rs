use std::collections::{BTreeMap, HashMap};
use super::{col_value::ColValue, table::Pointer};

pub struct HashIndex {
    pub col_index: usize,
    pub hash_map: HashMap<ColValue, Pointer>
}
pub struct BTreeIndex {
    pub col_index: usize,
    pub btree_map: BTreeMap<ColValue, Pointer>
}