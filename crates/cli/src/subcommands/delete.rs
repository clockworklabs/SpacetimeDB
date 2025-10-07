use std::collections::{BTreeMap, BTreeSet};
use std::io;

use crate::common_args;
use crate::config::Config;
use crate::util::{add_auth_header_opt, database_identity, get_auth_header, y_or_n, AuthHeader};
use clap::{Arg, ArgMatches};
use http::StatusCode;
use reqwest::Response;
use spacetimedb_lib::{Hash, Identity};

pub fn cli() -> clap::Command {
    clap::Command::new("delete")
        .about("Deletes a SpacetimeDB database")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The name or identity of the database to delete"),
        )
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
        .arg(common_args::yes())
        .after_help("Run `spacetime help delete` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let database = args.get_one::<String>("database").unwrap();
    let force = args.get_flag("force");

    let identity = database_identity(&config, database, server).await?;
    let host_url = config.get_host_url(server)?;
    let request_path = format!("{host_url}/v1/database/{identity}");
    let auth_header = get_auth_header(&mut config, false, server, !force).await?;
    let client = reqwest::Client::new();

    let response = send_request(&client, &request_path, &auth_header, None).await?;
    match response.status() {
        StatusCode::PRECONDITION_REQUIRED => {
            let confirm = response.json::<ConfirmationResponse>().await?;
            println!("WARNING: Deleting the database {identity} will also delete its children:");
            confirm.print_database_tree_info(io::stdout())?;
            if y_or_n(force, "Do you want to proceed deleting above databases?")? {
                send_request(&client, &request_path, &auth_header, Some(confirm.token))
                    .await?
                    .error_for_status()?;
            } else {
                println!("Aborting");
            }

            Ok(())
        }
        StatusCode::OK => Ok(()),
        _ => response.error_for_status().map(drop).map_err(Into::into),
    }
}

async fn send_request(
    client: &reqwest::Client,
    request_path: &str,
    auth: &AuthHeader,
    confirmation_token: Option<Hash>,
) -> Result<Response, reqwest::Error> {
    let mut builder = client.delete(request_path);
    builder = add_auth_header_opt(builder, auth);
    if let Some(token) = confirmation_token {
        builder = builder.query(&[("token", token.to_string())]);
    }
    builder.send().await
}

#[derive(serde::Deserialize)]
struct ConfirmationResponse {
    database_tree: DatabaseTreeInfo,
    token: Hash,
}

impl ConfirmationResponse {
    pub fn print_database_tree_info(&self, mut out: impl io::Write) -> anyhow::Result<()> {
        let fmt_names = |names: &BTreeSet<String>| match names.len() {
            0 => <_>::default(),
            1 => format!(": {}", names.first().unwrap()),
            _ => format!(": {names:?}"),
        };

        let tree_info = &self.database_tree;

        write!(out, "{}{}", tree_info.root.identity, fmt_names(&tree_info.root.names))?;
        for (identity, info) in &tree_info.children {
            let names = fmt_names(&info.names);
            let parent = info
                .parent
                .map(|parent| format!(" (parent: {parent})"))
                .unwrap_or_default();

            write!(out, "{identity}{parent}{names}")?;
        }

        Ok(())
    }
}

// TODO: Should below types be in client-api?

#[derive(serde::Deserialize)]
pub struct DatabaseTreeInfo {
    root: RootDatabase,
    children: BTreeMap<Identity, DatabaseInfo>,
}

#[derive(serde::Deserialize)]
pub struct RootDatabase {
    identity: Identity,
    names: BTreeSet<String>,
}

#[derive(serde::Deserialize)]
pub struct DatabaseInfo {
    names: BTreeSet<String>,
    parent: Option<Identity>,
}
