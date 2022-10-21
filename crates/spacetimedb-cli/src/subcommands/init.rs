use clap::Arg;
use clap::ArgMatches;
use serde::Deserialize;
use serde::Serialize;
use std::fs;

use crate::config::Config;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("init")
        .about("Create a new SpacetimeDB database.")
        .arg(
            Arg::new("host_type")
                .takes_value(true)
                .required(false)
                .long("host_type")
                .short('t'),
        )
        .arg(Arg::new("force").long("force").short('f'))
        .arg(Arg::new("path to project").required(true))
        .arg(
            Arg::new("identity")
                .takes_value(true)
                .long("identity")
                .short('i')
                .required(false),
        )
        .arg(
            Arg::new("name")
                .takes_value(true)
                .long("name")
                .short('n')
                .required(false),
        )
        .after_help("Run `spacetime help init for more detailed information.\n`")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InitDatabaseResponse {
    address: String,
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let hex_identity = if let Some(identity) = args.value_of("identity") {
        identity.to_string()
    } else {
        let identity_config = config.get_default_identity_config().unwrap();
        identity_config.identity.to_string()
    };

    let name = args.value_of("name");

    let path_to_project = args.value_of("path to project").unwrap();
    let force = args.is_present("force");
    let host_type = args.value_of("host_type").unwrap_or("wasmer");
    let path = fs::canonicalize(path_to_project).unwrap();
    let program_bytes = fs::read(path)?;

    let url = if let Some(name) = name {
        format!(
            "http://{}/database/init?identity={}&name={}&force={}&host_type={}",
            config.host, hex_identity, name, force, host_type,
        )
    } else {
        format!(
            "http://{}/database/init?identity={}&force={}&host_type={}",
            config.host, hex_identity, force, host_type,
        )
    };

    let client = reqwest::Client::new();
    let res = client.post(url).body(program_bytes).send().await?;

    let res = res.error_for_status()?;
    let bytes = res.bytes().await.unwrap();

    let response: InitDatabaseResponse = serde_json::from_slice(&bytes[..]).unwrap();
    println!("Created new database with address: {}", response.address);

    Ok(())
}
