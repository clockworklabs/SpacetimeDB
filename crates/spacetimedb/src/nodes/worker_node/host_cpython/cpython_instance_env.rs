use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use spacetimedb_bindings::{ElementDef, EqTypeValue, PrimaryKey, RangeTypeValue, TupleDef, TupleValue};

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
        if let Some(count) = stdb.delete_eq(tx, table_id, col_id, eq_value) {
            return count as i32;
        } else {
            return -1;
        }
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
                    element_type: col_type.clone(),
                },
                ElementDef {
                    tag: 1,
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

        if let Some(count) = stdb.delete_range(tx, table_id, col_id, start..end) {
            return count as i32;
        } else {
            return -1;
        }
    }

    pub fn create_table(&self, table_id: u32, buffer: bytes::Bytes) {
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let (schema, _) = spacetimedb_bindings::decode_schema(&mut &buffer[..]);
        if let Err(e) = schema {
            panic!("create_table: Could not decode schema! Err: {}", e);
        }

        stdb.create_table(tx, table_id, schema.unwrap()).unwrap();
    }

    pub fn iter(&self, table_id: u32) -> Vec<u8> {
        let stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let mut bytes = Vec::new();
        let schema = stdb.schema_for_table(tx, table_id).unwrap();
        spacetimedb_bindings::encode_schema(schema, &mut bytes);

        for row in stdb.iter(tx, table_id).unwrap() {
            RelationalDB::encode_row(&row, &mut bytes);
        }

        bytes
    }
}
