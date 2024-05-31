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
    /// Log an ad-hoc query if it exceeds this threshold
    SlowQueryThreshold,
    /// Log a subscription query if its incremental evaluation exceeds this threshold
    SlowIncrementalUpdatesThreshold,
    /// Log a subscription query if its initial evaluation exceeds this threshold
    SlowSubscriptionsThreshold,
    /// Reject queries whose estimated cardinality exceeds this limit
    RowLimit,
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
            ReadConfigOption::RowLimit => "row_limit",
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
            "row_limit" => Ok(Self::RowLimit),
            x => Err(ConfigError::NotFound(x.into())),
        }
    }
}

/// Holds a list of the runtime configurations settings of the database
#[derive(Debug, Clone, Copy)]
pub struct DatabaseConfig {
    /// Log queries whose execution time exceeds this limit
    pub(crate) slow_query: SlowQueryConfig,
    /// Reject queries whose estimated cardinality exceeds this limit
    pub(crate) row_limit: Option<u64>,
}

impl DatabaseConfig {
    /// Creates a new `DatabaseConfig` with the specified settings.
    pub(crate) fn new(slow_query: SlowQueryConfig, row_limit: Option<u64>) -> Self {
        Self { slow_query, row_limit }
    }

    /// Reads a configuration setting specified by parsing `key` and converts it into a `MemTable`.
    ///
    /// For returning as `table` for `SQL` queries.
    pub(crate) fn read_key_into_table(&self, key: &str) -> Result<MemTable, ConfigError> {
        let (value, ty): (AlgebraicValue, _) = match ReadConfigOption::from_str(key)? {
            ReadConfigOption::SlowQueryThreshold => (
                self.slow_query.queries.map(|v| v.as_millis()).into(),
                AlgebraicType::option(AlgebraicType::U128),
            ),
            ReadConfigOption::SlowIncrementalUpdatesThreshold => (
                self.slow_query.incremental_updates.map(|v| v.as_millis()).into(),
                AlgebraicType::option(AlgebraicType::U128),
            ),
            ReadConfigOption::SlowSubscriptionsThreshold => (
                self.slow_query.subscriptions.map(|v| v.as_millis()).into(),
                AlgebraicType::option(AlgebraicType::U128),
            ),
            ReadConfigOption::RowLimit => (self.row_limit.into(), AlgebraicType::option(AlgebraicType::U64)),
        };

        let table_id = u32::MAX.into();
        let col = Column::new(FieldName::new(table_id, 0.into()), ty);
        let head = Header::new(table_id, "mem#read_key_into_table".into(), [col].into(), Vec::new());

        Ok(MemTable::from_iter(Arc::new(head), [product![value]]))
    }

    /// Writes the configuration setting specified by parsing `key` and `value`.
    pub(crate) fn set_config(&mut self, key: &str, value: AlgebraicValue) -> Result<(), ErrorVm> {
        let config = ReadConfigOption::from_str(key)?;
        let Some(v) = value.as_u64() else {
            return Err(ConfigError::TypeError(key.into(), value, AlgebraicType::U64).into());
        };

        let to_dur_opt = |v| (v > 0).then(|| Duration::from_millis(v));

        match config {
            ReadConfigOption::SlowQueryThreshold => self.slow_query.queries = to_dur_opt(*v),
            ReadConfigOption::SlowIncrementalUpdatesThreshold => self.slow_query.incremental_updates = to_dur_opt(*v),
            ReadConfigOption::SlowSubscriptionsThreshold => self.slow_query.subscriptions = to_dur_opt(*v),
            ReadConfigOption::RowLimit => self.row_limit = (*v < u64::MAX).then_some(*v),
        };

        Ok(())
    }
}
