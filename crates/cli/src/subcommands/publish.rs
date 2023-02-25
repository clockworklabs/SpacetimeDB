use clap::Arg;

use clap::ArgAction::SetTrue;
use clap::ArgMatches;
use reqwest::Url;
use spacetimedb_lib::name::{is_address, parse_domain_name, PublishResult};
use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::util::get_auth_header;
use crate::util::init_default;

pub fn cli() -> clap::Command {
    clap::Command::new("publish")
        .about("Create and update a SpacetimeDB database")
        .arg(
            Arg::new("host_type")
                .long("host-type")
                .short('t')
                .value_parser(["wasmer"])
                .default_value("wasmer")
                .help("The type of host that should be for hosting this module"),
        )
        .arg(
            // TODO(jdetter): Rename this to --delete-tables (clear doesn't really implies the tables are being dropped)
            Arg::new("clear_database")
                .long("clear-database")
                .short('c')
                .required(true)
                .action(SetTrue)
                .help("When publishing a new module to an existing address, also delete all tables associated with the database"),
        )
        .arg(
            Arg::new("path_to_project")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(".")
                .long("project-path")
                .short('p')
                .help("The system path (absolute or relative) to the module project")
        )
        .arg(
            Arg::new("trace_log")
                .long("trace_log")
                .help("Turn on diagnostic/performance tracing for this project")
                .action(SetTrue),
        )
        // TODO(tyler): We should be able to pass in either an identity or an alias here
        // TODO(jdetter): Unify identity + identity alias
        .arg(
            Arg::new("identity")
                .long("identity")
                .short('I')
                .help("The identity that should own the database")
                .long_help("The identity that should own the database. If no identity is provided, your default identity will be used."))
        // TODO(jdetter): add this back in when we actually support this
        // .arg(
        //     Arg::new("as_identity")
        //         .long("as-identity")
        //         .short('i')
        //         .required(false)
        //         .conflicts_with("anon_identity"),
        // )
        .arg(
            Arg::new("anon_identity")
                .long("anon-identity")
                .short('a')
                .action(SetTrue)
                .help("Instruct SpacetimeDB to allocate a new identity to own this database"),
        )
        .arg(
            Arg::new("name|address")
                .help("A valid domain or address for this database"),
        )
        .after_help("Run `spacetime help publish` for more detailed information.")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let identity = args.get_one::<String>("identity");
    let name_or_address = args.get_one::<String>("name|address");
    let path_to_project = args.get_one::<PathBuf>("path_to_project").unwrap();
    let host_type = args.get_one::<String>("host_type").unwrap();
    let clear_database = args.get_flag("clear_database");
    let trace_log = args.get_flag("trace_log");
    let anon_identity = args.get_flag("anon_identity");

    let mut query_params = Vec::<(&str, &str)>::new();
    query_params.push(("host_type", host_type.as_str()));
    query_params.push(("register_tld", "true"));

    // If a domain or address was provided, we should locally make sure it looks correct and
    // append it as a query parameter
    if let Some(name_or_address) = name_or_address {
        if !is_address(name_or_address) {
            parse_domain_name(name_or_address)?;
        }
        query_params.push(("name_or_address", name_or_address.as_str()));
    }

    if !path_to_project.exists() {
        return Err(anyhow::anyhow!(
            "Project path does not exist: {}",
            path_to_project.display()
        ));
    }

    if clear_database {
        query_params.push(("clear", "true"));
    }

    if trace_log {
        query_params.push(("trace_log", "true"));
    }

    let client = reqwest::Client::new();

    let path_to_wasm = crate::tasks::build(path_to_project)?;
    let program_bytes = fs::read(path_to_wasm)?;

    let mut builder = client.post(Url::parse_with_params(
        format!("{}/database/publish", config.get_host_url()).as_str(),
        query_params,
    )?);

    // If the user didn't specify an identity and we didn't specify an anonymous identity, then
    // we want to use the default identity
    // TODO(jdetter): We should maybe have some sort of user prompt here for them to be able to
    //  easily create a new identity with an email
    let identity = if !anon_identity {
        if identity.is_none() && config.default_identity().is_none() {
            init_default(&mut config, None).await?;
        }

        if let Some((auth_header, chosen_identity)) =
            get_auth_header(&mut config, anon_identity, identity.map(|x| x.as_str())).await
        {
            builder = builder.header("Authorization", auth_header);
            Some(chosen_identity)
        } else {
            None
        }
    } else {
        None
    };

    let res = builder.body(program_bytes).send().await?;
    let res = res.error_for_status()?;
    let bytes = res.bytes().await.unwrap();

    let response: PublishResult = serde_json::from_slice(&bytes[..]).unwrap();
    match response {
        PublishResult::Success { domain, address } => {
            if let Some(domain) = domain {
                println!("Created new database with domain: {} address: {}", domain, address);
            } else {
                println!("Created new database with address: {}", address);
            }
        }
        PublishResult::TldNotRegistered { domain } => {
            let parsed = parse_domain_name(domain.as_str())?;
            return Err(anyhow::anyhow!(
                "The top level domain that you provided is not registered.\n\
            This tld is not yet registered to any identity. You can register this domain with the following command:\n\
            \n\
            \tspacetime dns register-tld {}\n",
                parsed.tld
            ));
        }
        PublishResult::PermissionDenied { domain } => {
            return match identity {
                Some(identity) => {
                    //TODO(jdetter): Have a nice name generator here, instead of using some abstract characters
                    // we should perhaps generate fun names like 'green-fire-dragon' instead
                    let suggested_tld: String = identity.to_hex().chars().take(12).collect();
                    let parsed_domain = parse_domain_name(domain.as_str())?;
                    if let Some(sub_domain) = parsed_domain.sub_domain {
                        Err(anyhow::anyhow!(
                            "The top level domain {} is not registered to the identity you provided.\n\
                        We suggest you publish to a domain that starts with a TLD owned by you, or publish to a new domain like:\n\
                        \tspacetime publish {}/{}\n",
                            parsed_domain.tld,
                            suggested_tld,
                            sub_domain
                        ))
                    } else {
                        Err(anyhow::anyhow!(
                            "The top level domain {} is not registered to the identity you provided.\n\
                        We suggest you push to either a domain owned by you, or a new domain like:\n\
                        \tspacetime publish {}\n",
                            parsed_domain.tld,
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
