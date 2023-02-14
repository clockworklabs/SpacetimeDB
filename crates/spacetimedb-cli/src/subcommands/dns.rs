use crate::config::Config;
use crate::util::{get_auth_header, spacetime_dns, spacetime_register_tld, spacetime_reverse_dns};
use clap::ArgMatches;
use clap::{Arg, Command};
use reqwest::Url;

use spacetimedb_lib::name::{parse_domain_name, DnsLookupResponse, InsertDomainResult, RegisterTldResult};

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
            .arg(Arg::new("identity").long("identity").short('i').help(
                "The identity that should own this tld. If no identity is specified, then the default identity is used",
            ))
            .after_help("Run `spacetime dns register-tld --help` for more detailed information.\n"),
        Command::new("lookup")
            .about("Resolves a domain to a database address")
            .arg(Arg::new("domain").required(true).help("The name of the domain to lookup"))
            .after_help("Run `spacetime dns lookup --help` for more detailed information"),
        Command::new("reverse-lookup")
            .about("Returns the domains for the provided database address")
            .arg(Arg::new("address").required(true).help("The address you would like to find all of the known domains for"))
            .after_help("Run `spacetime dns reverse-lookup --help` for more detailed information.\n"),
        Command::new("set-name")
            .about("Sets the domain of the database")
            .arg(Arg::new("domain").required(true).help("The domain you would like to assign or create"))
            .arg(Arg::new("address").required(true).help("The database address to assign to the domain"))
            .arg(Arg::new("identity").long("identity").short('i').long_help(
                "The identity that owns the tld for this domain. If no identity is specified, the default identity is used.",
            ).help("The identity that owns the tld for this domain"))
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
    let identity = args.get_one::<String>("identity");

    match spacetime_register_tld(&mut config, &tld, identity).await? {
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

    let response = spacetime_dns(&config, domain).await?;
    match response {
        DnsLookupResponse::Success { domain: _, address } => {
            println!("{}", address);
        }
        DnsLookupResponse::Failure { domain } => {
            println!("No such database: {}", domain);
        }
    }
    Ok(())
}

pub async fn exec_reverse_dns(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let addr = args.get_one::<String>("address").unwrap();
    let response = spacetime_reverse_dns(&config, addr).await?;
    if response.names.is_empty() {
        Err(anyhow::anyhow!("Could not find a name for the address: {}", addr))
    } else {
        for name in response.names {
            println!("{}", name);
        }
        Ok(())
    }
}

pub async fn exec_set_name(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let domain = args.get_one::<String>("domain").unwrap();
    let address = args.get_one::<String>("address").unwrap();
    let identity = args.get_one::<String>("identity");
    let (auth_header, _) = get_auth_header(&mut config, false, identity.map(|x| x.as_str()))
        .await
        .unwrap();

    let query_params = vec![
        ("domain", domain.clone()),
        ("address", address.clone()),
        ("register_tld", "true".to_string()),
    ];
    let builder = reqwest::Client::new()
        .get(Url::parse_with_params(
            format!("{}/database/set_name", config.get_host_url()).as_str(),
            query_params,
        )?)
        .header("Authorization", auth_header);

    let res = builder.send().await?.error_for_status()?;
    let bytes = res.bytes().await.unwrap();
    let result: InsertDomainResult = serde_json::from_slice(&bytes[..]).unwrap();
    match result {
        InsertDomainResult::Success { domain, address } => {
            println!("Domain set to {} for address {}.", domain, address);
        }
        InsertDomainResult::TldNotRegistered { domain } => {
            let parsed = parse_domain_name(domain.as_str())?;
            return Err(anyhow::anyhow!(
                "The top level domain that you provided is not registered.\n\
            This tld is not yet registered to any identity. You can register this domain with the following command:\n\
            \n\
            \tspacetime dns register-tld {}\n",
                parsed.tld
            ));
        }
        InsertDomainResult::PermissionDenied { domain } => {
            return match identity {
                Some(identity) => {
                    //TODO(jdetter): Have a nice name generator here, instead of using some abstract characters
                    // we should perhaps generate fun names like 'green-fire-dragon' instead
                    let suggested_tld: String = identity.chars().take(12).collect();
                    let parsed_domain = parse_domain_name(domain.as_str())?;
                    if let Some(sub_domain) = parsed_domain.sub_domain {
                        Err(anyhow::anyhow!(
                            "The top level domain {} is not registered to the identity you provided.\n\
                        We suggest you register a new tld:\n\
                        \tspacetime dns register-tld {}\n\
                        \n\
                        And then push to the domain that uses that tld:\n\
                        \tspacetime publish {}/{}\n",
                            parsed_domain.tld,
                            suggested_tld,
                            suggested_tld,
                            sub_domain
                        ))
                    } else {
                        Err(anyhow::anyhow!(
                            "The top level domain {} is not registered to the identity you provided.\n\
                        We suggest you register a new tld:\n\
                        \tspacetime dns register-tld {}\n\
                        \n\
                        And then push to the domain that uses that tld:\n\
                        \tspacetime publish {}\n",
                            parsed_domain.tld,
                            suggested_tld,
                            suggested_tld
                        ))
                    }
                }
                None => Err(anyhow::anyhow!(
                    "The domain {} is not registered to the identity you provided.",
                    domain
                )),
            };
        }
    }

    Ok(())
}
