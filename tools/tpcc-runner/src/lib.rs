mod client;
pub mod config;
pub mod coordinator;
pub mod driver;
pub mod loader;
mod module_bindings;
mod protocol;
pub mod summary;
mod topology;
mod tpcc;

use config::{Cli, Command, FileConfig};
use env_logger::Env;

pub fn init_logging() {
    let _ = env_logger::Builder::from_env(Env::default().default_filter_or("tpcc_runner=info")).try_init();
}

pub async fn run_cli(cli: Cli) -> anyhow::Result<()> {
    let file_config = FileConfig::load(cli.config.as_deref())?;

    match cli.command {
        Command::Load(args) => loader::run(args.resolve(&file_config)?).await,
        Command::Driver(args) => driver::run(args.resolve(&file_config)?).await,
        Command::Coordinator(args) => coordinator::run(args.resolve(&file_config)?).await,
    }
}
