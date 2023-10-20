// use clap::Arg;
use clap::{value_parser, Arg, ArgMatches};
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
    vec![
        clap::Command::new("status")
            .about("Show current energy balance for an identity")
            .arg(
                Arg::new("identity")
                    .help("The identity to check the balance for")
                    .long_help(
                    "The identity to check the balance for. If no identity is provided, the default one will be used.",
                ),
            )
            .arg(
                Arg::new("server")
                    .long("server")
                    .short('s')
                    .help("The nickname, host name or URL of the server from which to request balance information"),
            ),
        clap::Command::new("set-balance")
            .about("Update the current budget balance for a database")
            .arg(
                Arg::new("balance")
                    .required(true)
                    .value_parser(value_parser!(i128))
                    .help("The balance value to set"),
            )
            .arg(
                Arg::new("identity")
                    .help("The identity to set a balance for")
                    .long_help(
                        "The identity to set a balance for. If no identity is provided, the default one will be used.",
                    ),
            )
            .arg(
                Arg::new("server")
                    .long("server")
                    .short('s')
                    .help("The nickname, host name or URL of the server on which to update the identity's balance"),
            )
            .arg(
                Arg::new("quiet")
                    .long("quiet")
                    .short('q')
                    .action(clap::ArgAction::SetTrue)
                    .help("Runs command in silent mode"),
            ),
    ]
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "status" => exec_status(config, args).await,
        "set-balance" => exec_update_balance(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    exec_subcommand(config, cmd, subcommand_args).await
}

async fn exec_update_balance(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    // let project_name = args.value_of("project name").unwrap();
    let identity = args.get_one::<String>("identity");
    let balance = *args.get_one::<i128>("balance").unwrap();
    let quiet = args.get_flag("quiet");
    let server = args.get_one::<String>("server").map(|s| s.as_ref());

    let hex_id = resolve_id_or_default(identity, &config, server)?;
    let res = set_balance(&reqwest::Client::new(), &config, &hex_id, balance, server).await?;

    if !quiet {
        println!("{}", res.text().await?);
    }

    Ok(())
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

pub(super) async fn set_balance(
    client: &reqwest::Client,
    config: &Config,
    identity: &Identity,
    balance: i128,
    server: Option<&str>,
) -> anyhow::Result<reqwest::Response> {
    // TODO: this really should be form data in POST body, not query string parameter, but gotham
    // does not support that on the server side without an extension.
    // see https://github.com/gotham-rs/gotham/issues/11
    client
        .post(format!("{}/energy/{}", config.get_host_url(server)?, identity,))
        .query(&[("balance", balance)])
        .send()
        .await?
        .error_for_status()
        .map_err(|e| e.into())
}
