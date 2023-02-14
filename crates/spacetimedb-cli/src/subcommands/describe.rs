use crate::config::Config;
use crate::util::{get_auth_header, spacetime_dns};
use clap::Arg;
use clap::ArgAction::SetTrue;
use clap::ArgMatches;
use spacetimedb_lib::name::{is_address, DnsLookupResponse};

pub fn cli() -> clap::Command {
    clap::Command::new("describe")
        .about("Describe the structure of a database or entities within it")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The domain or address of the database to describe"),
        )
        .arg(
            Arg::new("entity_type")
                .value_parser(["reducer", "table"])
                .help("Whether to describe a reducer or table"),
        )
        .arg(
            Arg::new("entity_name")
                .requires("entity_type")
                .help("The name of the entity to describe"),
        )
        .arg(Arg::new("brief").long("brief").short('b').action(SetTrue)
            .help("If this flag is present, a brief description shall be returned"))
        .arg(
            Arg::new("as_identity")
                .long("as-identity")
                .short('i')
                .conflicts_with("anon_identity")
                .help("The identity to use to describe the entity")
                .long_help("The identity to use to describe the entity. If no identity is provided, the default one will be used."),
        )
        .arg(
            Arg::new("anon_identity")
                .long("anon-identity")
                .short('a')
                .conflicts_with("as_identity")
                .action(SetTrue)
                .help("If this flag is present, no identity will be provided when describing the database"),
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

    let auth_header = get_auth_header(&mut config, anon_identity, as_identity.map(|x| x.as_str()))
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

    let res = match entity_name {
        None => {
            let client = reqwest::Client::new();
            let mut builder = client.get(format!("{}/database/schema/{}", config.get_host_url(), address));
            if let Some(auth_header) = auth_header {
                builder = builder.header("Authorization", auth_header);
            }
            builder.query(&[("expand", expand)]).send().await?
        }
        Some(entity_name) => {
            let entity_type = format!("{}s", entity_type.unwrap());

            let client = reqwest::Client::new();
            let mut builder = client.get(format!(
                "{}/database/schema/{}/{}/{}",
                config.get_host_url(),
                address,
                entity_type,
                entity_name
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
