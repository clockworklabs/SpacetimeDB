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
    pub database: String,
    pub token: Option<String>,
    pub confirmed_reads: bool,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct LoadConfig {
    pub connection: ConnectionConfig,
    pub warehouses: u16,
    pub batch_size: usize,
    pub reset: bool,
}

#[derive(Debug, Clone)]
pub struct DriverConfig {
    pub connection: ConnectionConfig,
    pub run_id: Option<String>,
    pub driver_id: String,
    pub terminal_start: u32,
    pub terminals: u32,
    pub warehouse_count: u16,
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
    pub warehouses: Option<u16>,
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
    pub terminal_start: Option<u32>,
    #[arg(long)]
    pub terminals: Option<u32>,
    #[arg(long)]
    pub warehouses: Option<u16>,
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
    pub database: Option<String>,
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
    database: Option<String>,
    token: Option<String>,
    confirmed_reads: Option<bool>,
    timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileLoadConfig {
    warehouses: Option<u16>,
    batch_size: Option<usize>,
    reset: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileDriverConfig {
    run_id: Option<String>,
    driver_id: Option<String>,
    terminal_start: Option<u32>,
    terminals: Option<u32>,
    warehouses: Option<u16>,
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
            database: self
                .database
                .clone()
                .or_else(|| file.database.clone())
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
            warehouses: self.warehouses.or(file.load.warehouses).unwrap_or(1),
            batch_size: self.batch_size.or(file.load.batch_size).unwrap_or(500),
            reset: self.reset.or(file.load.reset).unwrap_or(true),
        }
    }
}

impl DriverArgs {
    pub fn resolve(&self, file: &FileConfig) -> Result<DriverConfig> {
        let connection = self.connection.resolve(&file.connection);
        let warehouse_count = self.warehouses.or(file.driver.warehouses).unwrap_or(1);
        let terminals = self
            .terminals
            .or(file.driver.terminals)
            .unwrap_or(u32::from(warehouse_count) * 10);
        let terminal_start = self.terminal_start.or(file.driver.terminal_start).unwrap_or(1);
        if terminals == 0 {
            bail!("terminal count must be positive");
        }
        Ok(DriverConfig {
            connection,
            run_id: self.run_id.clone().or_else(|| file.driver.run_id.clone()),
            driver_id: self
                .driver_id
                .clone()
                .or_else(|| file.driver.driver_id.clone())
                .unwrap_or_else(default_driver_id),
            terminal_start,
            terminals,
            warehouse_count,
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
