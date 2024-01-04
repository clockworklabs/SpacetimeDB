use super::database_logger::DatabaseLogger;
use crate::address::Address;
use crate::db::message_log::MessageLog;
use crate::db::ostorage::memory_object_db::MemoryObjectDB;
use crate::db::ostorage::sled_object_db::SledObjectDB;
use crate::db::ostorage::ObjectDB;
use crate::db::relational_db::RelationalDB;
use crate::db::{Config, FsyncPolicy, Storage};
use crate::error::DBError;
use crate::identity::Identity;
use crate::messages::control_db::Database;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub type Result<T> = anyhow::Result<T>;

#[derive(Clone)]
pub struct DatabaseInstanceContext {
    pub database_instance_id: u64,
    pub database_id: u64,
    pub identity: Identity,
    pub address: Address,
    pub logger: Arc<DatabaseLogger>,
    pub relational_db: Arc<RelationalDB>,
    pub publisher_address: Option<Address>,
}

impl DatabaseInstanceContext {
    pub fn from_database(config: Config, database: &Database, instance_id: u64, root_db_path: PathBuf) -> Result<Self> {
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
            database.publisher_address,
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
        publisher_address: Option<Address>,
    ) -> Result<Self> {
        let message_log = match config.storage {
            Storage::Memory => None,
            Storage::Disk => {
                let mlog_path = db_path.join("mlog");
                Some(Arc::new(Mutex::new(MessageLog::open(mlog_path)?)))
            }
        };

        let odb = match config.storage {
            Storage::Memory => Box::<MemoryObjectDB>::default(),
            Storage::Disk => {
                let odb_path = db_path.join("odb");
                Self::make_default_ostorage(odb_path)?
            }
        };
        let odb = Arc::new(Mutex::new(odb));
        let relational_db = RelationalDB::open(db_path, message_log, odb, address, config.fsync != FsyncPolicy::Never)?;

        Ok(Self {
            database_instance_id,
            database_id,
            identity,
            address,
            logger: Arc::new(DatabaseLogger::open(log_path)),
            relational_db: Arc::new(relational_db),
            publisher_address,
        })
    }

    pub(crate) fn make_default_ostorage(path: impl AsRef<Path>) -> Result<Box<dyn ObjectDB + Send>> {
        Ok(SledObjectDB::open(path).map(Box::new)?)
    }

    /// The number of bytes on disk occupied by the [MessageLog].
    pub fn message_log_size_on_disk(&self) -> std::result::Result<u64, DBError> {
        let size = self
            .relational_db
            .commit_log()
            .map(|commit_log| commit_log.message_log_size_on_disk())
            .transpose()?;

        Ok(size.unwrap_or_default())
    }

    /// The number of bytes on disk occupied by the [ObjectDB].
    pub fn object_db_size_on_disk(&self) -> std::result::Result<u64, DBError> {
        let size = self
            .relational_db
            .commit_log()
            .map(|commit_log| commit_log.object_db_size_on_disk())
            .transpose()?;

        Ok(size.unwrap_or_default())
    }

    /// The size of the log file.
    pub fn log_file_size(&self) -> std::result::Result<u64, DBError> {
        self.logger.size()
    }
}
