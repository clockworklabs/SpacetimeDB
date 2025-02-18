use crate::api::ClientApi;
use crate::common_args;
use crate::config::Config;
use crate::sql::parse_req;
use crate::util::UNSTABLE_WARNING;
use anyhow::Context;
use clap::{Arg, ArgAction, ArgMatches};
use spacetimedb_lib::sats;

pub fn cli() -> clap::Command {
    clap::Command::new("describe")
        .about(format!(
            "Describe the structure of a database or entities within it. {}",
            UNSTABLE_WARNING
        ))
        .arg(
            Arg::new("database")
                .required(true)
                .help("The name or identity of the database to describe"),
        )
        .arg(
            Arg::new("entity_type")
                .value_parser(clap::value_parser!(EntityType))
                .requires("entity_name")
                .help("Whether to describe a reducer or table"),
        )
        .arg(
            Arg::new("entity_name")
                .requires("entity_type")
                .help("The name of the entity to describe"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                // make not required() once we have a human readable output
                .required(true)
                .help(
                    "Output the schema in JSON format. Currently required; in the future, omitting this will \
                     give human-readable output.",
                ),
        )
        .arg(common_args::anonymous())
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
        .arg(common_args::yes())
        .after_help("Run `spacetime help describe` for more detailed information.\n")
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum EntityType {
    Reducer,
    Table,
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    eprintln!("{}\n", UNSTABLE_WARNING);

    let entity_name = args.get_one::<String>("entity_name");
    let entity_type = args.get_one::<EntityType>("entity_type");
    let entity = entity_type.zip(entity_name);
    let json = args.get_flag("json");

    let conn = parse_req(config, args).await?;
    let api = ClientApi::new(conn);

    let module_def = api.module_def().await?;

    if json {
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
    } else {
        // TODO: human-readable API
    }

    Ok(())
}
