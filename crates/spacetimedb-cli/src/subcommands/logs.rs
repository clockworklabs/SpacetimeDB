use crate::config::Config;
use crate::util::get_auth_header;
use crate::util::spacetime_dns;
use clap::Arg;
use clap::ArgMatches;
use spacetimedb_lib::name::{is_address, DnsLookupResponse};

pub fn cli() -> clap::Command {
    clap::Command::new("logs")
        .about("Prints logs from a SpacetimeDB database")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The domain or address of the database to print logs from"),
        )
        .arg(
            // TODO(jdetter): unify this with identity + name
            Arg::new("identity")
                .long("identity")
                .short('i')
                .help("The identity to use for printing logs from this database"),
        )
        .arg(
            Arg::new("num_lines")
                .value_parser(clap::value_parser!(u32))
                .help("The number of lines to print from the start of the log of this database")
                .long_help("The number of lines to print from the start of the log of this database. If no num lines is provided, all lines will be returned."),
        )
        .after_help("Run `spacetime help logs` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let num_lines = args.get_one::<u32>("num_lines");
    let database = args.get_one::<String>("database").unwrap();

    let identity = args.get_one::<String>("identity");

    let auth_header = get_auth_header(&mut config, false, identity.map(|x| x.as_str()))
        .await
        .map(|x| x.0);

    let address = if is_address(database.as_str()) {
        database.clone()
    } else {
        match spacetime_dns(&config, database).await? {
            DnsLookupResponse::Success { domain: _, address } => address,
            DnsLookupResponse::Failure { domain } => {
                return Err(anyhow::anyhow!("The dns resolution of {} failed.", domain));
            }
        }
    };

    let mut query_parms = Vec::new();
    if num_lines.is_some() {
        query_parms.push(("num_lines", num_lines.unwrap()));
    }

    let client = reqwest::Client::new();
    let mut builder = client.get(format!("{}/database/logs/{}", config.get_host_url(), address));
    if let Some(auth_header) = auth_header {
        builder = builder.header("Authorization", auth_header);
    }
    let res = builder.query(&query_parms).send().await?;

    let res = res.error_for_status()?;

    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);

    Ok(())
}
