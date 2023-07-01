use crate::{
    config::{Config, IdentityConfig},
    util::{init_default, IdentityTokenJson, InitDefaultResultType},
};
use std::io::Write;

use crate::util::{is_hex_identity, print_identity_config};
use clap::{Arg, ArgAction, ArgMatches, Command};
use email_address::EmailAddress;
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use spacetimedb_lib::recovery::RecoveryCodeResponse;
use tabled::{object::Columns, Alignment, Modify, Style, Table, Tabled};

pub fn cli() -> Command {
    Command::new("identity")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_subcommands())
        .about("Manage identities stored by the command line tool")
}

fn get_subcommands() -> Vec<Command> {
    vec![
        Command::new("list").about("List saved identities"),
        Command::new("set-default").about("Set the default identity").arg(
            Arg::new("identity")
                .help("The identity string or name that should become the new default identity")
                .required(true),
        ),
        Command::new("set-email")
            .about("Associates an email address with an identity")
            .arg(
                Arg::new("identity")
                    .help("The identity string or name that should become the new default identity")
                    .required(true),
            )
            .arg(
                Arg::new("email")
                    .help("The email that should be assigned to the provided identity")
                    .required(true),
            ),
        Command::new("init-default")
            .about("Initialize a new default identity if it is missing from the global config")
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
            ),
        Command::new("remove")
            .about("Removes a saved identity from your spacetime config")
            .arg(Arg::new("identity")
                .help("The identity string or name to delete")
                .required_unless_present("all")
            )
            .arg(
                Arg::new("all")
                    .long("all")
                    .short('a')
                    .help("Remove all identities from your spacetime config")
                    .action(ArgAction::SetTrue)
                    .conflicts_with("identity")
                    .required_unless_present("identity"),
            ).arg(
                Arg::new("force")
                    .long("force")
                    .help("Removes all identities without prompting (for CI usage)")
                    .action(ArgAction::SetTrue)
                    .conflicts_with("identity")
                    .required_unless_present("identity"),
            ),
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
            ),
        Command::new("find").about("Find an identity for an email").arg(
            Arg::new("email")
                .required(true)
                .help("The email associated with the identity that you would like to find"),
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
            )),
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
    let identity = config
        .resolve_name_to_identity(args.get_one::<String>("identity").map(|s| s.as_ref()))
        .unwrap();
    config.set_default_identity(identity);
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

    let init_default_result = init_default(&mut config, nickname).await?;
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

    if !all && identity_or_name.is_none() {
        return Err(anyhow::anyhow!("Must provide an identity or name to remove"));
    }

    if force && !all {
        return Err(anyhow::anyhow!("The --force flag can only be used with --all"));
    }

    if let Some(identity_or_name) = identity_or_name {
        let ic = if is_hex_identity(identity_or_name) {
            config.delete_identity_config_by_identity(identity_or_name.as_str())
        } else {
            config.delete_identity_config_by_name(identity_or_name.as_str())
        }
        .unwrap_or_else(|| panic!("No such identity or name: {}", identity_or_name));
        config.update_default_identity();
        config.save();
        println!(" Removed identity");
        print_identity_config(&ic);
    } else {
        if config.identity_configs().is_empty() {
            println!(" No identities to remove");
            return Ok(());
        }

        let mut input = String::new();
        if !force {
            print!("Are you sure you want to remove all identities? (y/n) ");
            std::io::stdout().flush()?;
            std::io::stdin().read_line(&mut input)?;

            if input.trim() != "y" {
                println!(" Aborted");
                return Ok(());
            }
        }

        let identity_count = config.identity_configs().len();
        config.delete_all_identity_configs();
        config.save();
        println!(
            " {} {} removed.",
            identity_count,
            if identity_count > 1 { "identities" } else { "identity" }
        );
    }
    Ok(())
}

