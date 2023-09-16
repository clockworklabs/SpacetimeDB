use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};

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
}
