mod config;
mod subcommands;
mod util;
use crate::config::Config;
use anyhow;
use clap::ArgMatches;
use clap::Command;
use std::vec;
use subcommands::*;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = Config::load();
    // Save a default version to disk
    config.save();

    let (cmd, subcommand_args) = util::match_subcommand_or_exit(get_command());
    exec_subcommand(config, &cmd, &subcommand_args).await?;

    Ok(())
}

fn get_command() -> Command<'static> {
    Command::new("spacetime")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_subcommands())
        .help_template(
            "\
┌──────────────────────────────────────────────────────────┐
│ SpacetimeDB Command Line Tool                            │
│ Easily interact with a SpacetimeDB cluster               │
│                                                          │
│ Please give us feedback at:                              │
│ https://github.com/clockworklabs/SpacetimeDB/issues      │
└──────────────────────────────────────────────────────────┘
Usage:
{usage}

Options:
{options}

Commands:
{subcommands}
",
        )
}

fn get_subcommands() -> Vec<Command<'static>> {
    vec![
        version::cli(),
        init::cli(),
        update::cli(),
        rm::cli(),
        logs::cli(),
        call::cli(),
        describe::cli(),
        identity::cli(),
        energy::cli(),
        sql::cli(),
        name::cli(),
    ]
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "version" => version::exec(config, args).await,
        "identity" => identity::exec(config, args).await,
        "call" => call::exec(config, args).await,
        "describe" => describe::exec(config, args).await,
        "energy" => energy::exec(config, args).await,
        "init" => init::exec(config, args).await,
        "rm" => rm::exec(config, args).await,
        "logs" => logs::exec(config, args).await,
        "sql" => sql::exec(config, args).await,
        "update" => update::exec(config, args).await,
        "name" => name::exec(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}
