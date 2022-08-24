use super::database_logger::DatabaseLogger;
use crate::nodes::HostType;
use crate::{db::relational_db::RelationalDB, hash::Hash};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct WorkerDatabaseInstance {
    pub database_instance_id: u64,
    pub database_id: u64,
    pub host_type: HostType,
    pub identity: Hash,
    pub name: String,
    pub logger: Arc<Mutex<DatabaseLogger>>,
    pub relational_db: Arc<Mutex<RelationalDB>>,
}
