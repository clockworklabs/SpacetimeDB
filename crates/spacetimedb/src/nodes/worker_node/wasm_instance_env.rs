use crate::db::relational_db::RelationalDB;
use crate::db::transactional_db::Tx;
use spacetimedb_bindings::{
    decode_schema, encode_schema, ElementDef, EqTypeValue, PrimaryKey, RangeTypeValue, TupleDef, TupleValue,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use wasmer::{Array, LazyInit, Memory, NativeFunc, WasmPtr, WasmerEnv};

use super::worker_database_instance::WorkerDatabaseInstance;

#[derive(WasmerEnv, Clone)]
pub struct InstanceEnv {
    pub instance_id: u32,
    pub worker_database_instance: WorkerDatabaseInstance,
    pub instance_tx_map: Arc<Mutex<HashMap<u32, Tx>>>,
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
        self.worker_database_instance
            .logger
            .lock()
            .unwrap()
            .write(level, s.clone());
        log::debug!("MOD: {}", s);
    }

    pub fn insert(&self, table_id: u32, ptr: u32) {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);

        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        let row = RelationalDB::decode_row(&schema, &buffer[..]);

        stdb.insert(tx, table_id, row);
    }

    pub fn delete_pk(&self, table_id: u32, ptr: u32) -> u8 {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);

        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let (primary_key, _) = PrimaryKey::decode(&buffer[..]);
        if let Some(_) = stdb.delete_pk(tx, table_id, primary_key) {
            return 1;
        } else {
            return 0;
        }
    }

    pub fn delete_value(&self, table_id: u32, ptr: u32) -> u8 {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);

        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        let row = RelationalDB::decode_row(&schema, &buffer[..]);

        let pk = RelationalDB::pk_for_row(&row);
        if let Some(_) = stdb.delete_pk(tx, table_id, pk) {
            return 1;
        } else {
            return 0;
        }
    }

    pub fn delete_eq(&self, table_id: u32, col_id: u32, ptr: u32) -> i32 {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);

        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        let type_def = &schema.elements[col_id as usize].element_type;

        let (eq_value, _) = EqTypeValue::decode(type_def, &buffer[..]);
        let eq_value = eq_value.expect("You can't let modules crash you like this you fool.");
        if let Some(count) = stdb.delete_eq(tx, table_id, col_id, eq_value) {
            return count as i32;
        } else {
            return -1;
        }
    }

    pub fn delete_range(&self, table_id: u32, col_id: u32, ptr: u32) -> i32 {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);

        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        let col_type = &schema.elements[col_id as usize].element_type;

        let tuple_def = TupleDef {
            elements: vec![
                ElementDef {
                    tag: 0,
                    element_type: col_type.clone(),
                },
                ElementDef {
                    tag: 1,
                    element_type: col_type.clone(),
                },
            ],
        };

        let (tuple, _) = TupleValue::decode(&tuple_def, &buffer[..]);
        let start = RangeTypeValue::try_from(&tuple.elements[0]).unwrap();
        let end = RangeTypeValue::try_from(&tuple.elements[1]).unwrap();

        if let Some(count) = stdb.delete_range(tx, table_id, col_id, start..end) {
            return count as i32;
        } else {
            return -1;
        }
    }

    pub fn create_table(&self, table_id: u32, ptr: u32) {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);

        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let (schema, _) = decode_schema(&mut &buffer[..]);
        stdb.create_table(tx, table_id, schema).unwrap();
    }

    pub fn iter(&self, table_id: u32) -> u64 {
        let stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let memory = self.memory.get_ref().expect("Initialized memory");

        let mut bytes = Vec::new();
        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        encode_schema(schema, &mut bytes);

        for row in stdb.iter(tx, table_id).unwrap() {
            RelationalDB::encode_row(&row, &mut bytes);
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
