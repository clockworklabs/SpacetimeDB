use crate::config::Config;
use crate::util::get_auth_header;
use crate::util::spacetime_dns;
use clap::Arg;
use clap::ArgAction;
use clap::ArgMatches;

pub fn cli() -> clap::Command {
    clap::Command::new("call")
        .about("Invokes a reducer function in a database")
        .arg(Arg::new("database").required(true))
        .arg(Arg::new("function_name").required(true))
        .arg(
            Arg::new("arguments")
                .required(false)
                .help("arguments as a JSON array")
                .default_value("[]"),
        )
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
        .after_help("Run `spacetime help call` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let function_name = args.get_one::<String>("function_name").unwrap();
    let arg_json = args.get_one::<String>("arguments").unwrap();

    let as_identity = args.get_one::<String>("as_identity");
    let anon_identity = args.get_flag("anon_identity");

    let auth_header = get_auth_header(&mut config, anon_identity, as_identity.map(|x| x.as_str())).await;

    let address = if let Ok(address) = spacetime_dns(&config, database).await {
        address
    } else {
        database.to_string()
    };

    let client = reqwest::Client::new();
    let mut builder = client.post(format!(
        "{}/database/call/{}/{}",
        config.get_host_url(),
        address,
        function_name
    ));
    if let Some(auth_header) = auth_header {
        builder = builder.header("Authorization", auth_header);
    }
    let res = builder.body(arg_json.to_owned()).send().await?;

    res.error_for_status()?;

    Ok(())
}
