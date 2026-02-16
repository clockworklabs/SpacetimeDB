use crate::common_args;
use crate::util;
use crate::util::get_login_token_or_log_in;
use crate::util::ResponseExt;
use crate::util::UNSTABLE_WARNING;
use crate::Config;
use anyhow::Context;
use clap::{ArgMatches, Command};
use serde::Deserialize;
use spacetimedb_lib::Identity;
use tabled::{
    settings::{object::Columns, Alignment, Modify, Style},
    Table, Tabled,
};

pub fn cli() -> Command {
    Command::new("list")
        .about(format!(
            "Lists the databases attached to an identity. {UNSTABLE_WARNING}"
        ))
        .arg(common_args::server().help("The nickname, host name or URL of the server from which to list databases"))
        .arg(common_args::yes())
}

#[derive(Deserialize)]
struct DatabasesResult {
    pub identities: Vec<IdentityOnlyRow>,
}

#[derive(Deserialize)]
#[serde(transparent)]
struct IdentityOnlyRow {
    pub db_identity: Identity,
}

#[derive(Tabled)]
struct IdentityRow {
    pub db_identity: Identity,
    pub default_name: String,
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    eprintln!("{UNSTABLE_WARNING}\n");

    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let force = args.get_flag("force");
    let token = get_login_token_or_log_in(&mut config, server, !force).await?;
    let identity = util::decode_identity(&token)?;

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{}/v1/identity/{}/databases",
            config.get_host_url(server)?,
            identity
        ))
        .bearer_auth(token)
        .send()
        .await?;

    let result: DatabasesResult = res
        .json_or_error()
        .await
        .context("unable to retrieve databases for identity")?;

    if !result.identities.is_empty() {
        let mut rows = Vec::with_capacity(result.identities.len());
        for row in result.identities {
            let default_name = util::spacetime_reverse_dns(&config, &row.db_identity.to_string(), server)
                .await
                .with_context(|| format!("unable to retrieve database names for {}", row.db_identity))?
                .names
                .first()
                .map(ToString::to_string)
                .unwrap_or_else(|| "<unnamed>".to_owned());
            rows.push(IdentityRow {
                db_identity: row.db_identity,
                default_name,
            });
        }

        let mut table = Table::new(rows);
        table
            .with(Style::psql())
            .with(Modify::new(Columns::first()).with(Alignment::left()));
        println!("Associated databases for {identity}:\n");
        println!("{table}");
    } else {
        println!("No databases found for {identity}.");
    }

    Ok(())
}
