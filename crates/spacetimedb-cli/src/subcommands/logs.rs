use crate::config::Config;
use crate::util::spacetime_dns;
use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command {
    clap::Command::new("logs")
        .about("Prints logs from a SpacetimeDB database.")
        .arg(Arg::new("database").required(true))
        .arg(
            Arg::new("num_lines")
                .required(true)
                .value_parser(clap::value_parser!(u32)),
        )
        .after_help("Run `spacetime help logs` for more detailed information.\n")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let num_lines = args.get_one::<u32>("num_lines").unwrap();
    let database = args.get_one::<String>("database").unwrap();

    let address = if let Ok(address) = spacetime_dns(&config, database).await {
        address
    } else {
        database.to_string()
    };

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{}/database/logs/{}", config.host, address))
        .query(&[("num_lines", num_lines)])
        .send()
        .await?;

    let res = res.error_for_status()?;

    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);

    Ok(())
}
