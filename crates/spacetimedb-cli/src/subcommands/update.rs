use crate::config::Config;
use crate::util::spacetime_dns;
use clap::Arg;
use clap::ArgMatches;
use std::fs;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("update")
        .about("Update a new SpacetimeDB actor.")
        .arg(Arg::new("database").required(true))
        .arg(Arg::new("path to project").required(true))
        .after_help("Run `stdb help init for more detailed information.\n`")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.value_of("database").unwrap();
    let address = if let Ok(address) = spacetime_dns(&config, database).await {
        address
    } else {
        database.to_string()
    };
    let path_to_project = args.value_of("path to project").unwrap();

    let path = fs::canonicalize(path_to_project).unwrap();
    let program_bytes = fs::read(path)?;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{}/database/update/{}", config.host, address))
        .body(program_bytes)
        .send()
        .await?;

    res.error_for_status()?;

    Ok(())
}
