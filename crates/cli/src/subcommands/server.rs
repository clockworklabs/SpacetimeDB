use crate::{
    common_args,
    util::{host_or_url_to_host_and_protocol, spacetime_server_fingerprint, y_or_n, UNSTABLE_WARNING, VALID_PROTOCOLS},
    Config,
};
use anyhow::Context;
use clap::{Arg, ArgAction, ArgMatches, Command};
use spacetimedb_paths::{server::ServerDataDir, SpacetimePaths};
use tabled::{
    settings::{object::Columns, Alignment, Modify, Style},
    Table, Tabled,
};

pub fn cli() -> Command {
    Command::new("server")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_subcommands())
        .about(format!(
            "Manage the connection to the SpacetimeDB server. {UNSTABLE_WARNING}"
        ))
}

fn get_subcommands() -> Vec<Command> {
    vec![
        Command::new("list").about("List stored server configurations"),
        Command::new("set-default")
            .about("Set the default server for future operations")
            .arg(
                Arg::new("server")
                    .help("The nickname, host name or URL of the new default server")
                    .required(true),
            ),
        Command::new("add")
            .about("Add a new server configuration")
            .arg(
                Arg::new("url")
                    .long("url")
                    .help("The URL of the server to add")
                    .required(true),
            )
            .arg(Arg::new("name").help("Nickname for this server").required(true))
            .arg(
                Arg::new("default")
                    .help("Make the new server the default server for future operations")
                    .long("default")
                    .short('d')
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("no-fingerprint")
                    .help("Skip fingerprinting the server")
                    .long("no-fingerprint")
                    .action(ArgAction::SetTrue),
            ),
        Command::new("remove")
            .about("Remove a saved server configuration")
            .arg(
                Arg::new("server")
                    .help("The nickname, host name or URL of the server to remove")
                    .required(true),
            )
            .arg(common_args::yes()),
        Command::new("fingerprint")
            .about("Show or update a saved server's fingerprint")
            .arg(
                Arg::new("server")
                    .required(true)
                    .help("The nickname, host name or URL of the server"),
            )
            .arg(common_args::yes()),
        Command::new("ping")
            .about("Checks to see if a SpacetimeDB host is online")
            .arg(
                Arg::new("server")
                    .required(true)
                    .help("The nickname, host name or URL of the server to ping"),
            ),
        Command::new("edit")
            .about("Update a saved server's nickname, host name or protocol")
            .arg(
                Arg::new("server")
                    .required(true)
                    .help("The nickname, host name or URL of the server"),
            )
            .arg(
                Arg::new("nickname")
                    .help("A new nickname to assign the server configuration")
                    .long("new-name"),
            )
            .arg(
                Arg::new("url")
                    .long("url")
                    .help("A new URL to assign the server configuration"),
            )
            .arg(
                Arg::new("no-fingerprint")
                    .help("Skip fingerprinting the server")
                    .long("no-fingerprint")
                    .action(ArgAction::SetTrue),
            )
            .arg(common_args::yes()),
        Command::new("clear")
            .about("Deletes all data from all local databases")
            .arg(
                Arg::new("data_dir")
                    .long("data-dir")
                    .help("The path to the server data directory to clear [default: that of the selected spacetime instance]")
                    .value_parser(clap::value_parser!(ServerDataDir)),
            )
            .arg(common_args::yes()),
        // TODO: set-name, set-protocol, set-host, set-url
    ]
}

pub async fn exec(config: Config, paths: &SpacetimePaths, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    eprintln!("{UNSTABLE_WARNING}\n");
    exec_subcommand(config, paths, cmd, subcommand_args).await
}

