use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use spacetimedb_bindings::{
    decode_schema, ElementDef, EqTypeValue, PrimaryKey, RangeTypeValue, TupleDef, TupleValue, TypeDef,
};

use crate::db::relational_db::RelationalDB;
use crate::db::transactional_db::Tx;
use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;

pub(crate) struct InstanceEnv {
    pub instance_id: u32,
    pub worker_database_instance: WorkerDatabaseInstance,
    pub instance_tx_map: Arc<Mutex<HashMap<u32, Tx>>>,
}

// Substantially copied from wasm_instance_env.rs, with excessive code duplication.
// TODO(ryan): factor up into some common pieces.
// TODO(cloutiertyler): agree with Ryan, can probably factor out the actual table manip logic
impl InstanceEnv {
    pub fn console_log(&self, level: u8, s: &String) {
        self.worker_database_instance
            .logger
            .lock()
            .unwrap()
            .write(level, s.clone());
        log::debug!("MOD: {}", s);
    }

    pub fn insert(&self, table_id: u32, buffer: bytes::Bytes) {
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        let row = RelationalDB::decode_row(&schema, &buffer[..]);
        if let Err(e) = row {
            log::error!("insert: Failed to decode row: table_id: {} Err: {}", table_id, e);
            return;
        }

        stdb.insert(tx, table_id, row.unwrap());
    }

    pub fn delete_pk(&self, table_id: u32, buffer: bytes::Bytes) -> u8 {
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

    pub fn delete_value(&self, table_id: u32, buffer: bytes::Bytes) -> u8 {
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        let row = RelationalDB::decode_row(&schema, &buffer[..]);
        if let Err(e) = row {
            log::error!("delete_value: Failed to decode row! table_id: {} Err: {}", table_id, e);
            return 0;
        }

        let pk = RelationalDB::pk_for_row(&row.unwrap());
        if let Some(_) = stdb.delete_pk(tx, table_id, pk) {
            return 1;
        } else {
            return 0;
        }
    }

    pub fn delete_eq(&self, table_id: u32, col_id: u32, buffer: bytes::Bytes) -> i32 {
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        let type_def = &schema.elements[col_id as usize].element_type;

        let (eq_value, _) = EqTypeValue::decode(type_def, &buffer[..]);
        let eq_value = eq_value.expect("You can't let modules crash you like this you fool.");
        let seek = stdb.seek(tx, table_id, col_id, eq_value);
        if let Some(seek) = seek {
            let seek: Vec<TupleValue> = seek.collect::<Vec<_>>();
            let count = stdb.delete_in(tx, table_id, seek).unwrap();
            return count as i32;
        }
        return -1;
    }

    pub fn delete_range(&self, table_id: u32, col_id: u32, buffer: bytes::Bytes) -> i32 {
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        let col_type = &schema.elements[col_id as usize].element_type;

        let tuple_def = TupleDef {
            elements: vec![
                ElementDef {
                    tag: 0,
                    name: None,
                    element_type: col_type.clone(),
                },
                ElementDef {
                    tag: 1,
                    name: None,
                    element_type: col_type.clone(),
                },
            ],
        };

        let (tuple, _) = TupleValue::decode(&tuple_def, &buffer[..]);
        if let Err(e) = tuple {
            log::error!("delete_range: Failed to decode tuple value: Err: {}", e);
            return -1;
        }
        let tuple = tuple.unwrap();

        let start = RangeTypeValue::try_from(&tuple.elements[0]).unwrap();
        let end = RangeTypeValue::try_from(&tuple.elements[1]).unwrap();

        let range = stdb.range_scan(tx, table_id, col_id, start..end);
        if let Some(range) = range {
            let range = range.collect::<Vec<_>>();
            let count = stdb.delete_in(tx, table_id, range).unwrap();
            return count as i32;
        } else {
            return -1;
        }
    }

    pub fn create_table(&self, table_id: u32, buffer: bytes::Bytes) {
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let table_info_schema = TupleDef {
            elements: vec![
                ElementDef {
                    tag: 0,
                    name: None,
                    element_type: TypeDef::String,
                },
                ElementDef {
                    tag: 1,
                    name: None,
                    element_type: TypeDef::Bytes,
                },
            ],
        };

        println!("{:?}", &buffer[..]);

        let (table_info, _) = TupleValue::decode(&table_info_schema, &mut &buffer[..]);
        let table_info = table_info.unwrap_or_else(|e| {
            panic!("create_table: Could not decode table_info! Err: {}", e);
        });

        let table_name = table_info.elements[0].as_string().unwrap();
        let schema_bytes = table_info.elements[1].as_bytes().unwrap();

        let (schema, _) = decode_schema(&mut &schema_bytes[..]);
        let schema = schema.unwrap_or_else(|e| {
            panic!("create_table: Could not decode schema! Err: {}", e);
        });

        stdb.create_table(tx, table_id, table_name, schema).unwrap();
    }

    pub fn iter(&self, table_id: u32) -> Vec<u8> {
        let stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let mut bytes = Vec::new();
        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        spacetimedb_bindings::encode_schema(schema, &mut bytes);

        for row in stdb.scan(tx, table_id).unwrap() {
            RelationalDB::encode_row(&row, &mut bytes);
        }

        bytes
    }
}
