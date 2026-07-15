use anyhow::Result;
use keynote_bench_harness::KeynoteBenchConfig;
use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};

pub fn run() -> Result<()> {
    let cli_path = ensure_binaries_built();
    let server = SpacetimeDbGuard::spawn_in_temp_data_dir();
    let server_url = server.host_url.clone();

    keynote_bench_harness::run(KeynoteBenchConfig::standalone(".", cli_path, server_url))
}
