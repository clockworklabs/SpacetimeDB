use clap::Arg;
use clap::ArgAction::SetTrue;
use clap::ArgMatches;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::util;
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
                .action(SetTrue),
        )
        .arg(
            Arg::new("path_to_project")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(".")
                .long("project-path")
                .short('p'),
        )
        .arg(Arg::new("identity").long("identity").short('i').required(false))
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

    let mut url_args = String::new();

    // Identity is required
    if let Some(identity) = identity {
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

    crate::tasks::pre_publish(path_to_project)?;

    let path_to_wasm = util::find_wasm_file(path_to_project)?;
    let program_bytes = fs::read(fs::canonicalize(path_to_wasm).unwrap())?;

    let url = format!("http://{}/database/publish{}", config.host, url_args);
    let client = reqwest::Client::new();
    let res = client.post(url).body(program_bytes).send().await?;
    let res = res.error_for_status()?;
    let bytes = res.bytes().await.unwrap();

    let response: InitDatabaseResponse = serde_json::from_slice(&bytes[..]).unwrap();
    println!("Created new database with address: {}", response.address);

    Ok(())
}
