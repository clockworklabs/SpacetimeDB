use std::collections::HashMap;
use super::hash::{Hash, hash_bytes};

pub struct ObjectDB {
    map: HashMap<Hash, Vec<u8>>,
    obj_size: u64,
}

impl ObjectDB {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            obj_size: 0
        }
    }

    pub fn total_key_size_bytes(&self) -> u64 {
        self.map.len() as u64 * 32 
    }

    pub fn total_obj_size_bytes(&self) -> u64 {
        self.obj_size
    }

    pub fn total_mem_size_bytes(&self) -> u64 {
        self.total_key_size_bytes() + self.total_obj_size_bytes()
    }

    pub fn add(&mut self, bytes: Vec<u8>) -> Hash {
        let hash = hash_bytes(&bytes);
        self.obj_size += bytes.len() as u64;
        if self.map.contains_key(&hash) {
            return hash;
        }
        self.map.insert(hash, bytes);
        hash
    }

    pub fn get(&self, hash: Hash) -> Option<&[u8]> {
        self.map.get(&hash).map(|v| &v[..])
    }
}