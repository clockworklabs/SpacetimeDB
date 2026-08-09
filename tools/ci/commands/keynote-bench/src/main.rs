use anyhow::{bail, Result};
use keynote_bench_harness::KeynoteBenchConfig;
use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "-h" || arg == "--help") {
        println!("Usage: cargo ci keynote-bench");
        return Ok(());
    }
    if !args.is_empty() {
        bail!("cargo ci keynote-bench does not accept arguments");
    }

    let cli_path = ensure_binaries_built();
    let server = SpacetimeDbGuard::spawn_in_temp_data_dir();
    let server_url = server.host_url.clone();

    keynote_bench_harness::run(KeynoteBenchConfig::standalone(".", cli_path, server_url))
}
