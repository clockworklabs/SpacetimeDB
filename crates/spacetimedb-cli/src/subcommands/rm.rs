use crate::config::Config;
use crate::util::spacetime_dns;
use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("rm")
        .about("Create a new SpacetimeDB account.")
        .arg(Arg::new("database").required(true))
        .after_help("Run `stdb help rm for more detailed information.\n`")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.value_of("database").unwrap();
    let address = if let Ok(address) = spacetime_dns(&config, database).await {
        address
    } else {
        database.to_string()
    };

    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{}/database/delete/{}", config.host, address))
        .send()
        .await?;

    res.error_for_status()?;

    Ok(())
}
