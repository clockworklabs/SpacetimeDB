use super::database_logger::DatabaseLogger;
use crate::db::relational_db::RelationalDB;
use crate::error::DBError;
use crate::messages::control_db::Database;
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use std::io;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

pub type Result<T> = anyhow::Result<T>;

/// A "live" database.
#[derive(Clone)]
pub struct ReplicaContext {
    pub database: Database,
    pub replica_id: u64,
    pub logger: Arc<DatabaseLogger>,
    pub subscriptions: ModuleSubscriptions,
    pub relational_db: Arc<RelationalDB>,
}

impl ReplicaContext {
    pub fn scheduler_db_path(&self, root_db_path: PathBuf) -> PathBuf {
        let mut scheduler_db_path = root_db_path;
        scheduler_db_path.extend([&*self.database_identity.to_hex(), &*self.replica_id.to_string()]);
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
        Ok(self.logger.size()?)
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

    /// The size in bytes of all of the in-memory data of the database.
    pub fn mem_usage(&self) -> usize {
        self.relational_db.size_in_memory()
    }
}

impl Deref for ReplicaContext {
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
