use super::database_logger::DatabaseLogger;
use crate::db::message_log::MessageLog;
use crate::db::ostorage::hashmap_object_db::HashMapObjectDB;
use crate::db::ostorage::ObjectDB;
use crate::db::relational_db::RelationalDB;
use crate::identity::Identity;
use crate::protobuf::control_db::{Database, HostType};
use crate::{address::Address, db::relational_db::RelationalDBWrapper};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct WorkerDatabaseInstance {
    pub database_instance_id: u64,
    pub database_id: u64,
    pub host_type: HostType,
    pub trace_log: bool,
    pub identity: Identity,
    pub address: Address,
    pub logger: Arc<Mutex<DatabaseLogger>>,
    pub relational_db: RelationalDBWrapper,
    pub message_log: Arc<Mutex<MessageLog>>,
    pub odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
}

impl WorkerDatabaseInstance {
    pub fn from_database(database: Database, instance_id: u64, root_db_path: impl Into<PathBuf>) -> Arc<Self> {
        let host_type = database.host_type();
        let identity = Identity::from_slice(&database.identity);
        let address = Address::from_slice(database.address);

        let mut db_path = root_db_path.into();
        db_path.push(address.to_hex());
        db_path.push(instance_id.to_string());
        db_path.push("database");

        let log_path = DatabaseLogger::filepath(&address, instance_id);

        Self::new(
            instance_id,
            database.id,
            host_type,
            database.trace_log,
            identity,
            address,
            db_path,
            log_path,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        database_instance_id: u64,
        database_id: u64,
        host_type: HostType,
        trace_log: bool,
        identity: Identity,
        address: Address,
        db_path: impl AsRef<Path>,
        log_path: impl AsRef<Path>,
    ) -> Arc<Self> {
        let mlog_path = db_path.as_ref().join("mlog");
        let odb_path = db_path.as_ref().join("odb");

        let message_log = Arc::new(Mutex::new(MessageLog::open(mlog_path).unwrap()));
        let odb = Arc::new(Mutex::new(WorkerDatabaseInstance::make_default_ostorage(odb_path)));
        Arc::new(Self {
            database_instance_id,
            database_id,
            host_type,
            trace_log,
            identity,
            address,
            logger: Arc::new(Mutex::new(DatabaseLogger::open(&log_path))),
            message_log: message_log.clone(),
            odb: odb.clone(),
            relational_db: RelationalDBWrapper::new(RelationalDB::open(db_path, message_log, odb).unwrap()),
        })
    }

    pub(crate) fn make_default_ostorage(path: impl AsRef<Path>) -> Box<dyn ObjectDB + Send> {
        Box::new(HashMapObjectDB::open(path).unwrap())
    }
}
