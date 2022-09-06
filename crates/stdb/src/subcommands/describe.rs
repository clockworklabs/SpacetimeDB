use clap::Arg;
use clap::ArgMatches;

use crate::config::Config;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("describe")
        .about("Describe arguments of a SpacetimeDB function.")
        .override_usage("stdb describe <identity> <name> <reducer|table> <name>")
        .arg(Arg::new("identity").required(true))
        .arg(Arg::new("module_name").required(true))
        .arg(Arg::new("entity_type").required(true).value_parser(
            ["reducer", "table"]
        ))
        .arg(Arg::new("entity_name").required(true))
        .after_help("Run `stdb help call for more detailed information.\n`")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let hex_identity = args.value_of("identity").unwrap();
    let name = args.value_of("module_name").unwrap();
    let entity_name = args.value_of("entity_name").unwrap();
    let entity_type = format!("{}s", args.value_of("entity_type").unwrap());

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "http://{}/database/{}/{}/schema/{}/{}",
            config.host, hex_identity, name, entity_type, entity_name
        ))
        .send()
        .await?;

    let res = res.error_for_status()?;
    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);
    Ok(())
}
