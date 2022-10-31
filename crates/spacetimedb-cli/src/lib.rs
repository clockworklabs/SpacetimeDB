mod config;
mod subcommands;
pub mod util;

use clap::{ArgMatches, Command};

pub use config::Config;
pub use subcommands::*;

pub fn get_subcommands() -> Vec<Command<'static>> {
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
        name::cli(),
        codegen::cli(),
        init::cli(),
        build::cli(),
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
        "name" => name::exec(config, args).await,
        "generate" => codegen::exec(args),
        "init" => init::exec(config, args).await,
        "build" => build::exec(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}
