use crate::common_args;
use crate::util;
use crate::Config;
use clap::{ArgMatches, Command};
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
        .arg(common_args::server().help("The nickname, host name or URL of the server from which to list databases"))
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
    let identity = util::decode_identity(&config)?;

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{}/identity/{}/databases",
            config.get_host_url(server)?,
            identity
        ))
        .basic_auth("token", Some(config.spacetimedb_token_or_error()?))
        .send()
        .await?;

    if res.status() != StatusCode::OK {
        return Err(anyhow::anyhow!(format!(
            "Unable to retrieve databases for identity: {}",
            res.status()
        )));
    }

    let result: DatabasesResult = res.json().await?;

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
