use super::super::hash::{hash_bytes, Hash};
use rocksdb::{self, ColumnFamily};

pub(crate) struct ObjectDB<'a>  {
    rocksdb: &'a mut rocksdb::DB,
    // cf_handle: ColumnFamily,
}

impl<'a> ObjectDB<'a> {
    pub fn new(rocksdb: &'a mut rocksdb::DB) -> Self {
        // let cf_handle = rocksdb.cf_handle("odb").unwrap();
        // let cf_handle = cf_handle.to_owned();
        Self {
            rocksdb,
            // cf_handle, 
        }
    }

    pub fn total_key_size_bytes(&self) -> u64 {
        0
    }

    pub fn total_obj_size_bytes(&self) -> u64 {
        self.rocksdb.property_int_value("rocksdb.cur-size-all-mem-tables").unwrap().unwrap()
    }

    pub fn total_mem_size_bytes(&self) -> u64 {
        self.total_key_size_bytes() + self.total_obj_size_bytes()
    }

    pub fn add(&mut self, bytes: Vec<u8>) -> Hash {
        let hash = hash_bytes(&bytes);
        // if let Some(_) = self.rocksdb.get_cf(&self.cf_handle, hash).unwrap() {
        //     return hash;
        // }
        // self.rocksdb.put_cf(&self.cf_handle, hash, bytes).unwrap();
        hash
    }

    pub fn get(&'a self, hash: Hash) -> Option<&'a [u8]> {
        // self.rocksdb.get_pinned_cf(&self.cf_handle, hash).unwrap()
        unimplemented!();
    }

}

