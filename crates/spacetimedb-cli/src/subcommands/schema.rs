use clap::Arg;
use clap::ArgMatches;

use crate::config::Config;
use crate::util::spacetime_dns;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("schema")
        .about("Describe the entire schema of an stdb module")
        .arg(Arg::new("database").required(true))
        .arg(Arg::new("expand").long("expand").short('e'))
        .after_help("Run `stdb help schema for more detailed information.\n`")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.value_of("database").unwrap();
    let address = if let Ok(address) = spacetime_dns(&config, database).await {
        address
    } else {
        database.to_string()
    };
    let expand = args.is_present("expand");

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{}/database/schema/{}", config.host, address))
        .query(&[("expand", expand)])
        .send()
        .await?;

    let res = res.error_for_status()?;
    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);
    Ok(())
}
