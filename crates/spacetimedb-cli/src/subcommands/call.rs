use clap::Arg;
use clap::ArgMatches;

use crate::config::Config;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("call")
        .about("Invokes a SpacetimeDB function.")
        .override_usage("stdb call <identity> <name> <function name> <function params as json>")
        .arg(Arg::new("identity").required(true))
        .arg(Arg::new("name").required(true))
        .arg(Arg::new("function_name").required(true))
        .arg(Arg::new("arg_json").required(true))
        .after_help("Run `stdb help call for more detailed information.\n`")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let hex_identity = args.value_of("identity").unwrap();
    let name = args.value_of("name").unwrap();
    let function_name = args.value_of("function_name").unwrap();
    let arg_json = args.value_of("arg_json").unwrap_or("{}");

    let client = reqwest::Client::new();
    let res = client
        .post(format!(
            "http://{}/database/{}/{}/call/{}",
            config.host, hex_identity, name, function_name
        ))
        .body(arg_json.to_owned())
        .send()
        .await?;

    res.error_for_status()?;

    Ok(())
}