async fn exec_subcommand(
    config: Config,
    paths: &SpacetimePaths,
    cmd: &str,
    args: &ArgMatches,
) -> Result<(), anyhow::Error> {
    match cmd {
        "list" => exec_list(config, args).await,
        "set-default" => exec_set_default(config, args).await,
        "add" => exec_add(config, args).await,
        "remove" => exec_remove(config, args).await,
        "fingerprint" => exec_fingerprint(config, args).await,
        "ping" => exec_ping(config, args).await,
        "edit" => exec_edit(config, args).await,
        "clear" => exec_clear(config, paths, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {unknown}")),
    }
}

#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
struct LsRow {
    default: String,
    hostname: String,
    protocol: String,
    nickname: String,
}

pub async fn exec_list(config: Config, _args: &ArgMatches) -> Result<(), anyhow::Error> {
    let mut rows: Vec<LsRow> = Vec::new();
    for server_config in config.server_configs() {
        let default = if let Some(default_name) = config.default_server_name() {
            server_config.nick_or_host_or_url_is(default_name)
        } else {
            false
        };
        rows.push(LsRow {
            default: if default { "***" } else { "" }.to_string(),
            hostname: server_config.host.to_string(),
            protocol: server_config.protocol.to_string(),
            nickname: server_config.nickname.as_deref().unwrap_or("").to_string(),
        });
    }

    let mut table = Table::new(&rows);
    table
        .with(Style::empty())
        .with(Modify::new(Columns::first()).with(Alignment::right()));
    println!("{table}");

    Ok(())
}

pub async fn exec_set_default(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").unwrap();
    config.set_default_server(server)?;
    config.save();
    Ok(())
}

fn valid_protocol_or_error(protocol: &str) -> anyhow::Result<()> {
    if !VALID_PROTOCOLS.contains(&protocol) {
        Err(anyhow::anyhow!("Invalid protocol: {protocol}"))
    } else {
        Ok(())
    }
}

pub async fn exec_add(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    // Trim trailing `/`s because otherwise we end up with a double `//` in some later codepaths.
    // See https://github.com/clockworklabs/SpacetimeDB/issues/1551.
    let url = args.get_one::<String>("url").unwrap().trim_end_matches('/');
    let nickname = args.get_one::<String>("name");
    let default = *args.get_one::<bool>("default").unwrap();
    let no_fingerprint = *args.get_one::<bool>("no-fingerprint").unwrap();

    let (host, protocol) = host_or_url_to_host_and_protocol(url);
    let protocol = protocol.ok_or_else(|| anyhow::anyhow!("Invalid url: {url}"))?;

    valid_protocol_or_error(protocol)?;

    let fingerprint = if no_fingerprint {
        None
    } else {
        let fingerprint = spacetime_server_fingerprint(url).await.with_context(|| {
            format!(
                "Unable to retrieve fingerprint for server: {url}
Is the server running?
Add a server without retrieving its fingerprint with:
\tspacetime server add --url {url} --no-fingerprint",
            )
        })?;
        println!("For server {url}, got fingerprint:\n{fingerprint}");
        Some(fingerprint)
    };

    config.add_server(host.to_string(), protocol.to_string(), fingerprint, nickname.cloned())?;

    if default {
        config.set_default_server(host)?;
    }

    println!("Host: {host}");
    println!("Protocol: {protocol}");

    config.save();

    Ok(())
}

pub async fn exec_remove(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").unwrap();

    config.remove_server(server)?;

    config.save();

    Ok(())
}

async fn update_server_fingerprint(config: &mut Config, server: Option<&str>) -> Result<bool, anyhow::Error> {
    let url = config.get_host_url(server)?;
    let nick_or_host = config.server_nick_or_host(server)?;
    let new_fing = spacetime_server_fingerprint(&url)
        .await
        .context("Error fetching server fingerprint")?;
    if let Some(saved_fing) = config.server_fingerprint(server)? {
        if saved_fing == new_fing {
            println!("Fingerprint is unchanged for server {nick_or_host}:\n{saved_fing}");

            Ok(false)
        } else {
            println!("Fingerprint has changed for server {nick_or_host}.\nWas:\n{saved_fing}\nNew:\n{new_fing}");

            config.set_server_fingerprint(server, new_fing)?;

            Ok(true)
        }
    } else {
        println!("No saved fingerprint for server {nick_or_host}. New fingerprint:\n{new_fing}");

        config.set_server_fingerprint(server, new_fing)?;

        Ok(true)
    }
}

pub async fn exec_fingerprint(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").unwrap().as_str();
    let force = args.get_flag("force");

    if update_server_fingerprint(&mut config, Some(server)).await? {
        if !y_or_n(force, "Continue?")? {
            anyhow::bail!("Aborted");
        }

        config.save();
    }

    Ok(())
}

pub async fn exec_ping(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").unwrap().as_str();
    let url = config.get_host_url(Some(server))?;

    let builder = reqwest::Client::new().get(format!("{url}/v1/ping").as_str());
    let response = builder.send().await?;

    match response.status() {
        reqwest::StatusCode::OK => {
            println!("Server is online: {url}");
        }
        reqwest::StatusCode::NOT_FOUND => {
            println!("Server returned 404 (Not Found): {url}");
        }
        err => {
            println!("Server could not be reached ({err}): {url}");
        }
    }
    Ok(())
}

pub async fn exec_edit(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").unwrap().as_str();

    let old_url = config.get_host_url(Some(server))?;

    let new_nick = args.get_one::<String>("nickname").map(|s| s.as_str());
    let new_url = args.get_one::<String>("url").map(|s| s.as_str());
    let (new_host, new_proto) = match new_url {
        None => (None, None),
        Some(new_url) => {
            let (new_host, new_proto) = host_or_url_to_host_and_protocol(new_url);
            let new_proto = new_proto.ok_or_else(|| anyhow::anyhow!("Invalid url: {new_url}"))?;
            (Some(new_host), Some(new_proto))
        }
    };

    let no_fingerprint = args.get_flag("no-fingerprint");
    let force = args.get_flag("force");

    if let Some(new_proto) = new_proto {
        valid_protocol_or_error(new_proto)?;
    }

    let (old_nick, old_host, old_proto) = config.edit_server(server, new_nick, new_host, new_proto)?;
    let server = new_nick.unwrap_or(server);

    if let (Some(new_nick), Some(old_nick)) = (new_nick, old_nick) {
        println!("Changing nickname from {old_nick} to {new_nick}");
    }
    if let (Some(new_host), Some(old_host)) = (new_host, old_host) {
        println!("Changing host from {old_host} to {new_host}");
    }
    if let (Some(new_proto), Some(old_proto)) = (new_proto, old_proto) {
        println!("Changing protocol from {old_proto} to {new_proto}");
    }

    let new_url = config.get_host_url(Some(server))?;

    if old_url != new_url {
        if no_fingerprint {
            config.delete_server_fingerprint(Some(&new_url))?;
        } else {
            update_server_fingerprint(&mut config, Some(&new_url)).await?;
        }
    }

    if !y_or_n(force, "Continue?")? {
        anyhow::bail!("Aborted");
    }

    config.save();

    Ok(())
}

async fn exec_clear(_config: Config, paths: &SpacetimePaths, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let force = args.get_flag("force");
    let data_dir = args.get_one::<ServerDataDir>("data_dir").unwrap_or(&paths.data_dir);

    if data_dir.0.exists() {
        println!("Database path: {}", data_dir.display());

        if !y_or_n(
            force,
            "Are you sure you want to delete all data from the local database?",
        )? {
            println!("Aborting");
            return Ok(());
        }

        std::fs::remove_dir_all(data_dir)?;
        println!("Deleted database: {}", data_dir.display());
    } else {
        println!("Local database not found. Nothing has been deleted.");
    }
    Ok(())
}
