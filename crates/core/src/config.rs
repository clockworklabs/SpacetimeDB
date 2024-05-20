use crate::util::slow::SlowQueryConfig;
use spacetimedb_sats::relation::{Column, FieldName, Header};
use spacetimedb_sats::{product, AlgebraicType, AlgebraicValue};
use spacetimedb_vm::errors::{ConfigError, ErrorVm};
use spacetimedb_vm::relation::MemTable;
use std::env::temp_dir;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod paths {
    use super::*;

    /// The default path for the database files.
    pub(super) fn db_path() -> PathBuf {
        PathBuf::from("/stdb")
    }

    /// The default path for the database logs.
    pub(super) fn logs_path() -> PathBuf {
        PathBuf::from("/var/log")
    }

    /// The default path for the database config files.
    pub(super) fn config_path() -> PathBuf {
        PathBuf::from("/etc/spacetimedb/")
    }
}

#[cfg(target_os = "macos")]
mod paths {
    use super::*;

    /// The default path for the database files.
    pub(super) fn db_path() -> PathBuf {
        PathBuf::from("/usr/local/var/stdb")
    }

    /// The default path for the database logs.
    pub(super) fn logs_path() -> PathBuf {
        PathBuf::from("/var/log")
    }

    /// The default path for the database config files.
    pub(super) fn config_path() -> PathBuf {
        PathBuf::from("/etc/spacetimedb/")
    }
}

#[cfg(target_os = "windows")]
mod paths {
    use super::*;

    /// The default path for the database files.
    pub(super) fn db_path() -> PathBuf {
        dirs::data_dir()
            .map(|x| x.join("stdb"))
            .expect("failed to read the windows `data directory`")
    }

    /// The default path for the database logs.
    pub(super) fn logs_path() -> PathBuf {
        db_path().join("log")
    }

    /// The default path for the database config files.
    pub(super) fn config_path() -> PathBuf {
        dirs::config_dir()
            .map(|x| x.join("stdb"))
            .expect("Fail to read the windows `config directory`")
    }
}

/// Returns the default path for the database in the `OS` temporary directory.
pub fn stdb_path_temp() -> PathBuf {
    temp_dir().join("stdb")
}

/// Types specifying where to find various files needed by spacetimedb.
pub trait SpacetimeDbFiles {
    /// The path for the database files.
    fn db_path(&self) -> PathBuf;

    /// The path for the database logs.
    fn logs(&self) -> PathBuf;

    /// The path for the database config files.
    fn config(&self) -> PathBuf;

    /// The path of the database config file `log.conf` for logs.
    fn log_config(&self) -> PathBuf {
        self.config().join("log.conf")
    }

    /// The path of the private key file `id_ecdsa`.
    fn private_key(&self) -> PathBuf {
        self.config().join("id_ecdsa")
    }

    /// The path of the public key file `id_ecdsa.pub`.
    fn public_key(&self) -> PathBuf {
        self.config().join("id_ecdsa.pub")
    }
}

/// The location of paths for the database in a local OR temp folder.
pub struct FilesLocal {
    dir: PathBuf,
}

impl FilesLocal {
    /// Create a new [FilesLocal], appending `name` to the `temp` folder returned by [stdb_path_temp].
    pub fn temp(name: &str) -> Self {
        assert!(!name.is_empty(), "`name` should be filled");

        Self {
            dir: stdb_path_temp().join(name),
        }
    }

    /// Create a new [FilesLocal] that is in a hidden `path + .spacetime` folder.
    pub fn hidden<P: AsRef<Path>>(path: P) -> Self {
        Self {
            dir: path.as_ref().join(".spacetime"),
        }
    }
}

impl SpacetimeDbFiles for FilesLocal {
    fn db_path(&self) -> PathBuf {
        self.dir.clone()
    }

    fn logs(&self) -> PathBuf {
        self.db_path().join("logs")
    }

    fn config(&self) -> PathBuf {
        self.db_path().join("conf")
    }
}

