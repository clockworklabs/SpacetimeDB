use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};

use crate::db::db_metrics::DB_METRICS;
use crate::host::scheduler::Scheduler;

use super::database_instance_context::DatabaseInstanceContext;

#[derive(Default)]
pub struct DatabaseInstanceContextController {
    contexts: Mutex<HashMap<u64, (Arc<DatabaseInstanceContext>, Scheduler)>>,
}

impl DatabaseInstanceContextController {
    pub fn new() -> Self {
        Self::default()
    }

    #[tracing::instrument(skip_all)]
    pub fn get(&self, database_instance_id: u64) -> Option<(Arc<DatabaseInstanceContext>, Scheduler)> {
        let contexts = self.contexts.lock().unwrap();
        contexts.get(&database_instance_id).cloned()
    }

    #[tracing::instrument(skip_all)]
    pub fn insert(&self, database_instance_context: Arc<DatabaseInstanceContext>, scheduler: Scheduler) {
        let database_instance_id = database_instance_context.database_instance_id;
        let mut contexts = self.contexts.lock().unwrap();
        contexts.insert(database_instance_id, (database_instance_context, scheduler));
    }

    #[tracing::instrument(skip_all)]
    pub fn remove(&self, database_instance_id: u64) -> Option<(Arc<DatabaseInstanceContext>, Scheduler)> {
        let mut contexts = self.contexts.lock().unwrap();
        contexts.remove(&database_instance_id)
    }

    #[tracing::instrument(skip_all)]
    pub fn update_metrics(&self) {
        for (db, _) in self.contexts.lock().unwrap().values() {
            // Use the previous gauge value if there is an issue getting the file size.
            if let Ok(num_bytes) = db.message_log_size_on_disk() {
                DB_METRICS
                    .message_log_size
                    .with_label_values(&db.address)
                    .set(num_bytes as i64);
            }
            // Use the previous gauge value if there is an issue getting the file size.
            if let Ok(num_bytes) = db.object_db_size_on_disk() {
                DB_METRICS
                    .object_db_disk_usage
                    .with_label_values(&db.address)
                    .set(num_bytes as i64);
            }
            // Use the previous gauge value if there is an issue getting the file size.
            if let Ok(num_bytes) = db.log_file_size() {
                DB_METRICS
                    .module_log_file_size
                    .with_label_values(&db.address)
                    .set(num_bytes as i64);
            }
        }
    }
}
