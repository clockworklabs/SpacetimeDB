use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("rm")
        .about("Create a new SpacetimeDB account.")
        .override_usage("stdb rm <identity> <name>")
        .arg(Arg::new("identity").required(true))
        .arg(Arg::new("name").required(true))
        .after_help("Run `stdb help rm for more detailed information.\n`")
}

pub async fn exec(host: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let hex_identity = args.value_of("identity").unwrap();
    let name = args.value_of("name").unwrap();

    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{}/database/{}/{}/delete", host, hex_identity, name))
        .send()
        .await?;

    res.error_for_status()?;

    Ok(())
}
