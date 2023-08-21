use crate::{
    util::{host_or_url_to_host_and_protocol, spacetime_server_fingerprint, VALID_PROTOCOLS},
    Config,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use tabled::{object::Columns, Alignment, Modify, Style, Table, Tabled};

pub fn cli() -> Command {
    Command::new("server")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_subcommands())
        .about("Manage the connection to the SpacetimeDB server")
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
            )
            .arg(
                Arg::new("project")
                    .help("Set the default server for the project-local configuration")
                    .long("project")
                    .short('p')
                    .action(ArgAction::SetTrue),
            ),
        Command::new("add")
            .about("Add a new server configuration")
            .arg(Arg::new("url").help("The URL of the server to add").required(true))
            .arg(Arg::new("name").help("Nickname for this server"))
            .arg(
                Arg::new("default")
                    .help("Make the new server the default server for future operations")
                    .long("default")
                    .short('d')
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("project")
                    .help("Add the server to the project-level configuration")
                    .long("project")
                    .short('p')
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
            .arg(
                Arg::new("delete-identities")
                    .help("Also delete all identities which apply to the server")
                    .long("delete-identities")
                    .short('i')
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("project")
                    .help("Remove the server from the project-level configuration")
                    .long("project")
                    .short('p')
                    .action(ArgAction::SetTrue),
            ),
        Command::new("fingerprint")
            .about("Show a saved server's fingerprint")
            .arg(Arg::new("server").help("The nickname, host name or URL of the server")),
        Command::new("ping")
            .about("Checks to see if a SpacetimeDB host is online")
            .arg(Arg::new("server").help("The nickname, host name or URL of the server to ping")),
        Command::new("update")
            .about("Update a saved server's fingerprint")
            .arg(Arg::new("server").help("The nickname, host name or URL of the server"))
            .arg(
                Arg::new("project")
                    .help("Add the server to the project-level configuration")
                    .long("project")
                    .short('p')
                    .action(ArgAction::SetTrue),
            ),
        // TODO: set-name, set-protocol, set-host, set-url
    ]
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    exec_subcommand(config, cmd, subcommand_args).await
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "list" => exec_list(config, args).await,
        "set-default" => exec_set_default(config, args).await,
        "add" => exec_add(config, args).await,
        "remove" => exec_remove(config, args).await,
        "fingerprint" => exec_fingerprint(config, args).await,
        "ping" => exec_ping(config, args).await,
        "update" => exec_update(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
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

    let table = Table::new(&rows)
        .with(Style::empty())
        .with(Modify::new(Columns::first()).with(Alignment::right()));
    println!("{}", table);

    Ok(())
}

pub async fn exec_set_default(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").unwrap();
    let project = *args.get_one::<bool>("project").unwrap();
    config.set_default_server(server, project)?;
    config.save();
    Ok(())
}

pub async fn exec_add(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let url = args.get_one::<String>("url").unwrap();
    let nickname = args.get_one::<String>("name");
    let project = *args.get_one::<bool>("project").unwrap();
    let default = *args.get_one::<bool>("default").unwrap();
    let no_fingerprint = *args.get_one::<bool>("no-fingerprint").unwrap();

    let (host, protocol) = host_or_url_to_host_and_protocol(url);
    let protocol = protocol.ok_or_else(|| anyhow::anyhow!("Invalid url: {}", url))?;

    if !VALID_PROTOCOLS.contains(&protocol) {
        return Err(anyhow::anyhow!("Invalid protocol: {}", protocol));
    }

    let fingerprint = if no_fingerprint {
        None
    } else {
        let fingerprint = spacetime_server_fingerprint(host, protocol).await?;
        println!("For server {}, got fingerprint:\n{}", url, fingerprint);
        Some(fingerprint)
    };

    config.add_server(
        host.to_string(),
        protocol.to_string(),
        fingerprint,
        nickname.cloned(),
        project,
    )?;

    if default {
        config.set_default_server(host, project)?;
    }

    println!("Host: {}", host);
    println!("Protocol: {}", protocol);

    config.save();

    Ok(())
}

pub async fn exec_remove(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").unwrap();
    let project = *args.get_one::<bool>("project").unwrap();
    let delete_identities = *args.get_one::<bool>("delete-identities").unwrap();

    config.remove_server(server, project, delete_identities)?;
    config.save();

    Ok(())
}

pub async fn exec_fingerprint(_config: Config, _args: &ArgMatches) -> Result<(), anyhow::Error> {
    todo!()
}

pub async fn exec_ping(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let url = config.get_host_url(server)?;

    let builder = reqwest::Client::new().get(format!("{}/database/ping", url).as_str());
    match builder.send().await {
        Ok(_) => {
            println!("Server is online: {}", url);
        }
        Err(_) => {
            println!("Server could not be reached: {}", url);
        }
    }
    Ok(())
}

pub async fn exec_update(_config: Config, _args: &ArgMatches) -> Result<(), anyhow::Error> {
    todo!()
}
