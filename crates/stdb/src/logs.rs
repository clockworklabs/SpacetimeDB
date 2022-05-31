use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("logs")
        .about("Prints logs from a SpacetimeDB database.")
        .override_usage("stdb logs -f <identity> <name> <num_lines>")
        .arg(Arg::new("identity").required(true))
        .arg(Arg::new("name").required(true))
        .arg(Arg::new("num_lines").required(true))
        .after_help("Run `stdb help logs for more detailed information.\n`")
}

pub async fn exec(args: &ArgMatches) -> Result<(), anyhow::Error> {
    let hex_identity = args.value_of("identity").unwrap();
    let name = args.value_of("name").unwrap();
    let num_lines = args.value_of("num_lines").unwrap();

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://localhost:3000/database/logs/{}/{}", hex_identity, name))
        .query(&[("num_lines", num_lines)])
        .send()
        .await?;

    let res = res.error_for_status()?;

    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);

    Ok(())
}
