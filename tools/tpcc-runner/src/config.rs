use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::Deserialize;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use crate::protocol::DriverAssignment;

const DEFAULT_LOAD_BATCH_SIZE: usize = 500;

#[derive(Debug, Parser)]
#[command(name = "tpcc-runner")]
pub struct Cli {
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Load(LoadArgs),
    Status(StatusArgs),
    Wait(WaitArgs),
    LoadClient(LoadArgs),
    Driver(DriverArgs),
    Coordinator(CoordinatorArgs),
}

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub uri: String,
    pub database_prefix: String,
    pub token: Option<String>,
    pub confirmed_reads: bool,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct LoadConfig {
    pub connection: ConnectionConfig,
    pub warehouses_per_database: u32,
    pub num_databases: u32,
    pub load_parallelism: usize,
    pub batch_size: usize,
    pub reset: bool,
    pub warehouse_id_offset: u32,
    pub skip_items: bool,
}

#[derive(Debug, Clone)]
pub struct StatusConfig {
    pub connection: ConnectionConfig,
    pub num_databases: u32,
}

#[derive(Debug, Clone)]
pub struct WaitConfig {
    pub connection: ConnectionConfig,
    pub num_databases: u32,
    pub parallelism: usize,
    pub poll_interval_ms: u64,
}

#[derive(Debug, Clone)]
pub struct DriverConfig {
    pub connection: ConnectionConfig,
    pub run_id: Option<String>,
    pub driver_id: String,
    pub warehouse_count: u32,
    pub warehouse_start: u32,
    pub driver_warehouse_count: u32,
    pub warehouses_per_database: u32,
    pub warmup_secs: u64,
    pub measure_secs: u64,
    pub output_dir: Option<PathBuf>,
    pub coordinator_url: Option<String>,
    pub delivery_wait_secs: u64,
    pub connections_per_database: usize,
    pub keying_time_scale: f64,
    pub think_time_scale: f64,
}

#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub connection: ConnectionConfig,
    pub run_id: String,
    pub listen: SocketAddr,
    pub expected_drivers: usize,
    pub warehouses: u32,
    pub warehouses_per_database: u32,
    pub warmup_secs: u64,
    pub measure_secs: u64,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct LoadArgs {
    #[command(flatten)]
    pub connection: ConnectionArgs,
    #[arg(long)]
    pub num_databases: Option<u32>,
    #[arg(long)]
    pub warehouses_per_database: Option<u32>,
    #[arg(long)]
    pub load_parallelism: Option<usize>,
    #[arg(long)]
    pub batch_size: Option<usize>,
    #[arg(long)]
    pub reset: Option<bool>,
    /// Offset added to all warehouse IDs for this load. Use when adding warehouses
    /// to a database that already has data (e.g. set to 70 to load warehouses 71-140
    /// into a database that already has warehouses 1-70).
    #[arg(long)]
    pub warehouse_id_offset: Option<u32>,
    /// Skip loading the global Items table. Use together with --warehouse-id-offset
    /// when adding warehouses to an existing database.
    #[arg(long)]
    pub skip_items: Option<bool>,
}

#[derive(Debug, Clone, Args)]
pub struct StatusArgs {
    #[command(flatten)]
    pub connection: ConnectionArgs,
    #[arg(long)]
    pub num_databases: Option<u32>,
}

#[derive(Debug, Clone, Args)]
pub struct WaitArgs {
    #[command(flatten)]
    pub connection: ConnectionArgs,
    #[arg(long)]
    pub num_databases: Option<u32>,
    #[arg(long)]
    pub parallelism: Option<usize>,
    #[arg(long)]
    pub poll_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Args)]
pub struct DriverArgs {
    #[command(flatten)]
    pub connection: ConnectionArgs,
    #[arg(long)]
    pub run_id: Option<String>,
    #[arg(long)]
    pub driver_id: Option<String>,
    #[arg(long)]
    pub warehouse_start: Option<u32>,
    #[arg(long = "warehouse-count")]
    pub driver_warehouse_count: Option<u32>,
    #[arg(long)]
    pub warehouses: Option<u32>,
    #[arg(long)]
    pub warehouses_per_database: Option<u32>,
    #[arg(long)]
    pub warmup_secs: Option<u64>,
    #[arg(long)]
    pub measure_secs: Option<u64>,
    #[arg(long)]
    pub output_dir: Option<PathBuf>,
    #[arg(long)]
    pub coordinator_url: Option<String>,
    #[arg(long)]
    pub delivery_wait_secs: Option<u64>,
    #[arg(long)]
    pub connections_per_database: Option<usize>,
    #[arg(long)]
    pub keying_time_scale: Option<f64>,
    #[arg(long)]
    pub think_time_scale: Option<f64>,
}