/// The global location of paths for the database.
///
/// NOTE: This location varies by OS.
pub struct FilesGlobal;

impl SpacetimeDbFiles for FilesGlobal {
    fn db_path(&self) -> PathBuf {
        paths::db_path()
    }

    fn logs(&self) -> PathBuf {
        paths::logs_path()
    }

    fn config(&self) -> PathBuf {
        paths::config_path()
    }
}

/// Enumeration of options for reading configuration settings.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ReadConfigOption {
    SlowQueryThreshold,
    SlowIncrementalUpdatesThreshold,
    SlowSubscriptionsThreshold,
}

impl ReadConfigOption {
    pub fn type_of(&self) -> AlgebraicType {
        AlgebraicType::U64
    }
}

impl Display for ReadConfigOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            ReadConfigOption::SlowQueryThreshold => "slow_ad_hoc_query_ms",
            ReadConfigOption::SlowIncrementalUpdatesThreshold => "slow_tx_update_ms",
            ReadConfigOption::SlowSubscriptionsThreshold => "slow_subscription_query_ms",
        };
        write!(f, "{value}")
    }
}

impl FromStr for ReadConfigOption {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "slow_ad_hoc_query_ms" => Ok(Self::SlowQueryThreshold),
            "slow_tx_update_ms" => Ok(Self::SlowIncrementalUpdatesThreshold),
            "slow_subscription_query_ms" => Ok(Self::SlowSubscriptionsThreshold),
            x => Err(ConfigError::NotFound(x.into())),
        }
    }
}

/// Holds a list of the runtime configurations settings of the database
#[derive(Debug, Clone, Copy)]
pub struct DatabaseConfig {
    pub(crate) slow_query: SlowQueryConfig,
}

impl DatabaseConfig {
    /// Creates a new `DatabaseConfig` with the specified slow query settings.
    pub(crate) fn with_slow_query(slow_query: SlowQueryConfig) -> Self {
        Self { slow_query }
    }

    /// Reads a configuration setting specified by parsing `key`.
    fn read(&self, key: &str) -> Result<Option<Duration>, ConfigError> {
        let key = ReadConfigOption::from_str(key)?;

        Ok(match key {
            ReadConfigOption::SlowQueryThreshold => self.slow_query.queries,
            ReadConfigOption::SlowIncrementalUpdatesThreshold => self.slow_query.incremental_updates,
            ReadConfigOption::SlowSubscriptionsThreshold => self.slow_query.subscriptions,
        })
    }

    /// Reads a configuration setting specified by parsing `key` and converts it into a `MemTable`.
    ///
    /// For returning as `table` for `SQL` queries.
    pub(crate) fn read_key_into_table(&self, key: &str) -> Result<MemTable, ConfigError> {
        let value: AlgebraicValue = self.read(key)?.map(|v| v.as_millis()).into();

        let table_id = u32::MAX.into();
        let col = Column::new(
            FieldName::new(table_id, 0.into()),
            AlgebraicType::option(AlgebraicType::U128),
        );
        let head = Header::new(table_id, "mem#read_key_into_table".into(), [col].into(), Vec::new());

        Ok(MemTable::from_iter(Arc::new(head), [product![value]]))
    }

    /// Writes the configuration setting specified by parsing `key` and `value`.
    pub(crate) fn set_config(&mut self, key: &str, value: AlgebraicValue) -> Result<(), ErrorVm> {
        let config = ReadConfigOption::from_str(key)?;
        let millis = match value.as_u64() {
            Some(0) => None,
            Some(value) => Some(Duration::from_millis(*value)),
            None => return Err(ConfigError::TypeError(key.into(), value, AlgebraicType::U64).into()),
        };

        match config {
            ReadConfigOption::SlowQueryThreshold => self.slow_query.queries = millis,
            ReadConfigOption::SlowIncrementalUpdatesThreshold => self.slow_query.incremental_updates = millis,
            ReadConfigOption::SlowSubscriptionsThreshold => self.slow_query.subscriptions = millis,
        };

        Ok(())
    }
}
