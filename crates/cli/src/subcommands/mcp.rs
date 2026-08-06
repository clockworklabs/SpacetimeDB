use crate::api::{build_client, Connection};
use crate::common_args;
use crate::config::Config;
use crate::util::{auth_header_from_saved_token, database_identity, ResponseExt, UNSTABLE_WARNING};
use anyhow::Context;
use clap::{Arg, ArgMatches};
use spacetimedb_lib::Identity;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub fn cli() -> clap::Command {
    clap::Command::new("mcp")
        .about(format!(
            "Serve SpacetimeDB to MCP-aware agents and editors over stdio. {UNSTABLE_WARNING}"
        ))
        .arg(Arg::new("database").required(false).env("SPACETIMEDB_DB_NAME").help(
            "The name or identity of a single database to serve. Falls back to the SPACETIMEDB_DB_NAME environment variable. Omit it to serve the whole server, where each tool takes a database argument instead",
        ))
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
        .arg(common_args::anonymous())
        .after_help("Run `spacetime help mcp` for more detailed information.\n")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    eprintln!("{UNSTABLE_WARNING}\n");

    let database = args.get_one::<String>("database");
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let anon_identity = args.get_flag("anon_identity");

    let conn = Connection {
        host: config.get_host_url(server)?,
        auth_header: auth_header_from_saved_token(&config, anon_identity),
        database_identity: match database {
            Some(database) => database_identity(&config, database, server).await?,
            None => Identity::ZERO,
        },
        database: database.cloned().unwrap_or_default(),
    };

    let client = build_client(&conn);
    let url = match database {
        Some(_) => conn.db_uri("mcp"),
        None => conn.host_uri("mcp"),
    };
    eprintln!("Serving MCP over stdio, bridging to {url}");

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let response = client
            .post(url.as_str())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(line)
            .send()
            .await
            .with_context(|| format!("could not reach the SpacetimeDB host at {}", conn.host))?;
        // a notification answered with empty 202 body
        if response.status() == reqwest::StatusCode::ACCEPTED {
            continue;
        }
        let body = response.ensure_content_type("application/json").await?.text().await?;
        stdout.write_all(body.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}
