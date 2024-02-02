use crate::{
    util::{host_or_url_to_host_and_protocol, spacetime_server_fingerprint, y_or_n, VALID_PROTOCOLS},
    Config,
};
use anyhow::Context;
use clap::{Arg, ArgAction, ArgMatches, Command};
use tabled::{
    settings::{object::Columns, Alignment, Modify, Style},
    Table, Tabled,
};

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
            ),
        Command::new("add")
            .about("Add a new server configuration")
            .arg(Arg::new("url").help("The URL of the server to add").required(true))
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
            .arg(
                Arg::new("delete-identities")
                    .help("Also delete all identities which apply to the server")
                    .long("delete-identities")
                    .short('I')
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("force")
                    .help("Do not prompt before deleting identities")
                    .long("force")
                    .short('f')
                    .action(ArgAction::SetTrue),
            ),
        Command::new("fingerprint")
            .about("Show or update a saved server's fingerprint")
            .arg(Arg::new("server").help("The nickname, host name or URL of the server"))
            .arg(
                Arg::new("force")
                    .help("Save changes to the server's configuration without confirming")
                    .short('f')
                    .long("force")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("delete-obsolete-identities")
                    .help("Delete obsoleted identities if the server's fingerprint has changed")
                    .long("delete-obsolete-identities")
                    .short('I')
                    .action(ArgAction::SetTrue),
            ),
        Command::new("ping")
            .about("Checks to see if a SpacetimeDB host is online")
            .arg(Arg::new("server").help("The nickname, host name or URL of the server to ping")),
        Command::new("edit")
            .about("Update a saved server's nickname, host name or protocol")
            .arg(Arg::new("server").help("The nickname, host name or URL of the server"))
            .arg(
                Arg::new("nickname")
                    .help("A new nickname to assign the server configuration")
                    .short('n')
                    .long("nickname"),
            )
            .arg(
                Arg::new("host")
                    .help("A new hostname to assign the server configuration")
                    .short('H')
                    .long("host"),
            )
            .arg(
                Arg::new("protocol")
                    .help("A new protocol to assign the server configuration; http or https")
                    .short('p')
                    .long("protocol"),
            )
            .arg(
                Arg::new("no-fingerprint")
                    .help("Skip fingerprinting the server")
                    .long("no-fingerprint")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("delete-obsolete-identities")
                    .help("Delete obsoleted identities if the server's fingerprint has changed")
                    .long("delete-obsolete-identities")
                    .short('I')
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("force")
                    .help("Do not prompt before saving the edited configuration")
                    .long("force")
                    .short('f')
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
        "edit" => exec_edit(config, args).await,
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

    let mut table = Table::new(&rows);
    table
        .with(Style::empty())
        .with(Modify::new(Columns::first()).with(Alignment::right()));
    println!("{}", table);

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
        Err(anyhow::anyhow!("Invalid protocol: {}", protocol))
    } else {
        Ok(())
    }
}

pub async fn exec_add(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let url = args.get_one::<String>("url").unwrap();
    let nickname = args.get_one::<String>("name");
    let default = *args.get_one::<bool>("default").unwrap();
    let no_fingerprint = *args.get_one::<bool>("no-fingerprint").unwrap();

    let (host, protocol) = host_or_url_to_host_and_protocol(url);
    let protocol = protocol.ok_or_else(|| anyhow::anyhow!("Invalid url: {}", url))?;

    valid_protocol_or_error(protocol)?;

    let fingerprint = if no_fingerprint {
        None
    } else {
        let fingerprint = spacetime_server_fingerprint(url).await.with_context(|| {
            format!(
                "Unable to retrieve fingerprint for server: {url}
Is the server running?
Add a server without retrieving its fingerprint with:
\tspacetime server add {url} --no-fingerprint",
            )
        })?;
        println!("For server {}, got fingerprint:\n{}", url, fingerprint);
        Some(fingerprint)
    };

    config.add_server(host.to_string(), protocol.to_string(), fingerprint, nickname.cloned())?;

    if default {
        config.set_default_server(host)?;
    }

    println!("Host: {}", host);
    println!("Protocol: {}", protocol);

    config.save();

    Ok(())
}

pub async fn exec_remove(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").unwrap();
    let delete_identities = args.get_flag("delete-identities");
    let force = args.get_flag("force");

    let deleted_ids = config.remove_server(server, delete_identities)?;

    if !deleted_ids.is_empty() {
        println!(
            "Deleting {} {}:",
            deleted_ids.len(),
            if deleted_ids.len() == 1 {
                " identity"
            } else {
                "identities"
            }
        );
        for id in deleted_ids {
            println!("{}", id.identity);
        }
        if !(force || y_or_n("Continue?")?) {
            anyhow::bail!("Aborted");
        }

        config.update_all_default_identities();
    }

    config.save();

    Ok(())
}

