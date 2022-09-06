use lazy_static::lazy_static;
use std::{collections::HashMap, sync::Mutex};

use super::worker_database_instance::WorkerDatabaseInstance;

lazy_static! {
    static ref CONTEXT_CONTROLLER: DatabaseInstanceContextController = DatabaseInstanceContextController::new();
}

pub struct DatabaseInstanceContextController {
    contexts: Mutex<HashMap<u64, WorkerDatabaseInstance>>,
}

impl DatabaseInstanceContextController {
    pub fn get_shared() -> &'static DatabaseInstanceContextController {
        &CONTEXT_CONTROLLER
    }

    fn new() -> Self {
        let contexts = Mutex::new(HashMap::new());
        Self { contexts }
    }

    pub fn get(&self, database_instance_id: u64) -> Option<WorkerDatabaseInstance> {
        let contexts = self.contexts.lock().unwrap();
        contexts.get(&database_instance_id).map(|c| c.clone())
    }

    pub fn insert(&self, worker_database_instance: WorkerDatabaseInstance) {
        let database_instance_id = worker_database_instance.database_instance_id;
        let mut contexts = self.contexts.lock().unwrap();
        contexts.insert(database_instance_id, worker_database_instance);
    }

    pub fn remove(&self, database_instance_id: u64) {
        let mut contexts = self.contexts.lock().unwrap();
        contexts.remove(&database_instance_id);
    }
}
