use std::{sync::Mutex, collections::HashMap};
use crate::hash::Hash;
use prost::Message;
use crate::protobuf::{worker_db::DatabaseInstanceState, control_worker_api::ScheduleState, control_db::{Database, DatabaseInstance}};

lazy_static::lazy_static! {
    static ref WORKER_DB: sled::Db = init().unwrap();
    static ref DATABASES: Mutex<HashMap<u64, Database>> = Mutex::new(HashMap::new());
    static ref DATABASE_INSTANCES: Mutex<HashMap<u64, DatabaseInstance>> = Mutex::new(HashMap::new());
}

fn init() -> Result<sled::Db, anyhow::Error> {
    let config = sled::Config::default()
        .path("/stdb/worker_node/worker_db")
        .flush_every_ms(Some(50))
        .mode(sled::Mode::HighThroughput);
    let db = config.open()?;
    Ok(db)
}

pub fn set_node_id(node_id: u64) -> Result<(), anyhow::Error> {
    WORKER_DB.insert("node_id", &node_id.to_be_bytes())?;
    Ok(())
}

pub fn get_node_id() -> Result<Option<u64>, anyhow::Error> {
    if let Some(value) = WORKER_DB.get("node_id")? {
        let mut dst = [0u8; 8];
        dst.copy_from_slice(&value[..]);
        let node_id = u64::from_be_bytes(dst);

        Ok(Some(node_id))
    } else {
        Ok(None)
    }
}

pub fn upsert_database_instance_state(state: DatabaseInstanceState) -> Result<(), anyhow::Error> {
    let tree = WORKER_DB.open_tree("worker_database_instance")?;

    let mut buf = Vec::new();
    state.encode(&mut buf).unwrap();

    tree.insert(state.database_instance_id.to_be_bytes(), buf.clone())?;
    Ok(())
}

pub fn get_database_instance_state(database_instance_id: u64) -> Result<Option<DatabaseInstanceState>, anyhow::Error> {
    let tree = WORKER_DB.open_tree("worker_database_instance")?;

    if let Some(value) = tree.get(database_instance_id.to_be_bytes())? {
        let state  = DatabaseInstanceState::decode(&value[..])?;
        Ok(Some(state))
    } else {
        Ok(None)
    }
}

pub fn init_with_schedule_state(schedule_state: ScheduleState) {
    for database in schedule_state.databases {
        let mut databases = DATABASES.lock().unwrap();
        databases.insert(database.id, database);
    }

    for instance in schedule_state.database_instances {
        let mut instances = DATABASE_INSTANCES.lock().unwrap();
        instances.insert(instance.id, instance);
    }
}

pub fn get_database_by_id(id: u64) -> Option<Database> {
    let databases = DATABASES.lock().unwrap();
    databases.get(&id).map(|d| d.to_owned())
}

pub fn get_database_by_address(identity: &Hash, name: &str) -> Option<Database> {
    let databases = DATABASES.lock().unwrap();
    for database in databases.values() {
        if Hash::from_slice(&database.identity) == *identity && database.name == name {
            return Some(database.clone());
        }
    }
    None
}

pub fn _get_databases() -> Vec<Database> {
    let databases = DATABASES.lock().unwrap();
    databases.values().map(|d| d.to_owned()).collect()
}

pub fn insert_database(database: Database) {
    let mut databases = DATABASES.lock().unwrap();
    databases.insert(database.id, database);
}

pub fn delete_database(database_id: u64) {
    let mut databases = DATABASES.lock().unwrap();
    databases.remove(&database_id);
}

pub fn _get_database_instance_by_id(id: u64) -> Option<DatabaseInstance> {
    let instances = DATABASE_INSTANCES.lock().unwrap();
    instances.get(&id).map(|d| d.to_owned())
}

pub fn get_database_instances() -> Vec<DatabaseInstance> {
    let instances = DATABASE_INSTANCES.lock().unwrap();
    instances.values().map(|d| d.to_owned()).collect()
}

pub fn get_leader_database_instance_by_database(database_id: u64) -> Option<DatabaseInstance> {
    for instance in get_database_instances() {
        if instance.database_id == database_id && instance.leader {
            return Some(instance);
        }
    }
    None
}

pub fn insert_database_instance(database_instance: DatabaseInstance) {
    let mut instances = DATABASE_INSTANCES.lock().unwrap();
    instances.insert(database_instance.id, database_instance);
}

pub fn delete_database_instance(database_instance_id: u64) {
    let mut instances = DATABASE_INSTANCES.lock().unwrap();
    instances.remove(&database_instance_id);
}