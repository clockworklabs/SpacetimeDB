pub mod api;
mod common_args;
mod config;
pub(crate) mod detect;
mod edit_distance;
mod errors;
mod subcommands;
mod tasks;
pub mod util;
pub mod version;

use std::process::ExitCode;

use clap::{ArgMatches, Command};

pub use config::Config;
use spacetimedb_paths::{RootDir, SpacetimePaths};
pub use subcommands::*;
pub use tasks::build;

pub fn get_subcommands() -> Vec<Command> {
    vec![
        publish::cli(),
        delete::cli(),
        logs::cli(),
        call::cli(),
        describe::cli(),
        dev::cli(),
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
        subscribe::cli(),
        start::cli(),
        subcommands::version::cli(),
    ]
}

pub async fn exec_subcommand(
    config: Config,
    paths: &SpacetimePaths,
    root_dir: Option<&RootDir>,
    cmd: &str,
    args: &ArgMatches,
) -> anyhow::Result<ExitCode> {
    match cmd {
        "call" => call::exec(config, args).await,
        "describe" => describe::exec(config, args).await,
        "dev" => dev::exec(config, args).await,
        "energy" => energy::exec(config, args).await,
        "publish" => publish::exec(config, args).await,
        "delete" => delete::exec(config, args).await,
        "logs" => logs::exec(config, args).await,
        "sql" => sql::exec(config, args).await,
        "rename" => dns::exec(config, args).await,
        "generate" => generate::exec(config, args).await,
        "list" => list::exec(config, args).await,
        "init" => init::exec(config, args).await.map(|_| ()),
        "build" => build::exec(config, args).await.map(drop),
        "server" => server::exec(config, paths, args).await,
        "subscribe" => subscribe::exec(config, args).await,
        "start" => return start::exec(paths, args).await,
        "login" => login::exec(config, args).await,
        "logout" => logout::exec(config, args).await,
        "version" => return subcommands::version::exec(paths, root_dir, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {unknown}")),
    }
    .map(|()| ExitCode::SUCCESS)
}
