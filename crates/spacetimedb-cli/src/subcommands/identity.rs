use crate::config::{Config, IdentityConfig};
use clap::{arg, Arg, ArgAction, ArgMatches, Command};
use reqwest::StatusCode;
use serde::Deserialize;
use tabled::{object::Columns, Alignment, Modify, Style, Table, Tabled};

#[derive(Deserialize)]
struct IdentityTokenJson {
    identity: String,
    token: String,
}

pub fn cli() -> Command<'static> {
    Command::new("identity")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_subcommands())
        .about("Manage identities stored by the command line tool")
}

fn get_subcommands() -> Vec<Command<'static>> {
    vec![
        Command::new("ls").about("List saved identities"),
        Command::new("set-default")
            .about("Set the default identity")
            .arg(Arg::new("identity").conflicts_with("name").required(true))
            .arg(arg!(-n --name <NAME> "name").conflicts_with("identity").required(true)),
        Command::new("set-email")
            .about("Associates an identity with an email address")
            .arg(Arg::new("identity").required(true))
            .arg(Arg::new("email").required(true)),
        Command::new("databases")
            .about("Lists the databases attached to an identity")
            .arg(Arg::new("identity").required(true)),
        Command::new("init-default")
            .about("Initialize a new default identity if missing")
            .arg(
                arg!(-n --name "Nickname for this identity")
                    .required(false)
                    .default_missing_value(""),
            ),
        Command::new("new")
            .about("Create a new identity")
            .arg(
                arg!(-s --save "Save to config")
                    .action(ArgAction::SetTrue)
                    .required(false),
            )
            .arg(
                arg!(-n --name "Nickname for this identity")
                    .required(false)
                    .default_missing_value(""),
            ),
        Command::new("delete")
            .about("Delete a saved identity")
            .arg(Arg::new("identity").conflicts_with("name").required(true))
            .arg(arg!(-n --name <NAME> "name").conflicts_with("identity").required(true)),
        Command::new("add")
            .about("Add an existing identity")
            .arg(Arg::new("identity").required(true))
            .arg(Arg::new("token").required(true))
            .arg(
                arg!(-n --name "Nickname for identity")
                    .required(false)
                    .default_missing_value(""),
            )
            .arg(
                arg!(-e --email "Nickname for identity")
                    .required(false)
                    .default_missing_value(""),
            ),
        Command::new("find")
            .about("Find an identity for an email")
            .arg(Arg::new("email").required(true)),
    ]
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    exec_subcommand(config, cmd, subcommand_args).await
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "ls" => exec_ls(config, args).await,
        "set-default" => exec_set_default(config, args).await,
        "init-default" => exec_init_default(config, args).await,
        "new" => exec_new(config, args).await,
        "rm" => exec_rm(config, args).await,
        "add" => exec_add(config, args).await,
        "set-email" => exec_email(config, args).await,
        "find" => exec_find(config, args).await,
        "databases" => exec_databases(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

async fn exec_set_default(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let name = args.get_one::<String>("name");
    if let Some(name) = name {
        if let Some(identity_config) = config.get_identity_config_by_name(name) {
            config.default_identity = Some(identity_config.identity.clone());
            config.save();
        } else {
            println!("No such identity by that name.");
            std::process::exit(0);
        }
    }

    if let Some(identity) = args.get_one::<String>("identity") {
        if let Some(identity_config) = config.get_identity_config_by_identity(identity) {
            config.default_identity = Some(identity_config.identity.clone());
            config.save();
        } else {
            println!("No such identity.");
            std::process::exit(0);
        }
    }

    Ok(())
}

// TODO(cloutiertyler): Realistically this should just be run before every
// single command, but I'm separating it out into its own command for now for
// simplicity.
async fn exec_init_default(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let nickname = args.get_one::<String>("name").unwrap_or(&"".to_string()).clone();
    if config.name_exists(&nickname) {
        println!("An identity with that name already exists.");
        std::process::exit(0);
    }

    let client = reqwest::Client::new();
    let builder = client.post(format!("http://{}/identity", config.host));

    if let Some(identity_config) = config.get_default_identity_config() {
        println!(" Existing default identity");
        println!(" IDENTITY  {}", identity_config.identity);
        println!(
            " NAME      {}",
            identity_config.nickname.clone().unwrap_or("".to_string())
        );
        return Ok(());
    }

    let res = builder.send().await?;
    let res = res.error_for_status()?;

    let body = res.bytes().await?;
    let body = String::from_utf8(body.to_vec())?;

    let identity_token: IdentityTokenJson = serde_json::from_str(&body)?;

    let identity = identity_token.identity.clone();

    let nickname = args.get_one::<String>("name").map(|s| s.clone());

    config.identity_configs.push(IdentityConfig {
        identity: identity_token.identity,
        token: identity_token.token,
        nickname: nickname.clone(),
        email: None,
    });
    if config.default_identity.is_none() {
        config.default_identity = Some(identity.clone());
    }
    config.save();
    println!(" Saved new identity");
    println!(" IDENTITY  {}", identity);
    println!(" NAME      {}", nickname.unwrap_or("".to_string()));

    Ok(())
}

async fn exec_rm(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let name = args.get_one::<String>("name");
    if let Some(name) = name {
        let index = config
            .identity_configs
            .iter()
            .position(|c| c.nickname.as_ref() == Some(name));
        if let Some(index) = index {
            let ic = config.identity_configs.remove(index);
            config.update_default_identity();
            config.save();
            println!(" Removed identity");
            println!(" IDENTITY  {}", ic.identity);
            println!(" NAME  {}", ic.nickname.unwrap_or("".to_string()));
        } else {
            println!("No such identity by that name.");
        }
        std::process::exit(0);
    }

    if let Some(identity) = args.get_one::<String>("identity") {
        let index = config.identity_configs.iter().position(|c| &c.identity == identity);
        if let Some(index) = index {
            let ic = config.identity_configs.remove(index);
            config.update_default_identity();
            config.save();
            println!(" Removed identity");
            println!(" IDENTITY  {}", ic.identity);
            println!(" NAME  {}", ic.nickname.unwrap_or("".to_string()));
        } else {
            println!("No such identity.");
        }
    }

    Ok(())
}

async fn exec_new(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let save = *args.get_one::<bool>("save").unwrap_or(&false);
    if save {
        let nickname = args.get_one::<String>("name").unwrap_or(&"".to_string()).clone();
        if config.name_exists(&nickname) {
            println!("An identity with that name already exists.");
            std::process::exit(0);
        }
    }

    let client = reqwest::Client::new();
    let mut builder = client.post(format!("http://{}/identity", config.host));

    if let Some(identity_token) = config.get_default_identity_config() {
        builder = builder.basic_auth("token", Some(identity_token.token.clone()));
    }

    let res = builder.send().await?;
    let res = res.error_for_status()?;

    let body = res.bytes().await?;
    let body = String::from_utf8(body.to_vec())?;

    let identity_token: IdentityTokenJson = serde_json::from_str(&body)?;

    let identity = identity_token.identity.clone();

    if save {
        let nickname = args.get_one::<String>("name").map(|s| s.clone());

        config.identity_configs.push(IdentityConfig {
            identity: identity_token.identity,
            token: identity_token.token,
            nickname: nickname.clone(),
            email: None,
        });
        if config.default_identity.is_none() {
            config.default_identity = Some(identity.clone());
        }
        config.save();
        println!(" Saved new identity");
        println!(" IDENTITY  {}", identity);
        println!(" NAME      {}", nickname.unwrap_or("".to_string()));
    } else {
        println!(" IDENTITY  {}", identity);
        println!(" TOKEN     {}", identity_token.token);
    }

    Ok(())
}

async fn exec_add(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let identity: String = args.get_one::<String>("identity").unwrap().clone();
    let token: String = args.get_one::<String>("token").unwrap().clone();

    //optional
    let nickname = args.get_one::<String>("name").map(|s| s.clone());
    let email: String = args.get_one::<String>("email").unwrap_or(&"".to_string()).clone();

    config.identity_configs.push(IdentityConfig {
        identity,
        token,
        nickname: nickname.clone(),
        email: Some(email.clone()),
    });

    config.save();

    println!(" New Identity Added");
    println!(" NAME      {}", nickname.unwrap_or("".to_string()));

    Ok(())
}

#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
struct LsRow {
    default: String,
    identity: String,
    name: String,
    email: String,
}

async fn exec_ls(config: Config, _args: &ArgMatches) -> Result<(), anyhow::Error> {
    let mut rows: Vec<LsRow> = Vec::new();
    for identity_token in config.identity_configs {
        let default_str = if config.default_identity.as_ref().unwrap() == &identity_token.identity {
            "***"
        } else {
            ""
        };
        rows.push(LsRow {
            default: default_str.to_string(),
            identity: identity_token.identity,
            name: identity_token.nickname.unwrap_or("".to_string()),
            email: identity_token.email.unwrap_or("".to_string()),
        });
    }
    let table = Table::new(&rows)
        .with(Style::empty())
        .with(Modify::new(Columns::first()).with(Alignment::right()));
    println!("{}", table.to_string());
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct GetIdentityResponse {
    identity: String,
    email: String,
}

async fn exec_find(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let email = args.get_one::<String>("email").unwrap().clone();

    let client = reqwest::Client::new();
    let builder = client.get(format!("http://{}/identity?email={}", config.host, email));

    // TODO: raise authorization error if this is not an identity we own
    let res = builder.send().await?;

    if res.status() == StatusCode::OK {
        let response: GetIdentityResponse = serde_json::from_slice(&res.bytes().await?[..])?;

        println!("Identity");
        println!(" IDENTITY  {}", response.identity);
        println!(" EMAIL     {}", response.email);
    } else if res.status() == StatusCode::NOT_FOUND {
        println!("Could not find identity for: {}", email)
    } else {
        println!("Error occurred in lookup: {}", res.status())
    }

    Ok(())
}

async fn exec_email(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let email = args.get_one::<String>("email").unwrap().clone();
    let identity = args.get_one::<String>("identity").unwrap().clone();

    let client = reqwest::Client::new();
    let mut builder = client.post(format!(
        "http://{}/identity/{}/set-email?email={}",
        config.host, identity, email
    ));

    if let Some(identity_token) = config.get_identity_config_by_identity(&identity) {
        builder = builder.basic_auth("token", Some(identity_token.token.clone()));
    } else {
        println!("Missing identity credentials for identity.");
        std::process::exit(0);
    }

    let res = builder.send().await?;
    res.error_for_status()?;

    let ic = config.get_identity_config_by_identity_mut(&identity).unwrap();
    ic.email = Some(email.clone());
    config.save();

    println!(" Associated email with identity");
    println!(" IDENTITY  {}", identity);
    println!(" EMAIL     {}", email);

    Ok(())
}

async fn exec_databases(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let identity = args.get_one::<String>("identity").unwrap().clone();

    let client = reqwest::Client::new();
    let mut builder = client.get(format!("http://{}/identity/{}/databases", config.host, identity));

    if let Some(identity_token) = config.get_identity_config_by_identity(&identity) {
        builder = builder.basic_auth("token", Some(identity_token.token.clone()));
    } else {
        println!("Missing identity credentials for identity.");
        std::process::exit(0);
    }

    let res = builder.send().await?;

    if res.status() != StatusCode::OK {
        println!("Unable to retrieve databases for identity: {}", res.status());
        return Ok(());
    }

    println!("Associated database addresses for identity:");
    println!("{}", res.text().await?);

    Ok(())
}
