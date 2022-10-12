// use clap::Arg;
use clap::{value_parser, Arg, ArgMatches};

use crate::config::Config;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("energy")
        .about("Invokes commands related to database budgets.")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_energy_subcommands())
}

fn get_energy_subcommands() -> Vec<clap::Command<'static>> {
    vec![
        clap::Command::new("status")
            .about("Show current energy balance for an identity")
            .arg(Arg::new("identity").required(false)),
        clap::Command::new("set-balance")
            .about("Update the current budget balance for a database")
            .arg(Arg::new("balance").required(true).value_parser(value_parser!(usize)))
            .arg(Arg::new("identity").required(false)),
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
    let hex_identity = args.value_of("identity");
    let hex_identity = if let Some(hex_identity) = hex_identity {
        hex_identity
    } else {
        config.get_default_identity_config().unwrap().identity.as_str()
    };

    let balance: usize = *args.get_one("balance").unwrap();

    let client = reqwest::Client::new();

    // TODO: this really should be form data in POST body, not query string parameter, but gotham
    // does not support that on the server side without an extension.
    // see https://github.com/gotham-rs/gotham/issues/11
    let url = format!("http://{}/energy/{}?balance={}", config.host, hex_identity, balance);
    let res = client.post(url).send().await?;

    let res = res.error_for_status()?;
    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);
    Ok(())
}

async fn exec_status(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    // let project_name = args.value_of("project name").unwrap();
    let hex_identity = args.value_of("identity");
    let hex_identity = if let Some(hex_identity) = hex_identity {
        hex_identity
    } else {
        config.get_default_identity_config().unwrap().identity.as_str()
    };

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{}/energy/{}", config.host, hex_identity))
        .send()
        .await?;

    let res = res.error_for_status()?;
    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);
    Ok(())
}
