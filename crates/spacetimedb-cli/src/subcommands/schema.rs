use clap::Arg;
use clap::ArgMatches;

use crate::config::Config;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("schema")
        .about("Describe the entire schema of an stdb module")
        .override_usage("stdb schema <identity> <name>")
        .arg(Arg::new("identity").required(true))
        .arg(Arg::new("module_name").required(true))
        .arg(Arg::new("expand").long("expand").short('e'))
        .after_help("Run `stdb help schema for more detailed information.\n`")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let hex_identity = args.value_of("identity").unwrap();
    let name = args.value_of("module_name").unwrap();
    let expand = args.is_present("expand");

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "http://{}/database/{}/{}/schema",
            config.host, hex_identity, name
        ))
        .query(&[("expand", expand)])
        .send()
        .await?;

    let res = res.error_for_status()?;
    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);
    Ok(())
}
