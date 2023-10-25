use crate::Config;
use anyhow::Context;
use clap::{Arg, ArgMatches, Command};
use reqwest::StatusCode;
use serde::Deserialize;
use spacetimedb_lib::Address;
use tabled::{
    settings::{object::Columns, Alignment, Modify, Style},
    Table, Tabled,
};

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
    pub addresses: Vec<AddressRow>,
}

#[derive(Tabled, Deserialize)]
#[serde(transparent)]
struct AddressRow {
    pub db_address: Address,
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let identity_config = match args.get_one::<String>("identity") {
        Some(identity_or_name) => config
            .get_identity_config(identity_or_name)
            .ok_or_else(|| anyhow::anyhow!("Missing identity credentials for identity: {identity_or_name}"))?,
        None => config
            .get_default_identity_config(server)
            .context("No default identity, and no identity provided!")?,
    };

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{}/identity/{}/databases",
            config.get_host_url(server)?,
            identity_config.identity
        ))
        .basic_auth("token", Some(&identity_config.token))
        .send()
        .await?;

    if res.status() != StatusCode::OK {
        return Err(anyhow::anyhow!(format!(
            "Unable to retrieve databases for identity: {}",
            res.status()
        )));
    }

    let result: DatabasesResult = res.json().await?;

    let identity = identity_config.nick_or_identity();
    if !result.addresses.is_empty() {
        let mut table = Table::new(result.addresses);
        table
            .with(Style::psql())
            .with(Modify::new(Columns::first()).with(Alignment::left()));
        println!("Associated database addresses for {}:\n", identity);
        println!("{}", table);
    } else {
        println!("No databases found for {}.", identity);
    }

    Ok(())
}
