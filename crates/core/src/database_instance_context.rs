use super::database_logger::DatabaseLogger;
use crate::db::message_log::MessageLog;
use crate::db::ostorage::memory_object_db::MemoryObjectDB;
use crate::db::ostorage::sled_object_db::SledObjectDB;
use crate::db::ostorage::ObjectDB;
use crate::db::relational_db::RelationalDB;
use crate::db::{Config, FsyncPolicy, Storage};
use crate::error::DBError;
use crate::messages::control_db::Database;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub type Result<T> = anyhow::Result<T>;

#[derive(Clone)]
pub struct DatabaseInstanceContext {
    pub database: Database,
    pub database_instance_id: u64,
    pub logger: Arc<DatabaseLogger>,
    pub relational_db: Arc<RelationalDB>,
}

impl DatabaseInstanceContext {
    pub fn from_database(config: Config, database: Database, instance_id: u64, root_db_path: PathBuf) -> Result<Self> {
        let mut db_path = root_db_path;
        db_path.extend([&*database.address.to_hex(), &*instance_id.to_string()]);
        db_path.push("database");

        let log_path = DatabaseLogger::filepath(&database.address, instance_id);

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
        let relational_db = RelationalDB::open(
            db_path,
            message_log,
            odb,
            database.address,
            config.fsync != FsyncPolicy::Never,
        )?;

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

    pub(crate) fn make_default_ostorage(path: impl AsRef<Path>) -> Result<Box<dyn ObjectDB + Send>> {
        Ok(SledObjectDB::open(path).map(Box::new)?)
    }

    /// The number of bytes on disk occupied by the [MessageLog].
    pub fn message_log_size_on_disk(&self) -> u64 {
        self.relational_db.message_log_size_on_disk()
    }

    /// The number of bytes on disk occupied by the [ObjectDB].
    pub fn object_db_size_on_disk(&self) -> std::result::Result<u64, DBError> {
        self.relational_db.object_db_size_on_disk()
    }

    /// The size of the log file.
    pub fn log_file_size(&self) -> std::result::Result<u64, DBError> {
        self.logger.size()
    }

    /// Obtain an array which can be summed to obtain the total disk usage.
    ///
    /// Some sources of size-on-disk may error, in which case the corresponding array element will be None.
    pub fn total_disk_usage(&self) -> TotalDiskUsage {
        TotalDiskUsage([
            Some(self.message_log_size_on_disk()),
            // the errors get logged by the functions, we're not discarding them here without logging
            self.object_db_size_on_disk().ok(),
            self.log_file_size().ok(),
        ])
    }
}

impl Deref for DatabaseInstanceContext {
    type Target = Database;

    fn deref(&self) -> &Self::Target {
        &self.database
    }
}

#[derive(Copy, Clone, Default)]
pub struct TotalDiskUsage(pub [Option<u64>; 3]);

impl TotalDiskUsage {
    /// Returns self, but if any of the sources are None then we take it from fallback
    pub fn or(mut self, fallback: TotalDiskUsage) -> Self {
        std::iter::zip(&mut self.0, fallback.0).for_each(|(x, fb)| *x = x.or(fb));
        self
    }

    pub fn sum(&self) -> u64 {
        self.0.iter().map(|x| x.unwrap_or(0)).sum()
    }
}
