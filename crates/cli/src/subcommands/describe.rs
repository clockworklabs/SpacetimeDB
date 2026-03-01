use crate::api::ClientApi;
use crate::common_args;
use crate::config::Config;
use crate::subcommands::db_arg_resolution::{load_config_db_targets, resolve_database_with_optional_parts};
use crate::util::UNSTABLE_WARNING;
use crate::util::{database_identity, get_auth_header};
use anyhow::Context;
use clap::{Arg, ArgAction, ArgMatches};
use spacetimedb_lib::sats;

pub fn cli() -> clap::Command {
    clap::Command::new("describe")
        .about(format!(
            "Describe the structure of a database or entities within it. {UNSTABLE_WARNING}"
        ))
        .arg(
            Arg::new("describe_parts")
                .num_args(0..)
                .help("Describe arguments: [DATABASE] [ENTITY_TYPE ENTITY_NAME]"),
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
        .arg(
            Arg::new("no_config")
                .long("no-config")
                .action(ArgAction::SetTrue)
                .help("Ignore spacetime.json configuration"),
        )
        .after_help("Run `spacetime help describe` for more detailed information.\n")
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum EntityType {
    Reducer,
    Table,
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    eprintln!("{UNSTABLE_WARNING}\n");

    let json = args.get_flag("json");
    let no_config = args.get_flag("no_config");
    let raw_parts: Vec<String> = args
        .get_many::<String>("describe_parts")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();
    let config_targets = load_config_db_targets(no_config)?;
    let resolved = resolve_database_with_optional_parts(
        &raw_parts,
        config_targets.as_deref(),
        "spacetime describe [database] [entity_type entity_name] --json [--no-config]",
    )?;
    let entity = match resolved.remaining_args.as_slice() {
        [] => None,
        [entity_type, entity_name] => {
            let entity_type = match entity_type.as_str() {
                "reducer" => EntityType::Reducer,
                "table" => EntityType::Table,
                _ => {
                    anyhow::bail!(
                        "Invalid entity_type '{}'. Expected one of: reducer, table.",
                        entity_type
                    )
                }
            };
            Some((entity_type, entity_name.as_str()))
        }
        _ => {
            anyhow::bail!(
                "Invalid describe arguments.\nUsage: spacetime describe [database] [entity_type entity_name] --json [--no-config]"
            );
        }
    };

    let mut config = config;
    let server_from_cli = args.get_one::<String>("server").map(|s| s.as_ref());
    let server = server_from_cli.or(resolved.server.as_deref());
    let force = args.get_flag("force");
    let anon_identity = args.get_flag("anon_identity");
    let conn = crate::api::Connection {
        host: config.get_host_url(server)?,
        auth_header: get_auth_header(&mut config, anon_identity, server, !force).await?,
        database_identity: database_identity(&config, &resolved.database, server).await?,
        database: resolved.database,
    };
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
                    .find(|r| *r.name == *reducer_name)
                    .context("no such reducer")?;
                sats_to_json(reducer)?
            }
            Some((EntityType::Table, table_name)) => {
                let table = module_def
                    .tables
                    .iter()
                    .find(|t| *t.name == *table_name)
                    .context("no such table")?;
                sats_to_json(table)?
            }
            None => sats_to_json(&module_def)?,
        };

        // TODO: validate the JSON output
        println!("{json}");
    } else {
        // TODO: human-readable API
    }

    Ok(())
}
