#![allow(clippy::disallowed_macros)]

use anyhow::{bail, ensure, Context, Result};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const DATABASE_NAME: &str = "test-1";
const KEYNOTE_DIR: &str = "templates/keynote-2";
const KEYNOTE_BINDINGS_DIR: &str = "templates/keynote-2/module_bindings";
const BENCH_SECONDS: &str = "60";
const BENCH_CONCURRENCY: &str = "64";
const MAX_INFLIGHT_PER_WORKER: &str = "96";
const SEED_ACCOUNTS: &str = "100000";
const SEED_INITIAL_BALANCE: &str = "1000000000000";
const TRANSFER_REDUCER: &str = "transfer";
const REDUCER_FUEL_METRIC: &str = "reducer_wasmtime_fuel_used";
const REDUCER_FUEL_METRIC_TOTAL: &str = "reducer_wasmtime_fuel_used_total";
const MAX_FUEL_RATIO: f64 = 2.0;

#[derive(Debug, Clone)]
pub struct KeynoteBenchConfig {
    public_root: PathBuf,
    cli_path: PathBuf,
    server_url: String,
    login_mode: LoginMode,
    publish_confirmation: PublishConfirmation,
}

impl KeynoteBenchConfig {
    pub fn standalone(
        public_root: impl Into<PathBuf>,
        cli_path: impl Into<PathBuf>,
        server_url: impl Into<String>,
    ) -> Self {
        Self {
            public_root: public_root.into(),
            cli_path: cli_path.into(),
            server_url: server_url.into(),
            login_mode: LoginMode::None,
            publish_confirmation: PublishConfirmation::Basic,
        }
    }

