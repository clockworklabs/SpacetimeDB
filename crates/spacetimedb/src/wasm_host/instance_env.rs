use crate::db::relational_db::RelationalDB;
use crate::{db::transactional_db::Tx, hash::Hash, logs};
use spacetimedb_bindings::{decode_schema, encode_schema, Schema};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use wasmer::{Array, LazyInit, Memory, NativeFunc, WasmPtr, WasmerEnv};

#[derive(WasmerEnv, Clone)]
pub struct InstanceEnv {
    pub instance_id: u32,
    pub relational_db: Arc<Mutex<RelationalDB>>,
    pub instance_tx_map: Arc<Mutex<HashMap<u32, Tx>>>,
    pub module_hash: Hash,
    #[wasmer(export)]
    pub memory: LazyInit<Memory>,
    #[wasmer(export)]
    pub alloc: LazyInit<NativeFunc<u32, WasmPtr<u8, Array>>>,
}

impl InstanceEnv {
    fn bytes_to_string(memory: &Memory, ptr: u32, len: u32) -> String {
        let view = memory.view::<u8>();
        let start = ptr as usize;
        let end = start + len as usize;
        let mut bytes = Vec::new();
        for c in view[start..end].iter() {
            let v = c.get();
            bytes.push(v);
        }
        String::from_utf8(bytes).unwrap()
    }

    fn read_output_bytes(memory: &Memory, ptr: u32) -> Vec<u8> {
        const ROW_BUF_LEN: usize = 1024;
        let view = memory.view::<u8>();
        let start = ptr as usize;
        let end = ptr as usize + ROW_BUF_LEN;
        view[start..end].iter().map(|c| c.get()).collect::<Vec<u8>>()
    }

    pub fn console_log(&self, level: u8, ptr: u32, len: u32) {
        let memory = self.memory.get_ref().expect("Initialized memory");

        let s = Self::bytes_to_string(memory, ptr, len);
        logs::write(self.module_hash, level, s);
    }

    pub fn insert(&self, table_id: u32, ptr: u32) {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);

        let mut stdb = self.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        let row = RelationalDB::decode_row(&schema, &buffer[..]);

        stdb.insert(tx, table_id, row);
    }

    pub fn create_table(&self, table_id: u32, ptr: u32) {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);

        let mut stdb = self.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = decode_schema(&mut &buffer[..]);
        stdb.create_table(tx, table_id, schema).unwrap();
    }

    pub fn iter(&self, table_id: u32) -> u64 {
        let stdb = self.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let memory = self.memory.get_ref().expect("Initialized memory");

        let mut bytes = Vec::new();
        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        encode_schema(Schema { columns: schema }, &mut bytes);

        for row in stdb.iter(tx, table_id).unwrap() {
            RelationalDB::encode_row(row, &mut bytes);
        }

        let alloc_func = self.alloc.get_ref().expect("Intialized alloc function");
        let ptr = alloc_func.call(bytes.len() as u32).unwrap();
        let values = ptr.deref(memory, 0, bytes.len() as u32).unwrap();

        for (i, byte) in bytes.iter().enumerate() {
            values[i].set(*byte);
        }

        let mut data = ptr.offset() as u64;
        data = data << 32 | bytes.len() as u64;
        return data;
    }
}
