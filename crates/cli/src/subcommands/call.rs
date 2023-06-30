use crate::config::Config;
use crate::util::{database_address, get_auth_header_only};
use anyhow::Error;
use clap::{Arg, ArgAction, ArgMatches};

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

    let address = database_address(&config, database).await?;

    let client = reqwest::Client::new();
    let mut builder = client.post(format!(
        "{}/database/call/{}/{}",
        config.get_host_url(),
        address,
        reducer_name
    ));
    if let Some(auth_header) = get_auth_header_only(&mut config, anon_identity, as_identity).await {
        builder = builder.header("Authorization", auth_header);
    }

    let res = builder.body(arg_json.to_owned()).send().await?;

    res.error_for_status()?;

    Ok(())
}
