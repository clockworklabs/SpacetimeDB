use crate::config::Config;
use crate::util::{add_auth_header_opt, database_address, get_auth_header_only};
use clap::{Arg, ArgAction::SetTrue, ArgMatches};

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
        .arg(
            Arg::new("server")
                .long("server")
                .short('s')
                .help("The nickname, host name or URL of the server hosting the database"),
        )
        .after_help("Run `spacetime help describe` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let expand = !args.get_flag("brief");
    let entity_name = args.get_one::<String>("entity_name");
    let entity_type = args.get_one::<String>("entity_type");
    let server = args.get_one::<String>("server").map(|s| s.as_ref());

    let as_identity = args.get_one::<String>("as_identity");
    let anon_identity = args.get_flag("anon_identity");

    let address = database_address(&config, database, server).await?;

    let builder = reqwest::Client::new().get(match entity_name {
        None => format!("{}/database/schema/{}", config.get_host_url(server)?, address),
        Some(entity_name) => format!(
            "{}/database/schema/{}/{}/{}",
            config.get_host_url(server)?,
            address,
            entity_type.unwrap(),
            entity_name
        ),
    });
    let auth_header = get_auth_header_only(&mut config, anon_identity, as_identity, server).await?;
    let builder = add_auth_header_opt(builder, &auth_header);

    let descr = builder
        .query(&[("expand", expand)])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    println!("{}", descr);

    Ok(())
}
