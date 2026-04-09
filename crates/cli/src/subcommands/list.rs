use crate::common_args;
use crate::util;
use crate::util::get_login_token_or_log_in;
use crate::util::ResponseExt;
use crate::util::UNSTABLE_WARNING;
use crate::Config;
use anyhow::Context;
use clap::{ArgMatches, Command};
use futures::future::join_all;
use serde::Deserialize;
use spacetimedb_client_api_messages::name::DatabaseName;
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
    pub identities: Vec<IdentityRow>,
}

#[derive(Tabled, Deserialize)]
#[serde(transparent)]
struct IdentityRow {
    pub db_identity: Identity,
}

#[derive(Tabled)]
struct DatabaseRow {
    pub db_names: String,
    pub db_identity: Identity,
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
        let databases = lookup_database_names(&config, server, result.identities).await;
        let mut table = Table::new(databases);
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

async fn lookup_database_names(
    config: &Config,
    server: Option<&str>,
    identities: Vec<IdentityRow>,
) -> Vec<DatabaseRow> {
    let lookups = identities.iter().map(|row| async {
        let result = util::spacetime_reverse_dns(config, &row.db_identity.to_string(), server).await;
        (row.db_identity, result)
    });

    join_all(lookups)
        .await
        .into_iter()
        .map(|(db_identity, result)| {
            let db_names = match result {
                Ok(response) if !response.names.is_empty() => format_database_names(response.names),
                Ok(_) => "(unnamed)".to_string(),
                Err(err) => {
                    eprintln!("Warning: failed to look up names for {db_identity}: {err}");
                    "(lookup failed)".to_string()
                }
            };

            DatabaseRow { db_names, db_identity }
        })
        .collect()
}

fn format_database_names(names: Vec<DatabaseName>) -> String {
    names
        .into_iter()
        .map(|name| name.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