/// Executes the `identity new` command which creates a new identity.
async fn exec_new(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let save = !args.get_flag("no-save");
    let alias = args.get_one::<String>("name");
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
        format!("{}/identity", config.get_host_url()).as_str(),
        query_params,
    )?);

    if let Some(identity_token) = config.get_default_identity_config() {
        builder = builder.basic_auth("token", Some(identity_token.token.clone()));
    }

    let identity_token: IdentityTokenJson = builder.send().await?.error_for_status()?.json().await?;
    let identity = identity_token.identity.clone();

    if save {
        config.identity_configs_mut().push(IdentityConfig {
            identity: identity_token.identity,
            token: identity_token.token,
            nickname: alias.map(|s| s.to_string()),
        });
        if config.default_identity().is_none() {
            config.set_default_identity(identity.clone());
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
    let identity: String = args.get_one::<String>("identity").unwrap().clone();
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
    identity: String,
    name: String,
    // email: String,
}

/// Executes the `identity list` command which lists all identities in the config.
async fn exec_list(config: Config, _args: &ArgMatches) -> Result<(), anyhow::Error> {
    let mut rows: Vec<LsRow> = Vec::new();
    for identity_token in config.identity_configs() {
        let default_str = if config.default_identity().is_some()
            && config.default_identity().as_ref().unwrap() == &identity_token.identity
        {
            "***"
        } else {
            ""
        };
        rows.push(LsRow {
            default: default_str.to_string(),
            identity: identity_token.clone().identity,
            name: identity_token.nickname.clone().unwrap_or_default(),
            // TODO(jdetter): We'll have to look this up via a query
            // email: identity_token.email.unwrap_or_default(),
        });
    }
    let table = Table::new(&rows)
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
    let client = reqwest::Client::new();
    let builder = client.get(format!("{}/identity?email={}", config.get_host_url(), email));

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
    let identity_or_name = config
        .resolve_name_to_identity(args.get_one::<String>("identity").map(|s| s.as_str()))
        .unwrap();
    let ic = config
        .get_identity_config_by_identity(identity_or_name.as_str())
        .unwrap();
    println!("{}", ic.token);
    Ok(())
}

/// Executes the `identity set-default` command which sets the default identity.
async fn exec_set_name(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let cloned_config = config.clone();
    let identity_or_name = cloned_config
        .resolve_name_to_identity(args.get_one::<String>("identity").map(|s| s.as_ref()))
        .unwrap();
    let new_name = args.get_one::<String>("name").unwrap().as_ref();
    let old_nickname = config.set_identity_nickname(identity_or_name.as_ref(), new_name)?;
    if let Some(old_nickname) = old_nickname {
        println!("Updated identity: {}", identity_or_name);
        println!(" OLD NAME: {}", old_nickname);
        println!(" NEW NAME: {}", new_name);
    } else {
        println!("Created identity: {}", identity_or_name);
        println!(" NAME: {}", new_name);
    }
    config.save();
    let ic = config
        .get_identity_config_by_identity(identity_or_name.as_ref())
        .unwrap();
    print_identity_config(ic);
    Ok(())
}

/// Executes the `identity set-email` command which sets the email for an identity.
async fn exec_set_email(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let email = args.get_one::<String>("email").unwrap().clone();
    let identity = config
        .resolve_name_to_identity(args.get_one::<String>("identity").map(|s| s.as_ref()))
        .unwrap();
    let identity_config = config
        .get_identity_config_by_identity(identity.as_str())
        .unwrap_or_else(|| panic!("Could not find identity: {}", identity));

    let mut builder = reqwest::Client::new().post(format!(
        "{}/identity/{}/set-email?email={}",
        config.get_host_url(),
        identity_config.identity,
        email
    ));

    if let Some(identity_token) = config.get_identity_config_by_identity(identity.as_str()) {
        builder = builder.basic_auth("token", Some(identity_token.token.clone()));
    } else {
        println!("Missing identity credentials for identity.");
        std::process::exit(0);
    }

    builder.send().await?.error_for_status()?;

    println!(" Associated email with identity");
    print_identity_config(config.get_identity_config_by_identity(identity.as_str()).unwrap());
    println!(" EMAIL {}", email);

    Ok(())
}

/// Executes the `identity recover` command which recovers an identity from an email.
async fn exec_recover(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let email = args.get_one::<String>("email").unwrap();
    let identity = config
        .resolve_name_to_identity(args.get_one::<String>("identity").map(|s| s.as_str()))
        .unwrap();

    let query_params = vec![
        ("email", email.as_str()),
        ("identity", identity.as_str()),
        ("link", "false"),
    ];

    if config
        .identity_configs()
        .iter()
        .any(|a| a.identity.to_lowercase() == identity.to_lowercase())
    {
        return Err(anyhow::anyhow!("No need to recover this identity, it is already stored in your config. Use `spacetime identity list` to list identities."));
    }

    let client = reqwest::Client::new();
    let builder = client.get(Url::parse_with_params(
        format!("{}/database/request_recovery_code", config.get_host_url()).as_str(),
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
            format!("{}/database/confirm_recovery_code", config.get_host_url()).as_str(),
            vec![
                ("code", code.to_string().as_str()),
                ("email", email.as_str()),
                ("identity", identity.as_str()),
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
                    identity: response.identity.clone(),
                    token: response.token,
                };
                config.identity_configs_mut().push(identity_config.clone());
                config.update_default_identity();
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
