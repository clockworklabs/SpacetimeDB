use clap::Arg;

use clap::ArgAction::SetTrue;
use clap::ArgMatches;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::util::get_auth_header;
use crate::util::init_default;

pub fn cli() -> clap::Command {
    clap::Command::new("publish")
        .about("Create and update a SpacetimeDB database.")
        .arg(
            Arg::new("host_type")
                .long("host-type")
                .short('t')
                .value_parser(["wasmer"])
                .default_value("wasmer"),
        )
        .arg(
            Arg::new("clear_database")
                .long("clear-database")
                .short('c')
                .required(true)
                .action(SetTrue),
        )
        .arg(
            Arg::new("path_to_project")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(".")
                .long("project-path")
                .short('p'),
        )
        .arg(
            Arg::new("trace_log")
                .long("trace_log")
                .help("Turn on diagnostic/performance tracing for this project")
                .required(false)
                .action(SetTrue),
        )
        // TODO(tyler): We should clean up setting an identity for a database in the future
        .arg(Arg::new("identity").long("identity").short('I').required(false))
        .arg(
            Arg::new("as_identity")
                .long("as-identity")
                .short('i')
                .required(false)
                .conflicts_with("anon_identity"),
        )
        .arg(
            Arg::new("anon_identity")
                .long("anon-identity")
                .short('a')
                .required(false)
                .conflicts_with("as_identity")
                .action(SetTrue),
        )
        .arg(
            Arg::new("use_cargo") // This flag is only used by the testsuite
                .long("use-cargo")
                .hide_long_help(true)
                .hide_short_help(true)
                .required(false)
                .action(SetTrue),
        )
        .arg(Arg::new("name|address").required(false))
        .after_help("Run `spacetime help publish` for more detailed information.")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InitDatabaseResponse {
    address: String,
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let identity = args.get_one::<String>("identity");
    let name_or_address = args.get_one::<String>("name|address");
    let path_to_project = args.get_one::<PathBuf>("path_to_project").unwrap();
    let host_type = args.get_one::<String>("host_type").unwrap();
    let clear_database = args.get_flag("clear_database");
    let trace_log = args.get_flag("trace_log");
    let use_cargo = args.get_flag("use_cargo");

    let as_identity = args.get_one::<String>("as_identity");
    let anon_identity = args.get_flag("anon_identity");

    let auth_header = get_auth_header(&mut config, anon_identity, as_identity.map(|x| x.as_str())).await;

    let mut url_args = String::new();

    // Identity is required
    if let Some(identity) = identity {
        let mut found = false;
        for identity_config in config.identity_configs.clone() {
            if identity_config.identity == identity.clone()
                || &identity_config.nickname.unwrap().to_string() == identity
            {
                found = true;
                break;
            }
        }

        if !found {
            return Err(anyhow::anyhow!(
                "Identity provided does not match any identity stored in your config file."
            ));
        }

        url_args.push_str(format!("?identity={}", identity).as_str());
    } else {
        let identity_config = init_default(&mut config, None).await?.identity_config;
        url_args.push_str(format!("?identity={}", identity_config.identity).as_str());
    }

    if let Some(name_or_address) = name_or_address {
        url_args.push_str(format!("&name_or_address={}", name_or_address).as_str());
    }

    if !path_to_project.exists() {
        return Err(anyhow::anyhow!(
            "Project path does not exist: {}",
            path_to_project.display()
        ));
    }

    if clear_database {
        url_args.push_str("&clear=true");
    }

    url_args.push_str(format!("&host_type={}", host_type).as_str());

    if trace_log {
        url_args.push_str("&trace_log=true");
    }

    let path_to_wasm = crate::tasks::pre_publish(path_to_project, use_cargo)?;

    let program_bytes = fs::read(path_to_wasm)?;

    let url = format!("{}/database/publish{}", config.get_host_url(), url_args);
    let client = reqwest::Client::new();
    let mut builder = client.post(url);
    if let Some(auth_header) = auth_header {
        builder = builder.header("Authorization", auth_header);
    }
    let res = builder.body(program_bytes).send().await?;
    let res = res.error_for_status()?;
    let bytes = res.bytes().await.unwrap();

    let response: InitDatabaseResponse = serde_json::from_slice(&bytes[..]).unwrap();
    println!("Created new database with address: {}", response.address);

    Ok(())
}
