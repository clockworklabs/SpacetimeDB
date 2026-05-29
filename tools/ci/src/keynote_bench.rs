#![allow(clippy::disallowed_macros)]

use anyhow::{bail, ensure, Context, Result};
use serde_json::Value;
use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};
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

struct BenchmarkModule {
    label: &'static str,
    module_dir: &'static str,
    min_tps: f64,
}

const BENCHMARK_MODULES: &[BenchmarkModule] = &[
    BenchmarkModule {
        label: "TypeScript",
        module_dir: "templates/keynote-2/spacetimedb",
        min_tps: 275_000.0,
    },
    BenchmarkModule {
        label: "Rust",
        module_dir: "templates/keynote-2/rust_module",
        min_tps: 300_000.0,
    },
];

pub fn run() -> Result<()> {
    let cli_path = ensure_binaries_built();
    let server = SpacetimeDbGuard::spawn_in_temp_data_dir();
    let cli_config_dir = tempfile::tempdir().context("failed to create temporary CLI config directory")?;
    let cli_config_path = cli_config_dir.path().join("config.toml");

    for module in BENCHMARK_MODULES {
        run_module_benchmark(module, &cli_path, &cli_config_path, &server.host_url)?;
    }

    Ok(())
}

fn run_module_benchmark(module: &BenchmarkModule, cli_path: &Path, config_path: &Path, server_url: &str) -> Result<()> {
    eprintln!(
        "Running keynote benchmark against {} module ({})...",
        module.label, module.module_dir
    );

    publish_module(module, cli_path, config_path, server_url)?;
    generate_module_bindings(module, cli_path, config_path)?;
    seed_accounts(cli_path, config_path, server_url)?;
    let runs_dir = tempfile::tempdir().context("failed to create temporary benchmark runs directory")?;
    run_benchmark(module, server_url, runs_dir.path())?;

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
        "Keynote perf check passed for {} module: throughput {tps:.0} TPS >= {:.0} TPS ({})",
        module.label,
        module.min_tps,
        result_path.display()
    );
    Ok(())
}

fn publish_module(module: &BenchmarkModule, cli_path: &Path, config_path: &Path, server_url: &str) -> Result<()> {
    let label = format!("spacetime publish keynote {} module", module.label);
    run_cli(
        cli_path,
        config_path,
        &[
            "publish",
            "--server",
            server_url,
            "--module-path",
            module.module_dir,
            "--yes",
            "--clear-database",
            DATABASE_NAME,
        ],
        &label,
    )
}

fn generate_module_bindings(module: &BenchmarkModule, cli_path: &Path, config_path: &Path) -> Result<()> {
    let label = format!("spacetime generate keynote {} TypeScript bindings", module.label);
    run_cli(
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

fn seed_accounts(cli_path: &Path, config_path: &Path, server_url: &str) -> Result<()> {
    run_cli(
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

fn run_cli(cli_path: &Path, config_path: &Path, args: &[&str], label: &str) -> Result<()> {
    let mut cmd = Command::new(cli_path);
    cmd.arg("--config-path").arg(config_path).args(args);
    run_command(&mut cmd, label)
}

fn run_benchmark(module: &BenchmarkModule, server_url: &str, runs_dir: &Path) -> Result<()> {
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
    .current_dir(KEYNOTE_DIR)
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
