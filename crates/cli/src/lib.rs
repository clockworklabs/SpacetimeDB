pub mod api;
mod common_args;
mod config;
pub(crate) mod detect;
mod edit_distance;
mod errors;
mod subcommands;
mod tasks;
pub mod util;

use std::process::ExitCode;

use clap::{ArgMatches, Command};

pub use config::Config;
use spacetimedb_paths::SpacetimePaths;
pub use subcommands::*;
pub use tasks::build;

pub fn get_subcommands() -> Vec<Command> {
    vec![
        version::cli(),
        publish::cli(),
        delete::cli(),
        logs::cli(),
        call::cli(),
        describe::cli(),
        energy::cli(),
        sql::cli(),
        dns::cli(),
        generate::cli(),
        list::cli(),
        login::cli(),
        logout::cli(),
        init::cli(),
        build::cli(),
        server::cli(),
        upgrade::cli(),
        subscribe::cli(),
        start::cli(),
    ]
}

pub async fn exec_subcommand(
    config: Config,
    paths: &SpacetimePaths,
    cmd: &str,
    args: &ArgMatches,
) -> anyhow::Result<ExitCode> {
    match cmd {
        "version" => version::exec(config, args).await,
        "call" => call::exec(config, args).await,
        "describe" => describe::exec(config, args).await,
        "energy" => energy::exec(config, args).await,
        "publish" => publish::exec(config, args).await,
        "delete" => delete::exec(config, args).await,
        "logs" => logs::exec(config, args).await,
        "sql" => sql::exec(config, args).await,
        "rename" => dns::exec(config, args).await,
        "generate" => generate::exec(config, args).await,
        "list" => list::exec(config, args).await,
        "init" => init::exec(config, args).await,
        "build" => build::exec(config, args).await.map(drop),
        "server" => server::exec(config, paths, args).await,
        "subscribe" => subscribe::exec(config, args).await,
        "start" => return start::exec(paths, args).await,
        "login" => login::exec(config, args).await,
        "logout" => logout::exec(config, args).await,
        "upgrade" => upgrade::exec(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
    .map(|()| ExitCode::SUCCESS)
}
