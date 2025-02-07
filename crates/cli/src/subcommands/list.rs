use crate::common_args;
use crate::util;
use crate::util::get_login_token_or_log_in;
use crate::util::UNSTABLE_HELPTEXT;
use crate::Config;
use clap::{ArgMatches, Command};
use reqwest::StatusCode;
use serde::Deserialize;
use spacetimedb::Identity;
use tabled::{
    settings::{object::Columns, Alignment, Modify, Style},
    Table, Tabled,
};

pub fn cli() -> Command {
    Command::new("list")
        .about(format!(
            "Lists the databases attached to an identity.\n\n{}",
            UNSTABLE_HELPTEXT
        ))
        .arg(common_args::server().help("The nickname, host name or URL of the server from which to list databases"))
        .arg(common_args::yes())
}

#[derive(Deserialize)]
struct DatabasesResult {
    pub identities: Vec<IdentityRow>,
}

#[derive(Tabled, Deserialize)]
#[serde(transparent)]
struct IdentityRow {
    pub db_identity: Identity,
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    println!("{}", UNSTABLE_HELPTEXT);

    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let force = args.get_flag("force");
    let token = get_login_token_or_log_in(&mut config, server, !force).await?;
    let identity = util::decode_identity(&token)?;

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{}/identity/{}/databases",
            config.get_host_url(server)?,
            identity
        ))
        .basic_auth("token", Some(token))
        .send()
        .await?;

    if res.status() != StatusCode::OK {
        return Err(anyhow::anyhow!(format!(
            "Unable to retrieve databases for identity: {}",
            res.status()
        )));
    }

    let result: DatabasesResult = res.json().await?;

    if !result.identities.is_empty() {
        let mut table = Table::new(result.identities);
        table
            .with(Style::psql())
            .with(Modify::new(Columns::first()).with(Alignment::left()));
        println!("Associated database identities for {}:\n", identity);
        println!("{}", table);
    } else {
        println!("No databases found for {}.", identity);
    }

    Ok(())
}
