// use clap::Arg;
use clap::{value_parser, Arg, ArgMatches};

use crate::config::Config;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("energy")
        .about("Invokes commands related to energy.")
        .subcommands(get_energy_subcommands())
        .override_usage("stdb energy [status|set-balance|set-default-maximum] <identity> OPTIONS")
        .after_help("Run `stdb help energy for more detailed information.\n`")
}

fn get_energy_subcommands() -> Vec<clap::Command<'static>> {
    vec![
        clap::Command::new("status")
            .about("Show current budget status and information")
            .arg(Arg::new("identity").required(true)),
        clap::Command::new("set-balance")
            .about("Update current budget balance")
            .arg(Arg::new("identity").required(true))
            .arg(Arg::new("balance").required(true).value_parser(value_parser!(usize))),
        clap::Command::new("set-default-maximum")
            .about("Update the default maximum spend per reducer invocation")
            .arg(Arg::new("identity").required(true))
            .arg(
                Arg::new("default_maximum")
                    .required(true)
                    .value_parser(value_parser!(usize)),
            ),
    ]
}

async fn exec_update_balance(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    // let project_name = args.value_of("project name").unwrap();
    let hex_identity: &str = args.value_of("identity").unwrap().clone();

    let balance: usize = *args.get_one("balance").unwrap();

    let client = reqwest::Client::new();

    // TODO: this really should be form data in POST body, not query string parameter, but gotham
    // does not support that on the server side without an extension.
    // see https://github.com/gotham-rs/gotham/issues/11
    let url = format!("http://{}/budget/{}?balance={}", config.host, hex_identity, balance);
    let res = client.post(url).send().await?;

    let res = res.error_for_status()?;
    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);
    Ok(())
}

async fn exec_update_default_maximum(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    // let project_name = args.value_of("project name").unwrap();
    let hex_identity: &str = args.value_of("identity").unwrap().clone();

    let default_maximum: usize = *args.get_one("default_maximum").unwrap();

    let client = reqwest::Client::new();

    // TODO: this really should be form data in POST body, not query string parameter, but gotham
    // does not support that on the server side without an extension.
    // see https://github.com/gotham-rs/gotham/issues/11
    let url = format!(
        "http://{}/budget/{}?default_maximum={}",
        config.host, hex_identity, default_maximum
    );
    let res = client.post(url).send().await?;

    let res = res.error_for_status()?;
    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);
    Ok(())
}

async fn exec_status(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    // let project_name = args.value_of("project name").unwrap();
    let hex_identity = args.value_of("identity").unwrap();

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{}/budget/{}", config.host, hex_identity))
        .send()
        .await?;

    let res = res.error_for_status()?;
    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);
    Ok(())
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "status" => exec_status(config, args).await,
        "set-balance" => exec_update_balance(config, args).await,
        "set-default-maximum" => exec_update_default_maximum(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    exec_subcommand(config, cmd, subcommand_args).await
}
