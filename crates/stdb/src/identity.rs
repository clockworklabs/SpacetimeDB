use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("identity")
        .about("Create a new SpacetimeDB identity.")
        .override_usage("stdb identity")
        .after_help("Run `stdb help identity for more detailed information.\n`")
}

pub async fn exec(host: &str, _args: &ArgMatches) -> Result<(), anyhow::Error> {
    let client = reqwest::Client::new();
    let res = client.get(format!("http://{}/identity", host)).send().await?;

    let res = res.error_for_status()?;

    let body = res.bytes().await?;
    let body = String::from_utf8(body.to_vec())?;

    println!("{}", body);

    Ok(())
}
