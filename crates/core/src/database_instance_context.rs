use super::database_logger::DatabaseLogger;
use crate::address::Address;
use crate::db::message_log::MessageLog;
use crate::db::ostorage::memory_object_db::MemoryObjectDB;
use crate::db::ostorage::sled_object_db::SledObjectDB;
use crate::db::ostorage::ObjectDB;
use crate::db::relational_db::RelationalDB;
use crate::db::{Config, FsyncPolicy, Storage};
use crate::identity::Identity;
use crate::messages::control_db::Database;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct DatabaseInstanceContext {
    pub database_instance_id: u64,
    pub database_id: u64,
    pub identity: Identity,
    pub address: Address,
    pub logger: Arc<Mutex<DatabaseLogger>>,
    pub relational_db: Arc<RelationalDB>,
}

impl DatabaseInstanceContext {
    pub fn from_database(config: Config, database: &Database, instance_id: u64, root_db_path: PathBuf) -> Arc<Self> {
        let mut db_path = root_db_path;
        db_path.extend([&*database.address.to_hex(), &*instance_id.to_string()]);
        db_path.push("database");

        let log_path = DatabaseLogger::filepath(&database.address, instance_id);

        Self::new(
            config,
            instance_id,
            database.id,
            database.identity,
            database.address,
            db_path,
            &log_path,
        )
    }

    pub fn scheduler_db_path(&self, root_db_path: PathBuf) -> PathBuf {
        let mut scheduler_db_path = root_db_path;
        scheduler_db_path.extend([&*self.address.to_hex(), &*self.database_instance_id.to_string()]);
        scheduler_db_path.push("scheduler");
        scheduler_db_path
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Config,
        database_instance_id: u64,
        database_id: u64,
        identity: Identity,
        address: Address,
        db_path: PathBuf,
        log_path: &Path,
    ) -> Arc<Self> {
        let message_log = match config.storage {
            Storage::Memory => None,
            Storage::Disk => {
                let mlog_path = db_path.join("mlog");
                Some(Arc::new(Mutex::new(MessageLog::open(mlog_path).unwrap())))
            }
        };

        let odb = match config.storage {
            Storage::Memory => Box::<MemoryObjectDB>::default(),
            Storage::Disk => {
                let odb_path = db_path.join("odb");
                DatabaseInstanceContext::make_default_ostorage(odb_path)
            }
        };
        let odb = Arc::new(Mutex::new(odb));

        Arc::new(Self {
            database_instance_id,
            database_id,
            identity,
            address,
            logger: Arc::new(Mutex::new(DatabaseLogger::open(log_path))),
            relational_db: Arc::new(
                RelationalDB::open(db_path, message_log, odb, address, config.fsync != FsyncPolicy::Never).unwrap(),
            ),
        })
    }

    pub(crate) fn make_default_ostorage(path: impl AsRef<Path>) -> Box<dyn ObjectDB + Send> {
        Box::new(SledObjectDB::open(path).unwrap())
    }
}
