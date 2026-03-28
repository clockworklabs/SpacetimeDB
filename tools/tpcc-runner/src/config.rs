use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::Deserialize;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

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
    pub warehouses_per_database: u16,
    pub num_databases: u16,
    pub batch_size: usize,
    pub reset: bool,
}

#[derive(Debug, Clone)]
pub struct DriverConfig {
    pub connection: ConnectionConfig,
    pub run_id: Option<String>,
    pub driver_id: String,
    pub warehouse_count: u16,
    pub warehouse_start: u16,
    pub driver_warehouse_count: u16,
    pub warehouses_per_database: u16,
    pub warmup_secs: u64,
    pub measure_secs: u64,
    pub output_dir: Option<PathBuf>,
    pub coordinator_url: Option<String>,
    pub delivery_wait_secs: u64,
    pub keying_time_scale: f64,
    pub think_time_scale: f64,
}

#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub run_id: String,
    pub listen: SocketAddr,
    pub expected_drivers: usize,
    pub warmup_secs: u64,
    pub measure_secs: u64,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct LoadArgs {
    #[command(flatten)]
    pub connection: ConnectionArgs,
    #[arg(long)]
    pub num_databases: Option<u16>,
    #[arg(long)]
    pub warehouses_per_database: Option<u16>,
    #[arg(long)]
    pub batch_size: Option<usize>,
    #[arg(long)]
    pub reset: Option<bool>,
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
    pub warehouse_start: Option<u16>,
    #[arg(long = "warehouse-count")]
    pub driver_warehouse_count: Option<u16>,
    #[arg(long)]
    pub warehouses: Option<u16>,
    #[arg(long)]
    pub warehouses_per_database: Option<u16>,
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
    pub keying_time_scale: Option<f64>,
    #[arg(long)]
    pub think_time_scale: Option<f64>,
}

#[derive(Debug, Clone, Args)]
pub struct CoordinatorArgs {
    #[arg(long)]
    pub run_id: Option<String>,
    #[arg(long)]
    pub listen: Option<SocketAddr>,
    #[arg(long)]
    pub expected_drivers: Option<usize>,
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
    num_databases: Option<u16>,
    warehouses_per_database: Option<u16>,
    batch_size: Option<usize>,
    reset: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileDriverConfig {
    run_id: Option<String>,
    driver_id: Option<String>,
    warehouse_start: Option<u16>,
    #[serde(rename = "warehouse_count")]
    driver_warehouse_count: Option<u16>,
    warehouses: Option<u16>,
    warehouses_per_database: Option<u16>,
    warmup_secs: Option<u64>,
    measure_secs: Option<u64>,
    output_dir: Option<PathBuf>,
    coordinator_url: Option<String>,
    delivery_wait_secs: Option<u64>,
    keying_time_scale: Option<f64>,
    think_time_scale: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileCoordinatorConfig {
    run_id: Option<String>,
    listen: Option<SocketAddr>,
    expected_drivers: Option<usize>,
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
    pub fn resolve(&self, file: &FileConfig) -> LoadConfig {
        LoadConfig {
            connection: self.connection.resolve(&file.connection),
            num_databases: self.num_databases.or(file.load.num_databases).unwrap_or(1),
            warehouses_per_database: self
                .warehouses_per_database
                .or(file.load.warehouses_per_database)
                .unwrap_or(1),
            batch_size: self.batch_size.or(file.load.batch_size).unwrap_or(500),
            reset: self.reset.or(file.load.reset).unwrap_or(true),
        }
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
        Ok(CoordinatorConfig {
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
    pub fn warehouse_end(&self) -> u16 {
        self.warehouse_start + self.driver_warehouse_count - 1
    }

    pub fn terminal_start(&self) -> u32 {
        (u32::from(self.warehouse_start) - 1) * u32::from(crate::tpcc::DISTRICTS_PER_WAREHOUSE) + 1
    }

    pub fn terminals(&self) -> u32 {
        u32::from(self.driver_warehouse_count) * u32::from(crate::tpcc::DISTRICTS_PER_WAREHOUSE)
    }
}
