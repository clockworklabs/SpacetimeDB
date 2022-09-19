use super::database_logger::DatabaseLogger;
use crate::hash::Hash;
use crate::nodes::HostType;
use crate::{address::Address, db::relational_db::RelationalDBWrapper};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct WorkerDatabaseInstance {
    pub database_instance_id: u64,
    pub database_id: u64,
    pub host_type: HostType,
    pub identity: Hash,
    pub address: Address,
    pub logger: Arc<Mutex<DatabaseLogger>>,
    pub relational_db: RelationalDBWrapper,
}
