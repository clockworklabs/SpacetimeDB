//! `spacetime sidecar` (PROTOTYPE)
//!
//! Manage sidecars: long-running programs (e.g. agents) hosted beside a
//! database. This command is a thin client: it records the desired state by
//! calling the control database's `create_sidecar` / `delete_sidecar` reducers.
//! The actual container is launched and reconciled by the SpacetimeDB Cloud
//! node hosting the database (see `private/crates/cloud`); SpacetimeDB
//! Standalone has no control plane and does not support sidecars.
//!
//! See `proposals/00XX-agent-hosting.md`.

use crate::api::{ClientApi, Connection};
use crate::common_args;
use crate::config::Config;
use crate::util::{database_identity, get_auth_header, AuthHeader, UNSTABLE_WARNING};
use anyhow::{bail, Context};
use clap::{Arg, ArgAction, ArgMatches};
use spacetimedb_lib::Identity;

pub fn cli() -> clap::Command {
    clap::Command::new("sidecar")
        .about(format!(
            "Manage sidecars (long-running programs hosted beside a database). {UNSTABLE_WARNING}"
        ))
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands([
            clap::Command::new("run")
                .about("Declare a sidecar for a database; the hosting node launches it")
                .arg(Arg::new("database").required(true).help("The database name or identity"))
                .arg(Arg::new("image").long("image").required(true).help("The OCI image to run"))
                .arg(
                    Arg::new("name")
                        .long("name")
                        .default_value("default")
                        .help("A name for this sidecar, allowing several per database"),
                )
                .arg(
                    Arg::new("env")
                        .long("env")
                        .short('e')
                        .value_name("KEY=VALUE")
                        .action(ArgAction::Append)
                        .help("Extra environment variables to inject (repeatable)"),
                )
                .arg(
                    Arg::new("command")
                        .num_args(0..)
                        .last(true)
                        .help("Optional command to run in the container, after `--`"),
                )
                .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
                .arg(common_args::anonymous())
                .arg(common_args::yes()),
            clap::Command::new("ls")
                .about("List declared sidecars")
                .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database")),
            clap::Command::new("stop")
                .about("Remove a sidecar from a database; the hosting node stops it")
                .arg(Arg::new("database").required(true).help("The database name or identity"))
                .arg(
                    Arg::new("name")
                        .long("name")
                        .default_value("default")
                        .help("The sidecar name to remove"),
                )
                .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
                .arg(common_args::anonymous())
                .arg(common_args::yes()),
        ])
        .after_help("Run `spacetime help sidecar` for more detailed information.\n")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    eprintln!("{UNSTABLE_WARNING}\n");
    let (cmd, sub) = args.subcommand().expect("subcommand required");
    match cmd {
        "run" => exec_run(config, sub).await,
        "ls" => exec_ls(config, sub).await,
        "stop" => exec_stop(config, sub).await,
        unknown => bail!("Invalid subcommand: {unknown}"),
    }
}

/// The control database is addressed by the all-zero identity.
fn control_api(config: &Config, server: Option<&str>, auth: AuthHeader) -> anyhow::Result<ClientApi> {
    Ok(ClientApi::new(Connection {
        host: config.get_host_url(server)?,
        database_identity: Identity::ZERO,
        database: "control".to_string(),
        auth_header: auth,
    }))
}

async fn exec_run(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_str());
    let force = args.get_flag("force");
    let anon = args.get_flag("anon_identity");
    let database = args.get_one::<String>("database").unwrap();
    let image = args.get_one::<String>("image").unwrap();
    let name = args.get_one::<String>("name").unwrap();
    let mut env: Vec<String> = args
        .get_many::<String>("env")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();
    let command: Vec<String> = args
        .get_many::<String>("command")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();

    let host = config.get_host_url(server)?;
    let db_identity = database_identity(&config, database, server).await?;
    let auth = get_auth_header(&mut config, anon, server, !force).await?;
    let token = auth
        .token()
        .context("No auth token available; run `spacetime login` first (the sidecar connects back with it)")?
        .to_string();

    // Connection details for the sidecar to reach this database, made reachable
    // from inside a container (Docker Desktop maps host.docker.internal).
    let db_hex = db_identity.to_hex().to_string();
    let mut connect_env = vec![
        format!("SPACETIMEDB_URI={}", to_container_host(&http_to_ws(&host))),
        format!("SPACETIMEDB_HTTP_URI={}", to_container_host(&host)),
        format!("SPACETIMEDB_TOKEN={token}"),
        format!("SPACETIMEDB_DB={db_hex}"),
    ];
    connect_env.append(&mut env);

    let api = control_api(&config, server, auth)?;
    let arg_json = serde_json::json!([db_hex, name, image, command, connect_env]).to_string();
    let res = api.call("create_sidecar", arg_json).await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("failed to declare sidecar ({status}): {body}");
    }

    println!("Declared sidecar `{name}` for database `{database}`.");
    println!("The node hosting the database will launch it shortly. Check with:");
    println!("  spacetime sidecar ls");
    Ok(())
}

async fn exec_stop(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_str());
    let force = args.get_flag("force");
    let anon = args.get_flag("anon_identity");
    let database = args.get_one::<String>("database").unwrap();
    let name = args.get_one::<String>("name").unwrap();

    let db_identity = database_identity(&config, database, server).await?;
    let auth = get_auth_header(&mut config, anon, server, !force).await?;
    let db_hex = db_identity.to_hex().to_string();

    let api = control_api(&config, server, auth)?;
    let arg_json = serde_json::json!([db_hex, name]).to_string();
    let res = api.call("delete_sidecar", arg_json).await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("failed to remove sidecar ({status}): {body}");
    }

    println!("Removed sidecar `{name}` from database `{database}`. The hosting node will stop it shortly.");
    Ok(())
}

async fn exec_ls(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_str());
    // Reading the control db requires a token; reuse the logged-in identity.
    let auth = get_auth_header(&mut config, false, server, true).await?;
    let api = control_api(&config, server, auth)?;
    let res = api
        .sql()
        .body("SELECT id, database_id, name, image, desired_state FROM sidecar")
        .send()
        .await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("failed to list sidecars ({status}): {body}");
    }
    println!("{body}");
    Ok(())
}

fn http_to_ws(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        url.to_string()
    }
}

fn to_container_host(url: &str) -> String {
    url.replace("localhost", "host.docker.internal")
        .replace("127.0.0.1", "host.docker.internal")
}
