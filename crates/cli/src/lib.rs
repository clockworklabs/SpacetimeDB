pub mod api;
mod common_args;
mod config;
mod edit_distance;
mod errors;
mod subcommands;
mod tasks;
pub mod util;

use clap::{ArgMatches, Command};

pub use config::Config;
use spacetimedb_standalone::subcommands::start::ProgramMode;
pub use subcommands::*;
pub use tasks::build;

#[cfg(feature = "standalone")]
use spacetimedb_standalone::subcommands::start;

pub fn get_subcommands() -> Vec<Command> {
    vec![
        version::cli(),
        publish::cli(),
        delete::cli(),
        logs::cli(),
        call::cli(),
        describe::cli(),
        identity::cli(),
        energy::cli(),
        sql::cli(),
        dns::cli(),
        generate::cli(),
        list::cli(),
        init::cli(),
        build::cli(),
        server::cli(),
        upgrade::cli(),
        subscribe::cli(),
        #[cfg(feature = "standalone")]
        start::cli(ProgramMode::CLI),
    ]
}

pub async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "version" => version::exec(config, args).await,
        "identity" => identity::exec(config, args).await,
        "call" => call::exec(config, args).await,
        "describe" => describe::exec(config, args).await,
        "energy" => energy::exec(config, args).await,
        "publish" => publish::exec(config, args).await,
        "delete" => delete::exec(config, args).await,
        "logs" => logs::exec(config, args).await,
        "sql" => sql::exec(config, args).await,
        "dns" => dns::exec(config, args).await,
        "generate" => generate::exec(config, args).await,
        "list" => list::exec(config, args).await,
        "init" => init::exec(config, args).await,
        "build" => build::exec(config, args).await.map(drop),
        "server" => server::exec(config, args).await,
        "subscribe" => subscribe::exec(config, args).await,
        #[cfg(feature = "standalone")]
        "start" => start::exec(args).await,
        "upgrade" => upgrade::exec(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}
