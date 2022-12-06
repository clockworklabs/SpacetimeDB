use crate::config::Config;
use crate::util::{get_auth_header, spacetime_dns};
use clap::ArgAction::SetTrue;
use clap::ArgMatches;
use clap::{Arg, ArgAction};

pub fn cli() -> clap::Command {
    clap::Command::new("describe")
        .about("Describe the structure of a database or entities within it")
        .arg(Arg::new("database").required(true))
        .arg(
            Arg::new("entity_type")
                .required(false)
                .value_parser(["reducer", "table", "repeater"]),
        )
        .arg(Arg::new("entity_name").required(false).requires("entity_type"))
        .arg(Arg::new("brief").long("brief").short('b').action(SetTrue))
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
        .after_help("Run `spacetime help describe` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let expand = !args.get_flag("brief");
    let entity_name = args.get_one::<String>("entity_name");
    let entity_type = args.get_one::<String>("entity_type");

    let as_identity = args.get_one::<String>("as_identity");
    let anon_identity = args.get_flag("anon_identity");

    let auth_header = get_auth_header(&mut config, anon_identity, as_identity.map(|x| x.as_str())).await;

    let address = if let Ok(address) = spacetime_dns(&config, database).await {
        address
    } else {
        database.to_string()
    };

    let res = match entity_name {
        None => {
            let client = reqwest::Client::new();
            let mut builder = client.get(format!("http://{}/database/schema/{}", config.host, address));
            if let Some(auth_header) = auth_header {
                builder = builder.header("Authorization", auth_header);
            }
            builder.query(&[("expand", expand)]).send().await?
        }
        Some(entity_name) => {
            let entity_type = format!("{}s", entity_type.unwrap());

            let client = reqwest::Client::new();
            let mut builder = client.get(format!(
                "http://{}/database/schema/{}/{}/{}",
                config.host, address, entity_type, entity_name
            ));
            if let Some(auth_header) = auth_header {
                builder = builder.header("Authorization", auth_header);
            }
            builder.query(&[("expand", expand)]).send().await?
        }
    };

    let res = res.error_for_status()?;
    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);
    Ok(())
}
