use crate::Config;
use anyhow::Context;
use clap::{Arg, ArgMatches, Command};
use reqwest::StatusCode;
use serde::Deserialize;
use tabled::object::Columns;
use tabled::{Alignment, Modify, Style, Table, Tabled};

pub fn cli() -> Command {
    Command::new("list")
        .about("Lists the databases attached to an identity")
        .arg(
            Arg::new("identity")
                .required(true)
                .help("The identity to list databases for"),
        )
        .arg(
            Arg::new("server")
                .long("server")
                .short('s')
                .help("The nickname, host name or URL of the server from which to list databases"),
        )
}

#[derive(Deserialize)]
struct DatabasesResult {
    pub addresses: Vec<String>,
}

#[derive(Tabled)]
struct AddressRow {
    pub db_address: String,
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let identity = match args.get_one::<String>("identity") {
        Some(value) => value.to_string(),
        None => config
            .default_identity(server)
            .map(str::to_string)
            .with_context(|| "No default identity, and no identity provided!")?,
    };

    let client = reqwest::Client::new();
    let mut builder = client.get(format!(
        "{}/identity/{}/databases",
        config.get_host_url(server)?,
        identity
    ));

    let identity_token = config.get_identity_config_by_identity(&identity)?;
    builder = builder.basic_auth("token", Some(identity_token.token.clone()));

    let res = builder.send().await?;
    if res.status() != StatusCode::OK {
        return Err(anyhow::anyhow!(format!(
            "Unable to retrieve databases for identity: {}",
            res.status()
        )));
    }

    let result: DatabasesResult = res.json().await?;
    let result_list = result
        .addresses
        .into_iter()
        .map(|db_address| AddressRow { db_address })
        .collect::<Vec<_>>();

    if !result_list.is_empty() {
        let table = Table::new(result_list)
            .with(Style::psql())
            .with(Modify::new(Columns::first()).with(Alignment::left()));
        println!("Associated database addresses for {}:\n", identity);
        println!("{}", table);
    } else {
        println!("No databases found for {}.", identity);
    }

    Ok(())
}