#[derive(Debug, Clone, Args)]
pub struct CoordinatorArgs {
    #[command(flatten)]
    pub connection: ConnectionArgs,
    #[arg(long)]
    pub run_id: Option<String>,
    #[arg(long)]
    pub listen: Option<SocketAddr>,
    #[arg(long)]
    pub expected_drivers: Option<usize>,
    #[arg(long)]
    pub warehouses: Option<u32>,
    #[arg(long)]
    pub warehouses_per_database: Option<u32>,
    #[arg(long)]
    pub warmup_secs: Option<u64>,
    #[arg(long)]
    pub measure_secs: Option<u64>,
    #[arg(long)]
    pub output_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Args)]
pub struct ConnectionArgs {
    #[arg(long)]
    pub uri: Option<String>,
    #[arg(long)]
    pub database_prefix: Option<String>,
    #[arg(long)]
    pub token: Option<String>,
    #[arg(long)]
    pub confirmed_reads: Option<bool>,
    #[arg(long)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FileConfig {
    #[serde(default)]
    connection: FileConnectionConfig,
    #[serde(default)]
    load: FileLoadConfig,
    #[serde(default)]
    wait: FileWaitConfig,
    #[serde(default)]
    driver: FileDriverConfig,
    #[serde(default)]
    coordinator: FileCoordinatorConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileConnectionConfig {
    uri: Option<String>,
    database_prefix: Option<String>,
    token: Option<String>,
    confirmed_reads: Option<bool>,
    timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileLoadConfig {
    num_databases: Option<u32>,
    warehouses_per_database: Option<u32>,
    load_parallelism: Option<usize>,
    batch_size: Option<usize>,
    reset: Option<bool>,
    warehouse_id_offset: Option<u32>,
    skip_items: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileWaitConfig {
    num_databases: Option<u32>,
    parallelism: Option<usize>,
    poll_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileDriverConfig {
    run_id: Option<String>,
    driver_id: Option<String>,
    warehouse_start: Option<u32>,
    #[serde(rename = "warehouse_count")]
    driver_warehouse_count: Option<u32>,
    warehouses: Option<u32>,
    warehouses_per_database: Option<u32>,
    warmup_secs: Option<u64>,
    measure_secs: Option<u64>,
    output_dir: Option<PathBuf>,
    coordinator_url: Option<String>,
    delivery_wait_secs: Option<u64>,
    connections_per_database: Option<usize>,
    keying_time_scale: Option<f64>,
    think_time_scale: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileCoordinatorConfig {
    run_id: Option<String>,
    listen: Option<SocketAddr>,
    expected_drivers: Option<usize>,
    warehouses: Option<u32>,
    warehouses_per_database: Option<u32>,
    warmup_secs: Option<u64>,
    measure_secs: Option<u64>,
    output_dir: Option<PathBuf>,
}

impl FileConfig {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let raw = fs::read_to_string(path).with_context(|| format!("failed to read config {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("failed to parse config {}", path.display()))
    }
}

impl ConnectionArgs {
    fn resolve(&self, file: &FileConnectionConfig) -> ConnectionConfig {
        ConnectionConfig {
            uri: self
                .uri
                .clone()
                .or_else(|| file.uri.clone())
                .unwrap_or_else(|| "http://127.0.0.1:3000".to_string()),
            database_prefix: self
                .database_prefix
                .clone()
                .or_else(|| file.database_prefix.clone())
                .unwrap_or_else(|| "tpcc".to_string()),
            token: self.token.clone().or_else(|| file.token.clone()),
            confirmed_reads: self.confirmed_reads.or(file.confirmed_reads).unwrap_or(true),
            timeout_secs: self.timeout_secs.or(file.timeout_secs).unwrap_or(30),
        }
    }
}

impl LoadArgs {
    pub fn resolve(&self, file: &FileConfig) -> Result<LoadConfig> {
        let num_databases = self.num_databases.or(file.load.num_databases).unwrap_or(1);
        if num_databases == 0 {
            bail!("num_databases must be positive");
        }
        let warehouses_per_database = self
            .warehouses_per_database
            .or(file.load.warehouses_per_database)
            .unwrap_or(1);
        if warehouses_per_database == 0 {
            bail!("warehouses_per_database must be positive");
        }
        let load_parallelism = self
            .load_parallelism
            .or(file.load.load_parallelism)
            .unwrap_or_else(|| usize::try_from(num_databases).unwrap_or(usize::MAX).min(8));
        if load_parallelism == 0 {
            bail!("load_parallelism must be positive");
        }
        let batch_size = self
            .batch_size
            .or(file.load.batch_size)
            .unwrap_or(DEFAULT_LOAD_BATCH_SIZE);
        if batch_size == 0 {
            bail!("batch_size must be positive");
        }
        Ok(LoadConfig {
            connection: self.connection.resolve(&file.connection),
            num_databases,
            warehouses_per_database,
            load_parallelism: load_parallelism.min(usize::try_from(num_databases).unwrap_or(usize::MAX)),
            batch_size,
            reset: self.reset.or(file.load.reset).unwrap_or(true),
            warehouse_id_offset: self.warehouse_id_offset.or(file.load.warehouse_id_offset).unwrap_or(0),
            skip_items: self.skip_items.or(file.load.skip_items).unwrap_or(false),
        })
    }
}

impl StatusArgs {
    pub fn resolve(&self, file: &FileConfig) -> Result<StatusConfig> {
        let num_databases = self.num_databases.or(file.load.num_databases).unwrap_or(1);
        if num_databases == 0 {
            bail!("num_databases must be positive");
        }

        Ok(StatusConfig {
            connection: self.connection.resolve(&file.connection),
            num_databases,
        })
    }
}

impl WaitArgs {
    pub fn resolve(&self, file: &FileConfig) -> Result<WaitConfig> {
        let num_databases = self
            .num_databases
            .or(file.wait.num_databases)
            .or(file.load.num_databases)
            .unwrap_or(1);
        if num_databases == 0 {
            bail!("num_databases must be positive");
        }

        let parallelism = self
            .parallelism
            .or(file.wait.parallelism)
            .or(file.load.load_parallelism)
            .unwrap_or_else(|| usize::try_from(num_databases).unwrap_or(usize::MAX).min(8));
        if parallelism == 0 {
            bail!("parallelism must be positive");
        }

        let poll_interval_ms = self.poll_interval_ms.or(file.wait.poll_interval_ms).unwrap_or(1_000);
        if poll_interval_ms == 0 {
            bail!("poll_interval_ms must be positive");
        }

        Ok(WaitConfig {
            connection: self.connection.resolve(&file.connection),
            num_databases,
            parallelism: parallelism.min(usize::try_from(num_databases).unwrap_or(usize::MAX)),
            poll_interval_ms,
        })
    }
}

impl DriverArgs {
    pub fn resolve(&self, file: &FileConfig) -> Result<DriverConfig> {
        let connection = self.connection.resolve(&file.connection);
        let warehouse_count = self.warehouses.or(file.driver.warehouses).unwrap_or(1);
        let warehouse_start = self.warehouse_start.or(file.driver.warehouse_start).unwrap_or(1);
        if warehouse_start == 0 {
            bail!("warehouse_start must be positive");
        }
        if warehouse_start > warehouse_count {
            bail!(
                "warehouse_start {} exceeds total warehouses {}",
                warehouse_start,
                warehouse_count
            );
        }
        let remaining_warehouses = warehouse_count - warehouse_start + 1;
        let driver_warehouse_count = self
            .driver_warehouse_count
            .or(file.driver.driver_warehouse_count)
            .unwrap_or(remaining_warehouses);
        if driver_warehouse_count == 0 {
            bail!("warehouse_count must be positive");
        }
        let warehouse_end = warehouse_start
            .checked_add(driver_warehouse_count - 1)
            .context("warehouse range overflowed")?;
        if warehouse_end > warehouse_count {
            bail!(
                "warehouse range {}..={} exceeds total warehouses {}",
                warehouse_start,
                warehouse_end,
                warehouse_count
            );
        }
        let warehouses_per_database = self
            .warehouses_per_database
            .or(file.driver.warehouses_per_database)
            .or(file.load.warehouses_per_database)
            .unwrap_or(warehouse_count);
        if warehouses_per_database == 0 {
            bail!("warehouses_per_database must be positive");
        }
        let connections_per_database = self
            .connections_per_database
            .or(file.driver.connections_per_database)
            .unwrap_or(4);
        if connections_per_database == 0 {
            bail!("connections_per_database must be positive");
        }
        Ok(DriverConfig {
            connection,
            run_id: self.run_id.clone().or_else(|| file.driver.run_id.clone()),
            driver_id: self
                .driver_id
                .clone()
                .or_else(|| file.driver.driver_id.clone())
                .unwrap_or_else(default_driver_id),
            warehouse_count,
            warehouse_start,
            driver_warehouse_count,
            warehouses_per_database,
            warmup_secs: self.warmup_secs.or(file.driver.warmup_secs).unwrap_or(5),
            measure_secs: self.measure_secs.or(file.driver.measure_secs).unwrap_or(30),
            output_dir: self.output_dir.clone().or_else(|| file.driver.output_dir.clone()),
            coordinator_url: self
                .coordinator_url
                .clone()
                .or_else(|| file.driver.coordinator_url.clone()),
            delivery_wait_secs: self.delivery_wait_secs.or(file.driver.delivery_wait_secs).unwrap_or(60),
            connections_per_database,
            keying_time_scale: self.keying_time_scale.or(file.driver.keying_time_scale).unwrap_or(1.0),
            think_time_scale: self.think_time_scale.or(file.driver.think_time_scale).unwrap_or(1.0),
        })
    }
}

impl CoordinatorArgs {
    pub fn resolve(&self, file: &FileConfig) -> Result<CoordinatorConfig> {
        let expected_drivers = self.expected_drivers.or(file.coordinator.expected_drivers).unwrap_or(1);
        if expected_drivers == 0 {
            bail!("expected_drivers must be positive");
        }
        let warehouses = self.warehouses.or(file.coordinator.warehouses).unwrap_or(1);
        if warehouses == 0 {
            bail!("warehouses must be positive");
        }
        let warehouses_per_database = self
            .warehouses_per_database
            .or(file.coordinator.warehouses_per_database)
            .unwrap_or(warehouses);
        if warehouses_per_database == 0 {
            bail!("warehouses_per_database must be positive");
        }
        if expected_drivers > usize::try_from(warehouses).unwrap_or(usize::MAX) {
            bail!(
                "expected_drivers {} exceeds total warehouses {}",
                expected_drivers,
                warehouses
            );
        }
        Ok(CoordinatorConfig {
            connection: self.connection.resolve(&file.connection),
            run_id: self
                .run_id
                .clone()
                .or_else(|| file.coordinator.run_id.clone())
                .unwrap_or_else(default_run_id),
            listen: self
                .listen
                .or(file.coordinator.listen)
                .unwrap_or_else(|| "127.0.0.1:7878".parse().expect("hard-coded coordinator address")),
            expected_drivers,
            warehouses,
            warehouses_per_database,
            warmup_secs: self.warmup_secs.or(file.coordinator.warmup_secs).unwrap_or(5),
            measure_secs: self.measure_secs.or(file.coordinator.measure_secs).unwrap_or(30),
            output_dir: self
                .output_dir
                .clone()
                .or_else(|| file.coordinator.output_dir.clone())
                .unwrap_or_else(|| PathBuf::from("tpcc-results/coordinator")),
        })
    }
}

pub fn default_run_id() -> String {
    format!("tpcc-{}", crate::summary::now_millis())
}

pub fn default_driver_id() -> String {
    format!("driver-{}", std::process::id())
}

impl DriverConfig {
    pub fn with_assignment(&self, assignment: &DriverAssignment) -> Self {
        let mut updated = self.clone();
        updated.warehouse_count = assignment.warehouse_count;
        updated.warehouse_start = assignment.warehouse_start;
        updated.driver_warehouse_count = assignment.driver_warehouse_count;
        updated.warehouses_per_database = assignment.warehouses_per_database;
        updated
    }

    pub fn warehouse_end(&self) -> u32 {
        self.warehouse_start + self.driver_warehouse_count - 1
    }

    pub fn terminal_start(&self) -> u32 {
        (self.warehouse_start - 1) * u32::from(crate::tpcc::DISTRICTS_PER_WAREHOUSE) + 1
    }

    pub fn terminals(&self) -> u32 {
        self.driver_warehouse_count * u32::from(crate::tpcc::DISTRICTS_PER_WAREHOUSE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_args_reject_zero_batch_size() {
        let args = LoadArgs {
            connection: ConnectionArgs::default(),
            num_databases: Some(1),
            warehouses_per_database: Some(1),
            load_parallelism: Some(1),
            batch_size: Some(0),
            reset: Some(true),
            warehouse_id_offset: None,
            skip_items: None,
        };

        let err = args.resolve(&FileConfig::default()).unwrap_err().to_string();
        assert!(err.contains("batch_size must be positive"), "{err}");
    }

    #[test]
    fn load_args_default_batch_size_is_500() {
        let args = LoadArgs {
            connection: ConnectionArgs::default(),
            num_databases: Some(1),
            warehouses_per_database: Some(1),
            load_parallelism: Some(1),
            batch_size: None,
            reset: Some(true),
            warehouse_id_offset: None,
            skip_items: None,
        };

        let config = args.resolve(&FileConfig::default()).unwrap();
        assert_eq!(config.batch_size, DEFAULT_LOAD_BATCH_SIZE);
    }

    #[test]
    fn status_args_use_load_num_databases_default() {
        let file = FileConfig {
            load: FileLoadConfig {
                num_databases: Some(4),
                ..FileLoadConfig::default()
            },
            ..FileConfig::default()
        };
        let args = StatusArgs {
            connection: ConnectionArgs::default(),
            num_databases: None,
        };

        let config = args.resolve(&file).unwrap();
        assert_eq!(config.num_databases, 4);
    }

    #[test]
    fn driver_args_default_connections_per_database_is_four() {
        let args = DriverArgs {
            connection: ConnectionArgs::default(),
            run_id: None,
            driver_id: None,
            warehouse_start: Some(1),
            driver_warehouse_count: Some(1),
            warehouses: Some(1),
            warehouses_per_database: Some(1),
            warmup_secs: None,
            measure_secs: None,
            output_dir: None,
            coordinator_url: None,
            delivery_wait_secs: None,
            connections_per_database: None,
            keying_time_scale: None,
            think_time_scale: None,
        };

        let config = args.resolve(&FileConfig::default()).unwrap();
        assert_eq!(config.connections_per_database, 4);
    }

    #[test]
    fn driver_args_reject_zero_connections_per_database() {
        let args = DriverArgs {
            connection: ConnectionArgs::default(),
            run_id: None,
            driver_id: None,
            warehouse_start: Some(1),
            driver_warehouse_count: Some(1),
            warehouses: Some(1),
            warehouses_per_database: Some(1),
            warmup_secs: None,
            measure_secs: None,
            output_dir: None,
            coordinator_url: None,
            delivery_wait_secs: None,
            connections_per_database: Some(0),
            keying_time_scale: None,
            think_time_scale: None,
        };

        let err = args.resolve(&FileConfig::default()).unwrap_err().to_string();
        assert!(err.contains("connections_per_database must be positive"), "{err}");
    }

    #[test]
    fn driver_args_cli_overrides_file_connections_per_database() {
        let file = FileConfig {
            driver: FileDriverConfig {
                connections_per_database: Some(2),
                ..FileDriverConfig::default()
            },
            ..FileConfig::default()
        };
        let args = DriverArgs {
            connection: ConnectionArgs::default(),
            run_id: None,
            driver_id: None,
            warehouse_start: Some(1),
            driver_warehouse_count: Some(1),
            warehouses: Some(1),
            warehouses_per_database: Some(1),
            warmup_secs: None,
            measure_secs: None,
            output_dir: None,
            coordinator_url: None,
            delivery_wait_secs: None,
            connections_per_database: Some(4),
            keying_time_scale: None,
            think_time_scale: None,
        };

        let config = args.resolve(&file).unwrap();
        assert_eq!(config.connections_per_database, 4);
    }

    #[test]
    fn wait_args_use_clamped_parallelism_default() {
        let file = FileConfig {
            load: FileLoadConfig {
                num_databases: Some(3),
                ..FileLoadConfig::default()
            },
            ..FileConfig::default()
        };
        let args = WaitArgs {
            connection: ConnectionArgs::default(),
            num_databases: None,
            parallelism: Some(5),
            poll_interval_ms: None,
        };

        let config = args.resolve(&file).unwrap();
        assert_eq!(config.num_databases, 3);
        assert_eq!(config.parallelism, 3);
        assert_eq!(config.poll_interval_ms, 1_000);
    }
}
