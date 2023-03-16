use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};

use super::worker_database_instance::WorkerDatabaseInstance;

#[derive(Default)]
pub struct DatabaseInstanceContextController {
    contexts: Mutex<HashMap<u64, Arc<WorkerDatabaseInstance>>>,
}

impl DatabaseInstanceContextController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, database_instance_id: u64) -> Option<Arc<WorkerDatabaseInstance>> {
        let contexts = self.contexts.lock().unwrap();
        contexts.get(&database_instance_id).cloned()
    }

    pub fn insert(&self, worker_database_instance: Arc<WorkerDatabaseInstance>) {
        let database_instance_id = worker_database_instance.database_instance_id;
        let mut contexts = self.contexts.lock().unwrap();
        contexts.insert(database_instance_id, worker_database_instance);
    }

    pub fn remove(&self, database_instance_id: u64) {
        let mut contexts = self.contexts.lock().unwrap();
        contexts.remove(&database_instance_id);
    }
}
