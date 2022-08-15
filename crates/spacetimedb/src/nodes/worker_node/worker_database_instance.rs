use std::sync::{Arc, Mutex};
use crate::{hash::Hash, db::relational_db::RelationalDB};
use super::database_logger::DatabaseLogger;

#[derive(Clone)]
pub struct WorkerDatabaseInstance {
    pub database_instance_id: u64,
    pub database_id: u64,
    pub identity: Hash,
    pub name: String,
    pub logger: Arc<Mutex<DatabaseLogger>>,
    pub relational_db: Arc<Mutex<RelationalDB>>,
}