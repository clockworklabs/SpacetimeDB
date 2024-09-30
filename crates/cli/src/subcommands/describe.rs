use crate::common_args;
use crate::config::Config;
use crate::util::{add_auth_header_opt, database_address, get_auth_header_only};
use clap::{Arg, ArgMatches};

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
                .long("type")
                .short('t')
                .value_parser(["reducer", "table"])
                .help("Whether to describe a reducer or table"),
        )
        .arg(
            Arg::new("entity_name")
                .requires("entity_type")
                .help("The name of the entity to describe"),
        )
        .arg(
            common_args::identity()
                .conflicts_with("anon_identity")
                .help("The identity to use to describe the entity")
                .long_help("The identity to use to describe the entity. If no identity is provided, the default one will be used."),
        )
        .arg(
            common_args::anonymous()
        )
        .arg(
            common_args::server()
                .help("The nickname, host name or URL of the server hosting the database"),
        )
        .after_help("Run `spacetime help describe` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let entity_name = args.get_one::<String>("entity_name");
    let entity_type = args.get_one::<String>("entity_type");
    let server = args.get_one::<String>("server").map(|s| s.as_ref());

    let identity = args.get_one::<String>("identity");
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
    let auth_header = get_auth_header_only(&mut config, anon_identity, identity, server).await?;
    let builder = add_auth_header_opt(builder, &auth_header);

    let descr = builder.send().await?.error_for_status()?.text().await?;
    println!("{}", descr);

    Ok(())
}
