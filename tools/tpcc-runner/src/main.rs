use clap::Parser;
use tpcc_runner::{config::Cli, init_logging, run_cli};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    run_cli(Cli::parse()).await
}
