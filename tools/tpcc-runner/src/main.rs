mod client;
mod config;
mod coordinator;
mod driver;
mod loader;
mod module_bindings;
mod protocol;
mod summary;
mod tpcc;

use clap::Parser;
use config::{Cli, Command, FileConfig};
use env_logger::Env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("tpcc_runner=info")).init();

    let cli = Cli::parse();
    let file_config = FileConfig::load(cli.config.as_deref())?;

    match cli.command {
        Command::Load(args) => loader::run(args.resolve(&file_config)).await,
        Command::Driver(args) => driver::run(args.resolve(&file_config)?).await,
        Command::Coordinator(args) => coordinator::run(args.resolve(&file_config)?).await,
    }
}
