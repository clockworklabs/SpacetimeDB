use crate::{
    common_args,
    config::{Config, IdentityConfig},
    util::{init_default, y_or_n, IdentityTokenJson, InitDefaultResultType},
};
use std::io::Write;

use crate::util::print_identity_config;
use anyhow::Context;
use clap::{Arg, ArgAction, ArgMatches, Command};
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use spacetimedb::auth::identity::decode_token;
use spacetimedb_client_api_messages::recovery::RecoveryCodeResponse;
use spacetimedb_lib::Identity;
use tabled::{
    settings::{object::Columns, Alignment, Modify, Style},
    Table, Tabled,
};

pub fn cli() -> Command {
    Command::new("identity")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_subcommands())
        .about("Manage identities stored by the command line tool")
}

fn get_subcommands() -> Vec<Command> {
    vec![
        Command::new("import")
            .about("Import an existing identity into your spacetime config")
            .arg(
                common_args::identity()
                    .required(true)
                    .value_parser(clap::value_parser!(Identity))
                    .help("The identity string associated with the provided token"),
            )
            .arg(
                Arg::new("token")
                    .required(true)
                    .help("The identity token to import. This is used for authenticating with SpacetimeDB"),
            )
            .arg(
                Arg::new("name")
                    .long("name")
                    .short('n')
                    .help("A name for the newly imported identity"),
            )
            // TODO: project flag?
            ,
        Command::new("init-default")
            .about("Initialize a new default identity if it is missing from a server's config")
            .arg(
                common_args::server()
                    .help("The nickname, host name or URL of the server for which to set the default identity"),
            )
            .arg(
                Arg::new("name")
                    .long("name")
                    .short('n')
                    .help("The name of the identity that should become the new default identity"),
            )
            .arg(
                Arg::new("quiet")
                    .long("quiet")
                    .short('q')
                    .action(ArgAction::SetTrue)
                    .help("Runs command in silent mode"),
            ),
        Command::new("list").about("List saved identities which apply to a server")
            .arg(
                common_args::server()
                    .help("The nickname, host name or URL of the server to list identities for")
                    .conflicts_with("all")
            )
            .arg(
                Arg::new("all")
                    .short('a')
                    .long("all")
                    .help("List all stored identities, regardless of server")
                    .action(ArgAction::SetTrue)
                    .conflicts_with("server")
            )
            // TODO: project flag?
            ,
        Command::new("new")
            .about("Creates a new identity")
            .arg(
                common_args::server()
                    .help("The nickname, host name or URL of the server from which to request the identity"),
            )
            .arg(
                Arg::new("no-save")
                    .help("Don't save to local config, just create a new identity")
                    .long("no-save")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("name")
                    .long("name")
                    .short('n')
                    .help("Nickname for this identity")
                    .conflicts_with("no-save"),
            )
            .arg(
                Arg::new("default")
                    .help("Make the new identity the default for the server")
                    .long("default")
                    .short('d')
                    .conflicts_with("no-save")
                    .action(ArgAction::SetTrue),
            ),
        Command::new("remove")
            .about("Removes a saved identity from your spacetime config")
            .arg(common_args::identity()
                .help("The identity string or name to delete")
            )
            .arg(
                Arg::new("all-server")
                    .long("all-server")
                    .short('s')
                    .help("Remove all identities associated with a particular server")
                    .conflicts_with_all(["identity", "all"])
            )
            .arg(
                Arg::new("all")
                    .long("all")
                    .short('a')
                    .help("Remove all identities from your spacetime config")
                    .action(ArgAction::SetTrue)
                    .conflicts_with_all(["identity", "all-server"])
            ).arg(
                common_args::yes()
            )
            // TODO: project flag?
            ,
        Command::new("token").about("Print the token for an identity").arg(
            common_args::identity()
                .help("The identity string or name that we should print the token for")
                .required(true),
        ),
        Command::new("set-default").about("Set the default identity for a server")
            .arg(
                common_args::identity()
                    .help("The identity string or name that should become the new default identity")
                    .required(true),
            )
            .arg(
                common_args::server()
                    .help("The server nickname, host name or URL of the server which should use this identity as a default")
            )
            // TODO: project flag?
            ,
        Command::new("set-name").about("Set the name of an identity or rename an existing identity nickname").arg(
            common_args::identity()
                .help("The identity string or name to be named. If a name is supplied, the corresponding identity will be renamed.")
                .required(true))
            .arg(Arg::new("name")
                .help("The new name for the identity")
                .required(true)
        ),
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
        "init-default" => exec_init_default(config, args).await,
        "new" => exec_new(config, args).await,
        "remove" => exec_remove(config, args).await,
        "set-name" => exec_set_name(config, args).await,
        "import" => exec_import(config, args).await,
        "token" => exec_token(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

/// Executes the `identity set-default` command which sets the default identity.
async fn exec_set_default(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let identity = config.resolve_name_to_identity(args.get_one::<String>("identity").unwrap())?;
    config.set_default_identity(
        identity.to_hex().to_string(),
        args.get_one::<String>("server").map(|s| s.as_ref()),
    )?;
    config.save();
    Ok(())
}

// TODO(cloutiertyler): Realistically this should just be run before every
//  single command, but I'm separating it out into its own command for now for
//  simplicity.
/// Executes the `identity init-default` command which initializes the default identity.
async fn exec_init_default(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let nickname = args.get_one::<String>("name").map(|s| s.to_owned());
    let quiet = args.get_flag("quiet");

    let init_default_result = init_default(
        &mut config,
        nickname,
        args.get_one::<String>("server").map(|s| s.as_ref()),
    )
    .await?;
    let identity_config = init_default_result.identity_config;
    let result_type = init_default_result.result_type;

    if !quiet {
        match result_type {
            InitDefaultResultType::Existing => {
                println!(" Existing default identity");
                print_identity_config(&identity_config);
                return Ok(());
            }
            InitDefaultResultType::SavedNew => {
                println!(" Saved new identity");
                print_identity_config(&identity_config);
            }
        }
    }

    Ok(())
}

/// Executes the `identity remove` command which removes an identity from the config.
async fn exec_remove(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let identity_or_name = args.get_one::<String>("identity");
    let force = args.get_flag("force");
    let all = args.get_flag("all");
    let all_server = args.get_one::<String>("all-server").map(|s| s.as_str());

    if !all && identity_or_name.is_none() && all_server.is_none() {
        return Err(anyhow::anyhow!("Must provide an identity or name to remove"));
    }

    fn should_continue(force: bool, prompt: &str) -> anyhow::Result<bool> {
        y_or_n(
            force,
            &format!("Are you sure you want to remove all identities{}?", prompt),
        )
    }

    if let Some(identity_or_name) = identity_or_name {
        let ic = if let Ok(identity) = Identity::from_hex(identity_or_name) {
            config.delete_identity_config_by_identity(&identity)
        } else {
            config.delete_identity_config_by_name(identity_or_name.as_str())
        }
        .ok_or(anyhow::anyhow!("No such identity or name: {}", identity_or_name))?;
        config.update_all_default_identities();
        println!(" Removed identity");
        print_identity_config(&ic);
    } else if let Some(server) = all_server {
        if !should_continue(force, &format!(" which apply to server {}", server))? {
            println!(" Aborted");
            return Ok(());
        }
        let removed = config.remove_identities_for_server(Some(server))?;
        let count = removed.len();
        println!(
            " {} {} removed:",
            count,
            if count == 1 { "identity" } else { "identities" }
        );
        for identity_config in removed {
            println!("{}", identity_config.identity);
        }
    } else {
        if config.identity_configs().is_empty() {
            println!(" No identities to remove");
            return Ok(());
        }

        if !should_continue(force, "")? {
            println!(" Aborted");
            return Ok(());
        }

        let identity_count = config.identity_configs().len();
        config.delete_all_identity_configs();
        println!(
            " {} {} removed.",
            identity_count,
            if identity_count == 1 { "identity" } else { "identities" }
        );
    }
    config.save();
    Ok(())
}

/// Executes the `identity new` command which creates a new identity.
async fn exec_new(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let save = !args.get_flag("no-save");
    let alias = args.get_one::<String>("name");
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let default = *args.get_one::<bool>("default").unwrap();
    if let Some(x) = alias {
        config.can_set_name(x)?;
    }

    let mut query_params = Vec::<(&str, &str)>::new();

    let mut builder = reqwest::Client::new().post(Url::parse_with_params(
        format!("{}/identity", config.get_host_url(server)?).as_str(),
        query_params,
    )?);

    if let Ok(identity_token) = config.get_default_identity_config(server) {
        builder = builder.basic_auth("token", Some(identity_token.token.clone()));
    }

    let identity_token: IdentityTokenJson = builder.send().await?.error_for_status()?.json().await?;
    let identity = identity_token.identity;

    if save {
        config.identity_configs_mut().push(IdentityConfig {
            identity: identity_token.identity,
            token: identity_token.token,
            nickname: alias.map(|s| s.to_string()),
        });
        if default || config.default_identity(server).is_err() {
            config.set_default_identity(identity.to_hex().to_string(), server)?;
        }

        config.save();
    }

    println!(" IDENTITY     {}", identity);
    println!(" NAME         {}", alias.unwrap_or(&String::new()));

    Ok(())
}

/// Executes the `identity import` command which imports an identity from a token into the config.
async fn exec_import(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let identity: Identity = *args.get_one::<Identity>("identity").unwrap();
    let token: String = args.get_one::<String>("token").unwrap().clone();

    //optional
    let nickname = args.get_one::<String>("name").cloned();
    if let Some(x) = nickname.as_deref() {
        config.can_set_name(x)?;
    }

    if config.identity_exists(&identity) {
        return Err(anyhow::anyhow!("Identity \"{}\" already exists in config", identity));
    };

    config.identity_configs_mut().push(IdentityConfig {
        identity,
        token,
        nickname: nickname.clone(),
    });

    config.save();

    println!(" New Identity Imported");
    println!(" NAME      {}", nickname.unwrap_or_default());
    // TODO(jdetter): For consistency lets query the database for the user's email and maybe any domain names
    //  associated with this identity.

    Ok(())
}

#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
struct LsRow {
    default: String,
    identity: Identity,
    name: String,
    // email: String,
}

/// Executes the `identity list` command which lists all identities in the config.
async fn exec_list(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let mut rows: Vec<LsRow> = Vec::new();

    if *args.get_one::<bool>("all").unwrap() {
        for identity_token in config.identity_configs() {
            rows.push(LsRow {
                default: "".to_string(),
                identity: identity_token.identity,
                name: identity_token.nickname.clone().unwrap_or_default(),
            });
        }
    } else {
        let server = args.get_one::<String>("server").map(|s| s.as_ref());

        let server_name = config.server_nick_or_host(server)?;
        let decoding_key = config.server_decoding_key(server).with_context(|| {
            format!(
                "Cannot list identities for server without a saved fingerprint: {server_name}
Fetch the server's fingerprint with:
\tspacetime server fingerprint -s {server_name}"
            )
        })?;
        let default_identity = config.get_default_identity_config(server).ok().map(|cfg| cfg.identity);

        for identity_token in config.identity_configs() {
            if decode_token(&decoding_key, &identity_token.token).is_ok() {
                rows.push(LsRow {
                    default: if Some(identity_token.identity) == default_identity {
                        "***"
                    } else {
                        ""
                    }
                    .to_string(),
                    identity: identity_token.identity,
                    name: identity_token.nickname.clone().unwrap_or_default(),
                    // TODO(jdetter): We'll have to look this up via a query
                    // email: identity_token.email.unwrap_or_default(),
                });
            }
        }
        println!("Identities for {}:", config.server_nick_or_host(server)?);
    }

    let mut table = Table::new(&rows);
    table
        .with(Style::empty())
        .with(Modify::new(Columns::first()).with(Alignment::right()));
    println!("{}", table);
    Ok(())
}

/// Executes the `identity token` command which prints the token for an identity.
async fn exec_token(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let identity = args.get_one::<String>("identity").unwrap();
    let ic = config
        .get_identity_config(identity)
        .ok_or_else(|| anyhow::anyhow!("Missing identity credentials for identity: {identity}"))?;
    println!("{}", ic.token);
    Ok(())
}

/// Executes the `identity set-default` command which sets the default identity.
async fn exec_set_name(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let new_name = args.get_one::<String>("name").unwrap();
    let identity = args.get_one::<String>("identity").unwrap();
    config.can_set_name(new_name.as_str())?;
    let ic = config
        .get_identity_config_mut(identity)
        .ok_or_else(|| anyhow::anyhow!("Missing identity credentials for identity: {identity}"))?;
    ic.nickname = Some(new_name.to_owned());
    println!("Updated identity:");
    print_identity_config(ic);
    config.save();
    Ok(())
}
