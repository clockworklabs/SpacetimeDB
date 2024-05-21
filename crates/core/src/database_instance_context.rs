use super::database_logger::DatabaseLogger;
use crate::db::relational_db::RelationalDB;
use crate::db::{Config, Storage};
use crate::error::DBError;
use crate::messages::control_db::Database;
use std::io;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

pub type Result<T> = anyhow::Result<T>;

#[derive(Clone)]
pub struct DatabaseInstanceContext {
    pub database: Database,
    pub database_instance_id: u64,
    pub logger: Arc<DatabaseLogger>,
    pub relational_db: Arc<RelationalDB>,
}

impl DatabaseInstanceContext {
    pub fn from_database(
        config: Config,
        database: Database,
        instance_id: u64,
        root_db_path: PathBuf,
        rt: tokio::runtime::Handle,
    ) -> Result<Self> {
        let mut db_path = root_db_path;
        db_path.extend([&*database.address.to_hex(), &*instance_id.to_string()]);
        db_path.push("database");

        let log_path = DatabaseLogger::filepath(&database.address, instance_id);
        let relational_db = match config.storage {
            Storage::Memory => RelationalDB::open(db_path, database.address, None)?,
            Storage::Disk => RelationalDB::local(db_path, rt, database.address)?,
        };

        Ok(Self {
            database,
            database_instance_id: instance_id,
            logger: Arc::new(DatabaseLogger::open(log_path)),
            relational_db: Arc::new(relational_db),
        })
    }

    pub fn scheduler_db_path(&self, root_db_path: PathBuf) -> PathBuf {
        let mut scheduler_db_path = root_db_path;
        scheduler_db_path.extend([&*self.address.to_hex(), &*self.database_instance_id.to_string()]);
        scheduler_db_path.push("scheduler");
        scheduler_db_path
    }

    /// The number of bytes on disk occupied by the database's durability layer.
    ///
    /// An in-memory database will return `Ok(0)`.
    pub fn durability_size_on_disk(&self) -> io::Result<u64> {
        self.relational_db.size_on_disk()
    }

    /// The size of the log file.
    pub fn log_file_size(&self) -> std::result::Result<u64, DBError> {
        self.logger.size()
    }

    /// Obtain an array which can be summed to obtain the total disk usage.
    ///
    /// Some sources of size-on-disk may error, in which case the corresponding array element will be None.
    pub fn total_disk_usage(&self) -> TotalDiskUsage {
        TotalDiskUsage {
            durability: self.durability_size_on_disk().ok(),
            logs: self.log_file_size().ok(),
        }
    }
}

impl Deref for DatabaseInstanceContext {
    type Target = Database;

    fn deref(&self) -> &Self::Target {
        &self.database
    }
}

#[derive(Copy, Clone, Default)]
pub struct TotalDiskUsage {
    pub durability: Option<u64>,
    pub logs: Option<u64>,
}

impl TotalDiskUsage {
    /// Returns self, but if any of the sources are None then we take it from fallback
    pub fn or(self, fallback: TotalDiskUsage) -> Self {
        Self {
            durability: self.durability.or(fallback.durability),
            logs: self.logs.or(fallback.logs),
        }
    }

    pub fn sum(&self) -> u64 {
        self.durability.unwrap_or(0) + self.logs.unwrap_or(0)
    }
}
