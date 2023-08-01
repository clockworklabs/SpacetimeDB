use crate::config::Config;
use crate::util::get_auth_header;
use crate::util::spacetime_dns;
use anyhow::Error;
use clap::Arg;
use clap::ArgAction;
use clap::ArgMatches;
use spacetimedb_lib::name::{is_address, DnsLookupResponse};

pub fn cli() -> clap::Command {
    clap::Command::new("call")
        .about("Invokes a reducer function in a database")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The database domain or address to use to invoke the call"),
        )
        .arg(
            Arg::new("reducer_name")
                .required(true)
                .help("The name of the reducer to call"),
        )
        .arg(
            Arg::new("arguments")
                .help("arguments as a JSON array")
                .default_value("[]"),
        )
        .arg(
            Arg::new("as_identity")
                .long("as-identity")
                .short('i')
                .conflicts_with("anon_identity")
                .help("The identity to use for the call"),
        )
        .arg(
            Arg::new("anon_identity")
                .long("anon-identity")
                .short('a')
                .conflicts_with("as_identity")
                .action(ArgAction::SetTrue)
                .help("If this flag is present, the call will be executed with no identity provided"),
        )
        .after_help("Run `spacetime help call` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), Error> {
    let database = args.get_one::<String>("database").unwrap();
    let reducer_name = args.get_one::<String>("reducer_name").unwrap();
    let arg_json = args.get_one::<String>("arguments").unwrap();

    let as_identity = args.get_one::<String>("as_identity");
    let anon_identity = args.get_flag("anon_identity");

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

    let client = reqwest::Client::new();
    let mut builder = client.post(format!(
        "{}/database/call/{}/{}",
        config.get_host_url(),
        address,
        reducer_name
    ));
    if let Some((auth_header, _)) = get_auth_header(&mut config, anon_identity, as_identity.map(|x| x.as_str())).await {
        builder = builder.header("Authorization", auth_header);
    }

    let res = builder.body(arg_json.to_owned()).send().await?;

    res.error_for_status()?;

    Ok(())
}
