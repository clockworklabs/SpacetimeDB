use crate::Config;
use clap::{Arg, ArgMatches, Command};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json;
use tabled::object::Columns;
use tabled::{Alignment, Modify, Style, Table, Tabled};

pub fn cli() -> clap::Command {
    Command::new("list")
        .about("Lists the databases attached to an identity")
        .arg(Arg::new("identity").required(true))
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
    let identity = match args.get_one::<String>("identity") {
        Some(value) => value.to_string(),
        None => match &config.default_identity {
            Some(default_ident) => default_ident.clone(),
            None => {
                return Err(anyhow::anyhow!("No default identity, and no identity provided!"));
            }
        },
    };

    let client = reqwest::Client::new();
    let mut builder = client.get(format!("http://{}/identity/{}/databases", config.host, identity));

    if let Some(identity_token) = config.get_identity_config_by_identity(&identity) {
        builder = builder.basic_auth("token", Some(identity_token.token.clone()));
    } else {
        return Err(anyhow::anyhow!("Missing identity credentials for identity."));
    }

    let res = builder.send().await?;
    if res.status() != StatusCode::OK {
        return Err(anyhow::anyhow!(format!(
            "Unable to retrieve databases for identity: {}",
            res.status()
        )));
    }

    let res_text = res.text().await?;
    let result: DatabasesResult = serde_json::from_str(res_text.as_str())?;
    let mut result_list: Vec<AddressRow> = Vec::<AddressRow>::new();
    for entry in result.addresses {
        result_list.push(AddressRow { db_address: entry });
    }

    if result_list.len() > 0 {
        let table = Table::new(result_list)
            .with(Style::psql())
            .with(Modify::new(Columns::first()).with(Alignment::left()));
        println!("Associated database addresses for {}:\n", identity);
        println!("{}", table.to_string());
    } else {
        println!("No databases found for {}.", identity);
    }

    Ok(())
}
