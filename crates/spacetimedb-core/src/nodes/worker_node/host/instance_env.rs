use log::debug;
use spacetimedb_lib::{ElementDef, PrimaryKey, TupleDef, TupleValue, TypeDef, TypeValue};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::db::relational_db::{RelationalDB, TxWrapper};
use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;
use crate::nodes::worker_node::worker_metrics::{
    INSTANCE_ENV_DELETE_EQ, INSTANCE_ENV_DELETE_PK, INSTANCE_ENV_DELETE_RANGE, INSTANCE_ENV_DELETE_VALUE,
    INSTANCE_ENV_INSERT, INSTANCE_ENV_ITER,
};
use crate::util::prometheus_handle::HistogramVecHandle;

#[derive(Clone)]
pub struct InstanceEnv {
    pub instance_id: u32,
    pub worker_database_instance: WorkerDatabaseInstance,
    pub instance_tx_map: Arc<Mutex<HashMap<u32, TxWrapper>>>,
}

// Generic 'instance environment' delegated to from various host types.
impl InstanceEnv {
    pub fn console_log(&self, level: u8, s: &String) {
        self.worker_database_instance
            .logger
            .lock()
            .unwrap()
            .write(level, s.clone());
        log::debug!("MOD: {}", s);
    }

    pub fn insert(&self, table_id: u32, buffer: bytes::Bytes) -> Result<(), ()> {
        let mut measure = HistogramVecHandle::new(
            &INSTANCE_ENV_INSERT,
            vec![self.worker_database_instance.address.to_hex(), format!("{}", table_id)],
        );
        measure.start();
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap().unwrap();
        let row = RelationalDB::decode_row(&schema, &mut &buffer[..]);
        let row = match row {
            Ok(x) => x,
            Err(e) => {
                log::error!("insert: Failed to decode row: table_id: {table_id} Err: {e}");
                return Err(());
            }
        };

        stdb.insert(tx, table_id, row)
            .map_err(|e| log::error!("insert: Failed to insert row: table_id: {} Err: {}", table_id, e))
    }

    pub fn delete_pk(&self, table_id: u32, buffer: bytes::Bytes) -> Result<(), ()> {
        let mut measure = HistogramVecHandle::new(
            &INSTANCE_ENV_DELETE_PK,
            vec![self.worker_database_instance.address.to_hex(), format!("{}", table_id)],
        );
        measure.start();
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let (primary_key, _) = PrimaryKey::decode(&buffer[..]);
        let res = stdb
            .delete_pk(tx, table_id, primary_key)
            .map_err(|e| log::error!("delete: Failed to delete row: table_id: {table_id} Err: {e}"))?;
        if let Some(_) = res {
            Ok(())
        } else {
            Err(())
        }
    }

    pub fn delete_value(&self, table_id: u32, buffer: bytes::Bytes) -> Result<(), ()> {
        let mut measure = HistogramVecHandle::new(
            &INSTANCE_ENV_DELETE_VALUE,
            vec![self.worker_database_instance.address.to_hex(), format!("{}", table_id)],
        );
        measure.start();
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap().unwrap();
        let row = RelationalDB::decode_row(&schema, &mut &buffer[..]);
        if let Err(e) = row {
            log::error!("delete_value: Failed to decode row! table_id: {} Err: {}", table_id, e);
            return Err(());
        }

        let pk = RelationalDB::pk_for_row(&row.unwrap());
        let res = stdb
            .delete_pk(tx, table_id, pk)
            .map_err(|e| log::error!("delete: Failed to delete row: table_id: {table_id} Err: {e}"))?;
        if let Some(_) = res {
            return Ok(());
        } else {
            return Err(());
        }
    }

    pub fn delete_eq(&self, table_id: u32, col_id: u32, buffer: bytes::Bytes) -> Result<u32, ()> {
        let mut measure = HistogramVecHandle::new(
            &INSTANCE_ENV_DELETE_EQ,
            vec![self.worker_database_instance.address.to_hex(), format!("{}", table_id)],
        );
        measure.start();

        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap().unwrap();
        let type_def = &schema.elements[col_id as usize].element_type;

        let eq_value = TypeValue::decode(type_def, &mut &buffer[..]);
        let eq_value = eq_value.expect("You can't let modules crash you like this you fool.");
        let seek = stdb.seek(tx, table_id, col_id, eq_value);
        if let Ok(seek) = seek {
            let seek: Vec<TupleValue> = seek.collect::<Vec<_>>();
            stdb.delete_in(tx, table_id, seek)
                .map_err(|e| log::error!("delete: Failed to delete row: table_id: {table_id} Err: {e}"))?
                .ok_or(())
        } else {
            Err(())
        }
    }

