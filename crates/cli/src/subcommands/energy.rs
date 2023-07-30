// use clap::Arg;
use clap::{value_parser, Arg, ArgMatches};

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
    let hex_id = args.get_one::<String>("identity");
    let balance = *args.get_one::<i128>("balance").unwrap();
    let quiet = args.get_flag("quiet");

    let hex_id = hex_id_or_default(hex_id, &config);
    let res = set_balance(&reqwest::Client::new(), &config, hex_id, balance).await?;

    if !quiet {
        println!("{}", res.text().await?);
    }

    Ok(())
}

async fn exec_status(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    // let project_name = args.value_of("project name").unwrap();
    let hex_id = args.get_one::<String>("identity");
    let hex_id = hex_id_or_default(hex_id, &config);

    let status = reqwest::Client::new()
        .get(format!("{}/energy/{}", config.get_host_url(), hex_id))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    println!("{}", status);

    Ok(())
}

fn hex_id_or_default<'a>(hex_id: Option<&'a String>, config: &'a Config) -> &'a String {
    hex_id.unwrap_or_else(|| &config.get_default_identity_config().unwrap().identity)
}

pub(super) async fn set_balance(
    client: &reqwest::Client,
    config: &Config,
    hex_identity: &str,
    balance: i128,
) -> anyhow::Result<reqwest::Response> {
    // TODO: this really should be form data in POST body, not query string parameter, but gotham
    // does not support that on the server side without an extension.
    // see https://github.com/gotham-rs/gotham/issues/11
    client
        .post(format!("{}/energy/{}", config.get_host_url(), hex_identity))
        .query(&[("balance", balance)])
        .send()
        .await?
        .error_for_status()
        .map_err(|e| e.into())
}
