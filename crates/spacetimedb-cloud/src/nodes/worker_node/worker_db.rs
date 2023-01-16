use prost::Message;
use spacetimedb::address::Address;
use spacetimedb::protobuf::{
    control_db::{Database, DatabaseInstance},
    control_worker_api::ScheduleState,
    worker_db::DatabaseInstanceState,
};
use std::{collections::HashMap, sync::Mutex};

// TODO: Consider making not static
lazy_static::lazy_static! {
    pub static ref WORKER_DB: WorkerDb = WorkerDb::init().unwrap();
}

pub struct WorkerDb {
    db: sled::Db,
    databases: Mutex<HashMap<u64, Database>>,
    database_instances: Mutex<HashMap<u64, DatabaseInstance>>,
}

impl WorkerDb {
    fn init() -> Result<Self, anyhow::Error> {
        let config = sled::Config::default()
            .path("/stdb/worker_node/worker_db")
            .flush_every_ms(Some(50))
            .mode(sled::Mode::HighThroughput);
        let db = config.open()?;
        let databases = Mutex::default();
        let database_instances = Mutex::default();
        Ok(Self {
            db,
            databases,
            database_instances,
        })
    }

    pub fn set_node_id(&self, node_id: u64) -> Result<(), anyhow::Error> {
        self.db.insert("node_id", &node_id.to_be_bytes())?;
        Ok(())
    }

    pub fn get_node_id(&self) -> Result<Option<u64>, anyhow::Error> {
        if let Some(value) = self.db.get("node_id")? {
            let mut dst = [0u8; 8];
            dst.copy_from_slice(&value[..]);
            let node_id = u64::from_be_bytes(dst);

            Ok(Some(node_id))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_database_instance_state(&self, state: DatabaseInstanceState) -> Result<(), anyhow::Error> {
        let tree = self.db.open_tree("worker_database_instance")?;

        let mut buf = Vec::new();
        state.encode(&mut buf).unwrap();

        tree.insert(state.database_instance_id.to_be_bytes(), buf.clone())?;
        Ok(())
    }

    pub fn get_database_instance_state(
        &self,
        database_instance_id: u64,
    ) -> Result<Option<DatabaseInstanceState>, anyhow::Error> {
        let tree = self.db.open_tree("worker_database_instance")?;

        if let Some(value) = tree.get(database_instance_id.to_be_bytes())? {
            let state = DatabaseInstanceState::decode(&value[..])?;
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    pub fn init_with_schedule_state(&self, schedule_state: ScheduleState) {
        for database in schedule_state.databases {
            let mut databases = self.databases.lock().unwrap();
            databases.insert(database.id, database);
        }

        for instance in schedule_state.database_instances {
            let mut instances = self.database_instances.lock().unwrap();
            instances.insert(instance.id, instance);
        }
    }

    pub fn get_database_by_id(&self, id: u64) -> Option<Database> {
        let databases = self.databases.lock().unwrap();
        databases.get(&id).map(|d| d.to_owned())
    }

    pub fn get_database_by_address(&self, address: &Address) -> Option<Database> {
        let databases = self.databases.lock().unwrap();
        for database in databases.values() {
            if Address::from_slice(&database.address) == *address {
                return Some(database.clone());
            }
        }
        None
    }

    pub fn _get_databases(&self) -> Vec<Database> {
        let databases = self.databases.lock().unwrap();
        databases.values().map(|d| d.to_owned()).collect()
    }

    pub fn insert_database(&self, database: Database) -> u64 {
        let mut databases = self.databases.lock().unwrap();
        let id = database.id;
        databases.insert(id, database);
        id
    }

    pub fn delete_database(&self, database_id: u64) -> Option<u64> {
        let mut databases = self.databases.lock().unwrap();
        databases.remove(&database_id).map(|_| database_id)
    }

    pub fn _get_database_instance_by_id(&self, id: u64) -> Option<DatabaseInstance> {
        let instances = self.database_instances.lock().unwrap();
        instances.get(&id).map(|d| d.to_owned())
    }

    pub fn get_database_instances(&self) -> Vec<DatabaseInstance> {
        let instances = self.database_instances.lock().unwrap();
        instances.values().map(|d| d.to_owned()).collect()
    }

    pub fn get_leader_database_instance_by_database(&self, database_id: u64) -> Option<DatabaseInstance> {
        self.get_database_instances()
            .into_iter()
            .find(|instance| instance.database_id == database_id && instance.leader)
    }

    pub fn insert_database_instance(&self, database_instance: DatabaseInstance) -> u64 {
        let mut instances = self.database_instances.lock().unwrap();
        let id = database_instance.id;
        instances.insert(id, database_instance);
        id
    }

    pub fn delete_database_instance(&self, database_instance_id: u64) {
        let mut instances = self.database_instances.lock().unwrap();
        instances.remove(&database_instance_id);
    }
}
