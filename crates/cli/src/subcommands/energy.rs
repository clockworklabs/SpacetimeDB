// use clap::Arg;
use crate::common_args;
use clap::ArgMatches;
use spacetimedb_lib::Identity;

use crate::config::Config;

pub fn cli() -> clap::Command {
    clap::Command::new("energy")
        .about("Invokes commands related to database budgets")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_energy_subcommands())
}

fn get_energy_subcommands() -> Vec<clap::Command> {
    vec![clap::Command::new("status")
        .about("Show current energy balance for an identity")
        .arg(
            common_args::identity()
                .help("The identity to check the balance for")
                .long_help(
                    "The identity to check the balance for. If no identity is provided, the default one will be used.",
                ),
        )
        .arg(
            common_args::server()
                .help("The nickname, host name or URL of the server from which to request balance information"),
        )]
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "status" => exec_status(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    exec_subcommand(config, cmd, subcommand_args).await
}

async fn exec_status(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    // let project_name = args.value_of("project name").unwrap();
    let identity = args.get_one::<String>("identity");
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let hex_id = resolve_id_or_default(identity, &config, server)?;

    let status = reqwest::Client::new()
        .get(format!("{}/energy/{}", config.get_host_url(server)?, hex_id,))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    println!("{}", status);

    Ok(())
}

fn resolve_id_or_default(identity: Option<&String>, config: &Config, server: Option<&str>) -> anyhow::Result<Identity> {
    match identity {
        Some(identity) => config.resolve_name_to_identity(identity),
        None => Ok(config.get_default_identity_config(server)?.identity),
    }
}
