use crate::{
    config::{Config, IdentityConfig},
    util::{init_default, y_or_n, IdentityTokenJson, InitDefaultResultType},
};
use std::io::Write;

use crate::util::{is_hex_identity, print_identity_config};
use anyhow::Context;
use clap::{Arg, ArgAction, ArgMatches, Command};
use email_address::EmailAddress;
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use spacetimedb::auth::identity::decode_token;
use spacetimedb_lib::{recovery::RecoveryCodeResponse, Identity};
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
        Command::new("list").about("List saved identities which apply to a server")
            .arg(
                Arg::new("server")
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
        Command::new("set-default").about("Set the default identity for a server")
            .arg(
                Arg::new("identity")
                    .help("The identity string or name that should become the new default identity")
                    .required(true),
            )
            .arg(
                Arg::new("server")
                    .long("server")
                    .short('s')
                    .help("The server nickname, host name or URL of the server which should use this identity as a default")
            )
            // TODO: project flag?
            ,
        Command::new("set-email")
            .about("Associates an email address with an identity")
            .arg(
                Arg::new("identity")
                    .help("The identity string or name that should be associated with the email")
                    .required(true),
            )
            .arg(
                Arg::new("email")
                    .help("The email that should be assigned to the provided identity")
                    .required(true),
            )
            .arg(
                Arg::new("server")
                    .long("server")
                    .short('s')
                    .help("The server that should be informed of the email change")
                    .conflicts_with("all-servers")
            )
            .arg(
                Arg::new("all-servers")
                    .long("all-servers")
                    .short('a')
                    .action(ArgAction::SetTrue)
                    .help("Inform all known servers of the email change")
                    .conflicts_with("server")
            ),
        Command::new("init-default")
            .about("Initialize a new default identity if it is missing from a server's config")
            .arg(
                Arg::new("server")
                    .long("server")
                    .short('s')
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
        Command::new("new")
            .about("Creates a new identity")
            .arg(
                Arg::new("server")
                    .long("server")
                    .short('s')
                    .help("The nickname, host name or URL of the server from which to request the identity"),
            )
            .arg(
                Arg::new("no-save")
                    .help("Don't save save to local config, just create a new identity")
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
                Arg::new("email")
                    .long("email")
                    .short('e')
                    .help("Recovery email for this identity")
                    .conflicts_with("no-email"),
            )
            .arg(
                Arg::new("no-email")
                    .long("no-email")
                    .help("Creates an identity without a recovery email")
                    .conflicts_with("email")
                    .action(ArgAction::SetTrue),
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
            .arg(Arg::new("identity")
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
                Arg::new("force")
                    .long("force")
                    .help("Removes all identities without prompting (for CI usage)")
                    .action(ArgAction::SetTrue)
                    .conflicts_with("identity")
            )
            // TODO: project flag?
            ,
        Command::new("token").about("Print the token for an identity").arg(
            Arg::new("identity")
                .help("The identity string or name that we should print the token for")
                .required(true),
        ),
        Command::new("set-name").about("Set the name of an identity or rename an existing identity nickname").arg(
            Arg::new("identity")
                .help("The identity string or name to be named. If a name is supplied, the corresponding identity will be renamed.")
                .required(true))
            .arg(Arg::new("name")
                .help("The new name for the identity")
                .required(true)
        ),
        Command::new("import")
            .about("Import an existing identity into your spacetime config")
            .arg(
                Arg::new("identity")
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
        Command::new("find").about("Find an identity for an email")
            .arg(
                Arg::new("email")
                    .required(true)
                    .help("The email associated with the identity that you would like to find"),
            )
            .arg(
                Arg::new("server")
                    .long("server")
                    .short('s')
                    .help("The server to search for identities matching the email"),
            ),
        Command::new("recover")
            .about("Recover an existing identity and import it into your local config")
            .arg(
                Arg::new("email")
                    .required(true)
                    .help("The email associated with the identity that you would like to recover."),
            )
            .arg(Arg::new("identity").required(true).help(
                "The identity you would like to recover. This identity must be associated with the email provided.",
            ).value_parser(clap::value_parser!(Identity)))
            .arg(
                Arg::new("server")
                    .long("server")
                    .short('s')
                    .help("The server from which to request recovery codes"),
            )
            // TODO: project flag?
            ,
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
        "set-email" => exec_set_email(config, args).await,
        "find" => exec_find(config, args).await,
        "token" => exec_token(config, args).await,
        "recover" => exec_recover(config, args).await,
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

    if force && !(all || all_server.is_some()) {
        return Err(anyhow::anyhow!(
            "The --force flag can only be used with --all or --all-server"
        ));
    }

    fn should_continue(force: bool, prompt: &str) -> anyhow::Result<bool> {
        Ok(force || y_or_n(&format!("Are you sure you want to remove all identities{}?", prompt))?)
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

    if let Some(alias) = alias {
        if config.name_exists(alias) {
            return Err(anyhow::anyhow!("An identity with that name already exists."));
        }

        if is_hex_identity(alias.as_str()) {
            return Err(anyhow::anyhow!("An identity name cannot be an identity."));
        }
    }

    let email = args.get_one::<String>("email");
    let no_email = args.get_flag("no-email");
    if email.is_none() && !no_email {
        return Err(anyhow::anyhow!(
            "You must either supply an email with --email <email>, or pass the --no-email flag."
        ));
    }

    let mut query_params = Vec::<(&str, &str)>::new();
    if let Some(email) = email {
        if !EmailAddress::is_valid(email.as_str()) {
            return Err(anyhow::anyhow!("The email you provided is malformed: {}", email));
        }
        query_params.push(("email", email.as_str()))
    }

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
    println!(" EMAIL        {}", email.unwrap_or(&String::new()));

    Ok(())
}

/// Executes the `identity import` command which imports an identity from a token into the config.
async fn exec_import(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let identity: Identity = *args.get_one::<Identity>("identity").unwrap();
    let token: String = args.get_one::<String>("token").unwrap().clone();

    //optional
    let nickname = args.get_one::<String>("name").cloned();

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
\tspacetime server fingerprint {server_name}"
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

#[derive(Debug, Clone, Deserialize)]
struct GetIdentityResponse {
    identities: Vec<GetIdentityResponseEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct GetIdentityResponseEntry {
    identity: String,
    email: String,
}

/// Executes the `identity find` command which finds an identity by email.
async fn exec_find(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let email = args.get_one::<String>("email").unwrap().clone();
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let client = reqwest::Client::new();
    let builder = client.get(format!("{}/identity?email={}", config.get_host_url(server)?, email));

    let res = builder.send().await?;

    if res.status() == StatusCode::OK {
        let response: GetIdentityResponse = res.json().await?;
        if response.identities.is_empty() {
            return Err(anyhow::anyhow!("Could not find identity for: {}", email));
        }

        for identity in response.identities {
            println!("Identity");
            println!(" IDENTITY  {}", identity.identity);
            println!(" EMAIL     {}", identity.email);
        }
        Ok(())
    } else if res.status() == StatusCode::NOT_FOUND {
        Err(anyhow::anyhow!("Could not find identity for: {}", email))
    } else {
        Err(anyhow::anyhow!("Error occurred in lookup: {}", res.status()))
    }
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
    let identity_or_name = args.get_one::<String>("identity").unwrap();
    let ic = config
        .get_identity_config_mut(identity_or_name)
        .ok_or_else(|| anyhow::anyhow!("Missing identity credentials for identity: {identity_or_name}"))?;
    ic.nickname = Some(new_name.to_owned());
    println!("Updated identity:");
    print_identity_config(ic);
    config.save();
    Ok(())
}

/// Executes the `identity set-email` command which sets the email for an identity.
async fn exec_set_email(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let email = args.get_one::<String>("email").unwrap().clone();
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let identity = args.get_one::<String>("identity").unwrap();
    let identity_config = config
        .get_identity_config(identity)
        .ok_or_else(|| anyhow::anyhow!("Missing identity credentials for identity: {identity}"))?;

    // TODO: check that the identity is valid for the server

    reqwest::Client::new()
        .post(format!(
            "{}/identity/{}/set-email?email={}",
            config.get_host_url(server)?,
            identity_config.identity,
            email
        ))
        .basic_auth("token", Some(&identity_config.token))
        .send()
        .await?
        .error_for_status()?;

    println!(" Associated email with identity");
    print_identity_config(identity_config);
    println!(" EMAIL {}", email);

    Ok(())
}

/// Executes the `identity recover` command which recovers an identity from an email.
async fn exec_recover(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let identity = args.get_one::<Identity>("identity").unwrap();
    let email = args.get_one::<String>("email").unwrap();
    let server = args.get_one::<String>("server").map(|s| s.as_ref());

    let query_params = [
        ("email", email.as_str()),
        ("identity", &*identity.to_hex()),
        ("link", "false"),
    ];

    if config.get_identity_config_by_identity(identity).is_some() {
        return Err(anyhow::anyhow!("No need to recover this identity, it is already stored in your config. Use `spacetime identity list` to list identities."));
    }

    let client = reqwest::Client::new();
    let builder = client.get(Url::parse_with_params(
        format!("{}/database/request_recovery_code", config.get_host_url(server)?,).as_str(),
        query_params,
    )?);
    let res = builder.send().await?;
    res.error_for_status()?;

    println!(
        "We have successfully sent a recovery code to {}. Enter the code now.",
        email
    );
    for _ in 0..5 {
        print!("Recovery Code: ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        let code = match line.trim().parse::<u32>() {
            Ok(value) => value,
            Err(_) => {
                println!("Malformed code. Please try again.");
                continue;
            }
        };

        let client = reqwest::Client::new();
        let builder = client.get(Url::parse_with_params(
            format!("{}/database/confirm_recovery_code", config.get_host_url(server)?,).as_str(),
            vec![
                ("code", code.to_string().as_str()),
                ("email", email.as_str()),
                ("identity", identity.to_hex().as_str()),
            ],
        )?);
        let res = builder.send().await?;
        match res.error_for_status() {
            Ok(res) => {
                let buf = res.bytes().await?.to_vec();
                let utf8 = String::from_utf8(buf)?;
                let response: RecoveryCodeResponse = serde_json::from_str(utf8.as_str())?;
                let identity_config = IdentityConfig {
                    nickname: None,
                    identity: response.identity,
                    token: response.token,
                };
                config.identity_configs_mut().push(identity_config.clone());
                config.set_default_identity_if_unset(server, &identity_config.identity.to_hex())?;
                config.save();
                println!("Success. Identity imported.");
                print_identity_config(&identity_config);
                // TODO: Remove this once print_identity_config prints email
                println!(" EMAIL     {}", email);
                return Ok(());
            }
            Err(_) => {
                println!("Invalid recovery code, please try again.");
            }
        }
    }

    Err(anyhow::anyhow!(
        "Maximum amount of attempts reached. Please start the process over."
    ))
}
