use crate::common_args;
use crate::config::Config;
use crate::util::{
    add_auth_header_opt, decode_identity, get_auth_header, spacetime_dns, spacetime_register_tld, spacetime_reverse_dns,
};
use clap::ArgMatches;
use clap::{Arg, Command};
use reqwest::Url;

use spacetimedb_client_api_messages::name::{DnsLookupResponse, InsertDomainResult, RegisterTldResult};

pub fn cli() -> Command {
    Command::new("dns")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_subcommands())
        .about("Create, manage and query domains")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    exec_subcommand(config, cmd, subcommand_args).await
}

fn get_subcommands() -> Vec<Command> {
    vec![
        Command::new("register-tld")
            .about("Registers a new top level domain")
            .arg(
                Arg::new("tld")
                    .required(true)
                    .help("The top level domain that you would like to register"),
            )
            .arg(
                common_args::server()
                    .help("The nickname, host name or URL of the server on which to register the domain"),
            )
            .after_help("Run `spacetime dns register-tld --help` for more detailed information.\n"),
        Command::new("lookup")
            .about("Resolves a domain to a database identity")
            .arg(
                Arg::new("domain")
                    .required(true)
                    .help("The name of the domain to lookup"),
            )
            .arg(
                common_args::server()
                    .help("The nickname, host name or URL of the server on which to look up the domain name"),
            )
            .after_help("Run `spacetime dns lookup --help` for more detailed information"),
        Command::new("reverse-lookup")
            .about("Returns the domains for the provided database identity")
            .arg(
                Arg::new("database-identity")
                    .required(true)
                    .help("The database identity you would like to find all of the known domains for"),
            )
            .arg(
                common_args::server()
                    .help("The nickname, host name or URL of the server on which to look up the database identity"),
            )
            .after_help("Run `spacetime dns reverse-lookup --help` for more detailed information.\n"),
        Command::new("set-name")
            .about("Sets the domain of the database")
            .arg(
                Arg::new("domain")
                    .required(true)
                    .help("The domain you would like to assign or create"),
            )
            .arg(
                Arg::new("database-identity")
                    .required(true)
                    .help("The database identity to assign to the domain"),
            )
            .arg(common_args::server().help("The nickname, host name or URL of the server on which to set the name"))
            .after_help("Run `spacetime dns set-name --help` for more detailed information.\n"),
    ]
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "register-tld" => exec_register_tld(config, args).await,
        "lookup" => exec_dns_lookup(config, args).await,
        "reverse-lookup" => exec_reverse_dns(config, args).await,
        "set-name" => exec_set_name(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

async fn exec_register_tld(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let tld = args.get_one::<String>("tld").unwrap().clone();
    let server = args.get_one::<String>("server").map(|s| s.as_ref());

    match spacetime_register_tld(&mut config, &tld, server).await? {
        RegisterTldResult::Success { domain } => {
            println!("Registered domain: {}", domain);
        }
        RegisterTldResult::Unauthorized { domain } => {
            return Err(anyhow::anyhow!("Domain is already registered by another: {}", domain));
        }
        RegisterTldResult::AlreadyRegistered { domain } => {
            println!("Domain is already registered by the identity you provided: {}", domain);
        }
    }
    config.save();

    Ok(())
}

pub async fn exec_dns_lookup(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let domain = args.get_one::<String>("domain").unwrap();
    let server = args.get_one::<String>("server").map(|s| s.as_ref());

    let response = spacetime_dns(&config, domain, server).await?;
    match response {
        DnsLookupResponse::Success { domain: _, identity } => {
            println!("{}", identity);
        }
        DnsLookupResponse::Failure { domain } => {
            println!("No such database: {}", domain);
        }
    }
    Ok(())
}

pub async fn exec_reverse_dns(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database_identity = args.get_one::<String>("database-identity").unwrap();
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let response = spacetime_reverse_dns(&config, database_identity, server).await?;
    if response.names.is_empty() {
        Err(anyhow::anyhow!(
            "Could not find a name for the identity: {}",
            database_identity
        ))
    } else {
        for name in response.names {
            println!("{}", name);
        }
        Ok(())
    }
}

pub async fn exec_set_name(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let domain = args.get_one::<String>("domain").unwrap();
    let database_identity = args.get_one::<String>("database-identity").unwrap();
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let identity = decode_identity(&config)?;
    let auth_header = get_auth_header(&config, false)?;

    let builder = reqwest::Client::new().get(Url::parse_with_params(
        format!("{}/database/set_name", config.get_host_url(server)?).as_str(),
        [
            ("domain", domain.clone()),
            ("database_identity", database_identity.clone()),
            ("register_tld", "true".to_string()),
        ],
    )?);
    let builder = add_auth_header_opt(builder, &auth_header);

    let res = builder.send().await?.error_for_status()?;
    let bytes = res.bytes().await.unwrap();
    println!("{}", String::from_utf8_lossy(&bytes[..]));
    let result: InsertDomainResult = serde_json::from_slice(&bytes[..]).unwrap();
    match result {
        InsertDomainResult::Success {
            domain,
            database_identity,
        } => {
            println!("Domain set to {} for identity {}.", domain, database_identity);
        }
        InsertDomainResult::TldNotRegistered { domain } => {
            return Err(anyhow::anyhow!(
                "The top level domain that you provided is not registered.\n\
            This tld is not yet registered to any identity. You can register this domain with the following command:\n\
            \n\
            \tspacetime dns register-tld {}\n",
                domain.tld()
            ));
        }
        InsertDomainResult::PermissionDenied { domain } => {
            //TODO(jdetter): Have a nice name generator here, instead of using some abstract characters
            // we should perhaps generate fun names like 'green-fire-dragon' instead
            let suggested_tld: String = identity.chars().take(12).collect();
            if let Some(sub_domain) = domain.sub_domain() {
                return Err(anyhow::anyhow!(
                    "The top level domain {} is not registered to the identity you provided.\n\
                We suggest you register a new tld:\n\
                \tspacetime dns register-tld {}\n\
                \n\
                And then push to the domain that uses that tld:\n\
                \tspacetime publish {}/{}\n",
                    domain.tld(),
                    suggested_tld,
                    suggested_tld,
                    sub_domain
                ));
            } else {
                return Err(anyhow::anyhow!(
                    "The top level domain {} is not registered to the identity you provided.\n\
                We suggest you register a new tld:\n\
                \tspacetime dns register-tld {}\n\
                \n\
                And then push to the domain that uses that tld:\n\
                \tspacetime publish {}\n",
                    domain.tld(),
                    suggested_tld,
                    suggested_tld
                ));
            }
        }
        InsertDomainResult::OtherError(e) => return Err(anyhow::anyhow!(e)),
    }

    Ok(())
}