async fn update_server_fingerprint(
    config: &mut Config,
    server: Option<&str>,
    delete_identities: bool,
) -> Result<bool, anyhow::Error> {
    let url = config.get_host_url(server)?;
    let nick_or_host = config.server_nick_or_host(server)?;
    let new_fing = spacetime_server_fingerprint(&url)
        .await
        .context("Error fetching server fingerprint")?;
    if let Some(saved_fing) = config.server_fingerprint(server)? {
        if saved_fing == new_fing {
            println!("Fingerprint is unchanged for server {}:\n{}", nick_or_host, saved_fing);

            Ok(false)
        } else {
            println!(
                "Fingerprint has changed for server {}.\nWas:\n{}\nNew:\n{}",
                nick_or_host, saved_fing, new_fing
            );

            if delete_identities {
                // Unfortunate clone because we need to mutate `config`
                // while holding `saved_fing`.
                let saved_fing = saved_fing.to_string();

                let deleted_ids = config.remove_identities_for_fingerprint(&saved_fing)?;
                if !deleted_ids.is_empty() {
                    println!(
                        "Deleting {} obsolete {}:",
                        deleted_ids.len(),
                        if deleted_ids.len() == 1 {
                            "identity"
                        } else {
                            "identities"
                        }
                    );
                    for id in deleted_ids {
                        println!("{}", id.identity);
                    }
                }

                config.update_all_default_identities();
            }

            config.set_server_fingerprint(server, new_fing)?;

            Ok(true)
        }
    } else {
        println!(
            "No saved fingerprint for server {}. New fingerprint:\n{}",
            nick_or_host, new_fing
        );

        config.set_server_fingerprint(server, new_fing)?;

        Ok(true)
    }
}

pub async fn exec_fingerprint(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_str());
    let delete_identities = args.get_flag("delete-obsolete-identities");
    let force = args.get_flag("force");

    if update_server_fingerprint(&mut config, server, delete_identities).await? {
        if !(force || y_or_n("Continue?")?) {
            anyhow::bail!("Aborted");
        }

        config.save();
    }

    Ok(())
}

pub async fn exec_ping(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let url = config.get_host_url(server)?;

    let builder = reqwest::Client::new().get(format!("{}/database/ping", url).as_str());
    let response = builder.send().await?;

    match response.status() {
        reqwest::StatusCode::OK => {
            println!("Server is online: {}", url);
        }
        reqwest::StatusCode::NOT_FOUND => {
            println!("Server returned 404 (Not Found): {}", url);
        }
        err => {
            println!("Server could not be reached ({}): {}", err, url);
        }
    }
    Ok(())
}

pub async fn exec_edit(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args
        .get_one::<String>("server")
        .map(|s| s.as_str())
        .expect("Supply a server to spacetime server edit");

    let old_url = config.get_host_url(Some(server))?;

    let new_nick = args.get_one::<String>("nickname").map(|s| s.as_str());
    let new_host = args.get_one::<String>("host").map(|s| s.as_str());
    let new_proto = args.get_one::<String>("protocol").map(|s| s.as_str());

    let no_fingerprint = args.get_flag("no-fingerprint");
    let delete_identities = args.get_flag("delete-obsolete-identities");
    let force = args.get_flag("force");

    if let Some(new_proto) = new_proto {
        valid_protocol_or_error(new_proto)?;
    }

    let (old_nick, old_host, old_proto) = config.edit_server(server, new_nick, new_host, new_proto)?;

    if let (Some(new_nick), Some(old_nick)) = (new_nick, old_nick) {
        println!("Changing nickname from {} to {}", old_nick, new_nick);
    }
    if let (Some(new_host), Some(old_host)) = (new_host, old_host) {
        println!("Changing host from {} to {}", old_host, new_host);
    }
    if let (Some(new_proto), Some(old_proto)) = (new_proto, old_proto) {
        println!("Changing protocol from {} to {}", old_proto, new_proto);
    }

    let new_url = config.get_host_url(Some(server))?;

    if old_url != new_url {
        if no_fingerprint {
            config.delete_server_fingerprint(Some(&new_url))?;
        } else {
            update_server_fingerprint(&mut config, Some(&new_url), delete_identities).await?;
        }
    }

    if !(force || y_or_n("Continue?")?) {
        anyhow::bail!("Aborted");
    }

    config.save();

    Ok(())
}
