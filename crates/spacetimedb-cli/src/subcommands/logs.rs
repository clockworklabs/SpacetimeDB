use crate::config::Config;
use crate::util::get_auth_header;
use crate::util::spacetime_dns;
use clap::Arg;
use clap::ArgAction;
use clap::ArgMatches;

pub fn cli() -> clap::Command {
    clap::Command::new("logs")
        .about("Prints logs from a SpacetimeDB database.")
        .arg(Arg::new("database").required(true))
        .arg(
            Arg::new("as_identity")
                .long("as-identity")
                .short('i')
                .required(false)
                .conflicts_with("anon_identity"),
        )
        .arg(
            Arg::new("anon_identity")
                .long("anon-identity")
                .short('a')
                .required(false)
                .conflicts_with("as_identity")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("num_lines")
                .required(false)
                .value_parser(clap::value_parser!(u32)),
        )
        .after_help("Run `spacetime help logs` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let num_lines = args.get_one::<u32>("num_lines");
    let database = args.get_one::<String>("database").unwrap();

    let as_identity = args.get_one::<String>("as_identity");
    let anon_identity = args.get_flag("anon_identity");

    let auth_header = get_auth_header(&mut config, anon_identity, as_identity.map(|x| x.as_str())).await;

    let address = if let Ok(address) = spacetime_dns(&config, database).await {
        address
    } else {
        database.to_string()
    };

    let mut query_parms = Vec::new();
    if num_lines.is_some() {
        query_parms.push(("num_lines", num_lines.unwrap()));
    }

    let client = reqwest::Client::new();
    let mut builder = client.get(format!("http://{}/database/logs/{}", config.host, address));
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