    pub fn remote_server(
        public_root: impl Into<PathBuf>,
        cli_path: impl Into<PathBuf>,
        server_url: impl Into<String>,
    ) -> Self {
        Self {
            public_root: public_root.into(),
            cli_path: cli_path.into(),
            server_url: server_url.into(),
            login_mode: LoginMode::ServerIssued,
            publish_confirmation: PublishConfirmation::RemoteMigrateBreakClientsDeleteData,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum LoginMode {
    None,
    ServerIssued,
}

#[derive(Debug, Clone, Copy)]
enum PublishConfirmation {
    Basic,
    RemoteMigrateBreakClientsDeleteData,
}

impl PublishConfirmation {
    fn as_cli_arg(self) -> &'static str {
        match self {
            Self::Basic => "--yes",
            Self::RemoteMigrateBreakClientsDeleteData => "--yes=remote,migrate,break-clients,delete-data",
        }
    }
}

struct BenchmarkModule {
    label: &'static str,
    module_dir: &'static str,
    min_tps: f64,
}

struct BenchmarkResult {
    label: &'static str,
    transfer_fuel_total: f64,
}

const BENCHMARK_MODULES: &[BenchmarkModule] = &[
    BenchmarkModule {
        label: "TypeScript",
        module_dir: "templates/keynote-2/spacetimedb",
        min_tps: 250_000.0,
    },
    BenchmarkModule {
        label: "Rust",
        module_dir: "templates/keynote-2/rust_module",
        min_tps: 250_000.0,
    },
];

pub fn ensure_release_cli_built(root: &Path) -> Result<PathBuf> {
    let cli_name = if cfg!(windows) {
        "spacetimedb-cli.exe"
    } else {
        "spacetimedb-cli"
    };
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("target"));
    let cli_path = target_dir.join("release").join(cli_name);
    ensure!(
        cli_path.exists(),
        "CLI binary not found at {}; build it with `cargo build --release -p spacetimedb-cli`",
        cli_path.display()
    );
    Ok(cli_path)
}

pub fn run(config: KeynoteBenchConfig) -> Result<()> {
    ensure!(
        config.public_root.join(KEYNOTE_DIR).exists(),
        "keynote benchmark template not found at {}",
        config.public_root.join(KEYNOTE_DIR).display()
    );

    let cli_config_dir = tempfile::tempdir().context("failed to create temporary CLI config directory")?;
    let cli_config_path = cli_config_dir.path().join("config.toml");

    if let LoginMode::ServerIssued = config.login_mode {
        login_with_server_issued_identity(
            &config.public_root,
            &config.cli_path,
            &cli_config_path,
            &config.server_url,
        )?;
    }

    let mut results = Vec::with_capacity(BENCHMARK_MODULES.len());
    for module in BENCHMARK_MODULES {
        results.push(run_module_benchmark(
            module,
            &config.public_root,
            &config.cli_path,
            &cli_config_path,
            &config.server_url,
            config.publish_confirmation,
        )?);
    }
    check_transfer_fuel_ratio(&results)?;

    Ok(())
}

fn run_module_benchmark(
    module: &BenchmarkModule,
    public_root: &Path,
    cli_path: &Path,
    config_path: &Path,
    server_url: &str,
    publish_confirmation: PublishConfirmation,
) -> Result<BenchmarkResult> {
    eprintln!(
        "Running keynote benchmark against {} module ({})...",
        module.label, module.module_dir
    );

    publish_module(
        module,
        public_root,
        cli_path,
        config_path,
        server_url,
        publish_confirmation,
    )?;
    generate_module_bindings(module, public_root, cli_path, config_path)?;
    seed_accounts(public_root, cli_path, config_path, server_url)?;
    let runs_dir = tempfile::tempdir().context("failed to create temporary benchmark runs directory")?;
    let transfer_fuel_before = transfer_fuel_total(server_url)?;
    run_benchmark(module, public_root, server_url, runs_dir.path())?;
    let transfer_fuel_after = transfer_fuel_total(server_url)?;
    let transfer_fuel_total = transfer_fuel_after - transfer_fuel_before;
    ensure!(
        transfer_fuel_total > 0.0,
        "{} keynote benchmark did not record any fuel for the {TRANSFER_REDUCER} reducer",
        module.label
    );

    let result_path = find_result_json(runs_dir.path())?;
    let result_json = fs::read_to_string(&result_path)
        .with_context(|| format!("failed to read benchmark result {}", result_path.display()))?;
    let tps = result_tps(&result_json)?;

    if tps < module.min_tps {
        eprintln!(
            "Keynote perf regression for {} module: throughput {tps:.0} TPS < {:.0} TPS\n\nResult JSON:\n{}",
            module.label, module.min_tps, result_json
        );
        bail!(
            "keynote benchmark throughput for {} module is below threshold",
            module.label
        );
    }

    println!(
        "Keynote perf check passed for {} module: throughput {tps:.0} TPS >= {:.0} TPS; \
         {TRANSFER_REDUCER} fuel total {transfer_fuel_total:.0} ({})",
        module.label,
        module.min_tps,
        result_path.display()
    );
    Ok(BenchmarkResult {
        label: module.label,
        transfer_fuel_total,
    })
}

fn login_with_server_issued_identity(
    public_root: &Path,
    cli_path: &Path,
    config_path: &Path,
    server_url: &str,
) -> Result<()> {
    run_cli(
        public_root,
        cli_path,
        config_path,
        &["login", "--server-issued-login", server_url],
        "spacetime login",
    )
}

fn publish_module(
    module: &BenchmarkModule,
    public_root: &Path,
    cli_path: &Path,
    config_path: &Path,
    server_url: &str,
    publish_confirmation: PublishConfirmation,
) -> Result<()> {
    let label = format!("spacetime publish keynote {} module", module.label);
    run_cli(
        public_root,
        cli_path,
        config_path,
        &[
            "publish",
            "--server",
            server_url,
            "--module-path",
            module.module_dir,
            publish_confirmation.as_cli_arg(),
            "--clear-database",
            DATABASE_NAME,
        ],
        &label,
    )
}

fn generate_module_bindings(
    module: &BenchmarkModule,
    public_root: &Path,
    cli_path: &Path,
    config_path: &Path,
) -> Result<()> {
    let label = format!("spacetime generate keynote {} TypeScript bindings", module.label);
    run_cli(
        public_root,
        cli_path,
        config_path,
        &[
            "generate",
            "--lang",
            "typescript",
            "--out-dir",
            KEYNOTE_BINDINGS_DIR,
            "--module-path",
            module.module_dir,
            "--yes",
        ],
        &label,
    )
}

fn seed_accounts(public_root: &Path, cli_path: &Path, config_path: &Path, server_url: &str) -> Result<()> {
    run_cli(
        public_root,
        cli_path,
        config_path,
        &[
            "call",
            "--server",
            server_url,
            DATABASE_NAME,
            "seed",
            SEED_ACCOUNTS,
            SEED_INITIAL_BALANCE,
        ],
        "spacetime call seed",
    )
}

fn run_cli(public_root: &Path, cli_path: &Path, config_path: &Path, args: &[&str], label: &str) -> Result<()> {
    let mut cmd = Command::new(cli_path);
    cmd.arg("--config-path")
        .arg(config_path)
        .args(args)
        .current_dir(public_root);
    run_command(&mut cmd, label)
}

fn run_benchmark(module: &BenchmarkModule, public_root: &Path, server_url: &str, runs_dir: &Path) -> Result<()> {
    let mut cmd = Command::new("pnpm");
    cmd.args([
        "run",
        "bench",
        DATABASE_NAME,
        "--seconds",
        BENCH_SECONDS,
        "--concurrency",
        BENCH_CONCURRENCY,
        "--connectors",
        "spacetimedb",
    ])
    .current_dir(public_root.join(KEYNOTE_DIR))
    .env("NODE_ENV", "production")
    .env("BENCH_PIPELINED", "1")
    .env("MAX_INFLIGHT_PER_WORKER", MAX_INFLIGHT_PER_WORKER)
    .env("BENCH_RUNS_DIR", runs_dir)
    .env("STDB_URL", server_url)
    .env("STDB_MODULE", DATABASE_NAME)
    .env("SEED_ACCOUNTS", SEED_ACCOUNTS)
    .env("SEED_INITIAL_BALANCE", SEED_INITIAL_BALANCE);
    let label = format!("keynote SpacetimeDB benchmark against {} module", module.label);
    run_command(&mut cmd, &label)
}

fn run_command(cmd: &mut Command, label: &str) -> Result<()> {
    let status = cmd.status().with_context(|| format!("failed to spawn {label}"))?;
    ensure!(status.success(), "{label} failed with status {status}");
    Ok(())
}

fn find_result_json(runs_dir: &Path) -> Result<PathBuf> {
    let mut matches = Vec::new();
    for entry in fs::read_dir(runs_dir).with_context(|| format!("failed to read {}", runs_dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with("test-1-spacetimedb-") && name.ends_with(".json") {
            matches.push(path);
        }
    }

    match matches.len() {
        0 => bail!(
            "benchmark did not write a test-1-spacetimedb result JSON in {}",
            runs_dir.display()
        ),
        1 => Ok(matches.remove(0)),
        _ => bail!(
            "benchmark wrote multiple test-1-spacetimedb result JSON files in {}: {:?}",
            runs_dir.display(),
            matches
        ),
    }
}

fn result_tps(result_json: &str) -> Result<f64> {
    let value: Value = serde_json::from_str(result_json).context("failed to parse benchmark result JSON")?;
    value
        .pointer("/results/0/res/tps")
        .and_then(Value::as_f64)
        .context("benchmark result JSON is missing results[0].res.tps")
}

fn transfer_fuel_total(server_url: &str) -> Result<f64> {
    let metrics_url = format!("{}/v1/metrics", server_url.trim_end_matches('/'));
    let metrics = reqwest::blocking::get(&metrics_url)
        .with_context(|| format!("failed to fetch metrics from {metrics_url}"))?
        .error_for_status()
        .with_context(|| format!("metrics request to {metrics_url} failed"))?
        .text()
        .context("failed to read metrics response body")?;

    let transfer_label = format!(r#"reducer="{TRANSFER_REDUCER}""#);
    let mut total = 0.0;
    for line in metrics.lines() {
        if !is_reducer_fuel_metric_line(line) || !line.contains(&transfer_label) {
            continue;
        }
        let value = line
            .split_whitespace()
            .nth(1)
            .with_context(|| format!("malformed {REDUCER_FUEL_METRIC} metric line: {line}"))?
            .parse::<f64>()
            .with_context(|| format!("invalid {REDUCER_FUEL_METRIC} metric value in line: {line}"))?;
        total += value;
    }
    Ok(total)
}

fn is_reducer_fuel_metric_line(line: &str) -> bool {
    line.starts_with(REDUCER_FUEL_METRIC) || line.starts_with(REDUCER_FUEL_METRIC_TOTAL)
}

fn check_transfer_fuel_ratio(results: &[BenchmarkResult]) -> Result<()> {
    ensure!(
        results.len() == 2,
        "expected exactly two keynote benchmark results to compare fuel usage, got {}",
        results.len()
    );
    let [first, second] = results else {
        unreachable!("length was checked above")
    };

    let higher = first.transfer_fuel_total.max(second.transfer_fuel_total);
    let lower = first.transfer_fuel_total.min(second.transfer_fuel_total);
    ensure!(
        lower > 0.0,
        "keynote benchmark fuel totals must be nonzero: {}={:.0}, {}={:.0}",
        first.label,
        first.transfer_fuel_total,
        second.label,
        second.transfer_fuel_total
    );

    let ratio = higher / lower;
    println!(
        "Keynote transfer fuel comparison: {}={:.0}, {}={:.0}, relative difference {ratio:.2}x",
        first.label, first.transfer_fuel_total, second.label, second.transfer_fuel_total
    );
    ensure!(
        ratio < MAX_FUEL_RATIO,
        "keynote benchmark transfer fuel totals differ by {ratio:.2}x, expected strictly less than {MAX_FUEL_RATIO}x: \
         {}={:.0}, {}={:.0}",
        first.label,
        first.transfer_fuel_total,
        second.label,
        second.transfer_fuel_total
    );

    Ok(())
}
