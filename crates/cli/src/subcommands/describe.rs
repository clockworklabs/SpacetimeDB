use crate::common_args;
use crate::config::Config;
use crate::util::{database_identity, fetch_raw_module_schema};
use anyhow::Context;
use clap::{Arg, ArgMatches};
use spacetimedb_lib::sats;

pub fn cli() -> clap::Command {
    clap::Command::new("describe")
        .about("Describe the structure of a database or entities within it")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The name or identity of the database to describe"),
        )
        .arg(
            Arg::new("entity_type")
                .value_parser(clap::value_parser!(EntityType))
                .help("Whether to describe a reducer or table"),
        )
        .arg(
            Arg::new("entity_name")
                .requires("entity_type")
                .help("The name of the entity to describe"),
        )
        .arg(common_args::anonymous())
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
        .after_help("Run `spacetime help describe` for more detailed information.\n")
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum EntityType {
    Reducer,
    Table,
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let entity_name = args.get_one::<String>("entity_name");
    let entity_type = args.get_one::<EntityType>("entity_type");
    let entity = entity_type.zip(entity_name);
    let server = args.get_one::<String>("server").map(|s| s.as_ref());

    let anon_identity = args.get_flag("anon_identity");

    let database_identity = database_identity(&config, database, server).await?;

    let module_def = fetch_raw_module_schema(
        &reqwest::Client::new(),
        &config,
        database_identity,
        server,
        anon_identity,
    )
    .await?;
    fn sats_to_json<T: sats::Serialize>(v: &T) -> serde_json::Result<String> {
        serde_json::to_string_pretty(sats::serde::SerdeWrapper::from_ref(v))
    }
    let json = match entity {
        Some((EntityType::Reducer, reducer_name)) => {
            let reducer = module_def
                .reducers
                .iter()
                .find(|r| *r.name == **reducer_name)
                .context("no such reducer")?;
            sats_to_json(reducer)?
        }
        Some((EntityType::Table, table_name)) => {
            let table = module_def
                .tables
                .iter()
                .find(|t| *t.name == **table_name)
                .context("no such table")?;
            sats_to_json(table)?
        }
        None => sats_to_json(&module_def)?,
    };

    println!("{json}");

    Ok(())
}