    pub fn delete_range(&self, table_id: u32, col_id: u32, buffer: bytes::Bytes) -> Result<u32, ()> {
        let mut measure = HistogramVecHandle::new(
            &INSTANCE_ENV_DELETE_RANGE,
            vec![self.worker_database_instance.address.to_hex(), format!("{}", table_id)],
        );
        measure.start();

        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = stdb.schema_for_table(tx, table_id).unwrap().unwrap();
        let col_type = &schema.elements[col_id as usize].element_type;

        let tuple_def = TupleDef {
            name: None,
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

        let tuple = match TupleValue::decode(&tuple_def, &mut &buffer[..]) {
            Ok(tuple) => tuple,
            Err(e) => {
                log::error!("delete_range: Failed to decode tuple value: Err: {}", e);
                return Err(());
            }
        };

        let start = &tuple.elements[0];
        let end = &tuple.elements[1];

        let range = stdb
            .range_scan(tx, table_id, col_id, start..end)
            .map_err(|e| log::error!("delete_range: Failed to scan range: Err: {}", e))?;
        let range = range.collect::<Vec<_>>();

        stdb.delete_in(tx, table_id, range)
            .map_err(|e| log::error!("delete_range: Failed to delete in range: Err: {}", e))?
            .ok_or(())
    }

    pub fn create_table(&self, buffer: bytes::Bytes) -> u32 {
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let table_info_schema = TupleDef {
            name: None,
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

        let table_info = TupleValue::decode(&table_info_schema, &mut &buffer[..]);
        let table_info = table_info.unwrap_or_else(|e| {
            panic!("create_table: Could not decode table_info! Err: {}", e);
        });

        let table_name = table_info.elements[0].as_string().unwrap();
        let schema_bytes = table_info.elements[1].as_bytes().unwrap();

        let schema = TupleDef::decode(&mut &schema_bytes[..]);
        let schema = schema.unwrap_or_else(|e| {
            panic!("create_table: Could not decode schema! Err: {}", e);
        });

        stdb.create_table(tx, table_name, schema).unwrap()
    }

    pub fn get_table_id(&self, buffer: bytes::Bytes) -> Option<u32> {
        let stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let schema = TypeDef::String;

        let table_name = TypeValue::decode(&schema, &mut &buffer[..]);
        if let Err(e) = table_name {
            debug!("get_table_id: Could not decode table_name! Err: {}", e);
            return None;
        }

        let table_name = table_name.unwrap();
        let table_name = table_name.as_string().unwrap();
        let table_id = stdb.table_id_from_name(tx, table_name);
        if let Err(e) = table_id {
            debug!("get_table_id: Table name {} has no table ID. Err={}", table_name, e);
            return None;
        }
        let table_id = table_id.unwrap();
        if let None = table_id {
            debug!("get_table_id: Table name {} has no table ID.", table_name);
            return None;
        }

        Some(table_id.unwrap())
    }

    pub fn iter(&self, table_id: u32) -> Vec<u8> {
        let mut measure = HistogramVecHandle::new(
            &INSTANCE_ENV_ITER,
            vec![self.worker_database_instance.address.to_hex(), format!("{}", table_id)],
        );
        measure.start();

        let stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
        let tx = instance_tx_map.get_mut(&self.instance_id).unwrap();

        let mut bytes = Vec::new();
        let schema = stdb.schema_for_table(tx, table_id).unwrap().unwrap();
        schema.encode(&mut bytes);

        let mut count = 0;
        for row_bytes in stdb.scan_raw(tx, table_id).unwrap() {
            count += 1;
            bytes.extend(row_bytes);
        }

        log::trace!(
            "Allocating iteration buffer of size {} for {} rows.",
            bytes.len(),
            count
        );

        bytes
    }
}
