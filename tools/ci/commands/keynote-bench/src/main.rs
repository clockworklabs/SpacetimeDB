use anyhow::Result;
use clap::Parser;
use keynote_bench_harness::KeynoteBenchConfig;
use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};

/// Runs the keynote benchmark as a CI performance regression gate.
///
/// Assumes release SpacetimeDB binaries and the TypeScript SDK are already built, runs the
/// keynote SpacetimeDB benchmark for 60 seconds against the TypeScript and Rust modules, and
/// fails if throughput is below 275K TPS for TypeScript or 300K TPS for Rust.
#[derive(Parser)]
struct Cli {}

fn main() -> Result<()> {
    Cli::parse();

    let cli_path = ensure_binaries_built();
    let server = SpacetimeDbGuard::spawn_in_temp_data_dir();
    let server_url = server.host_url.clone();

    keynote_bench_harness::run(KeynoteBenchConfig::standalone(".", cli_path, server_url))
}
