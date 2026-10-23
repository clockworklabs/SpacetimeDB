use crate::api::ClientApi;
use crate::common_args::{self, Format};
use crate::config::Config;
use crate::subcommands::db_arg_resolution::{load_config_db_targets, resolve_database_with_optional_parts};
use crate::subcommands::publish::pretty_print_style_from_env;
use crate::util::UNSTABLE_WARNING;
use crate::util::{database_identity, get_auth_header};
use anyhow::Context;
use clap::{Arg, ArgAction, ArgMatches};
use spacetimedb_client_api_messages::name::PrettyPrintStyle as EnvStyle;
use spacetimedb_lib::db::raw_def::v10::{
    RawHttpRouteDefV10, RawProcedureDefV10, RawReducerDefV10, RawTableDefV10, RawTypeDefV10, RawViewDefV10,
};
use spacetimedb_lib::environment::EnvironmentDeclaration;
use spacetimedb_lib::sats;
use spacetimedb_schema::auto_migrate::PrettyPrintStyle;
use spacetimedb_schema::def::{HttpRouteDef, ModuleDef};
use spacetimedb_schema::describe::{
    all_named_types, describe_env_var, describe_env_vars, describe_http_route, describe_http_routes, describe_module,
    describe_procedure, describe_procedures, describe_reducer, describe_reducers, describe_table, describe_tables,
    describe_type, describe_types, describe_view, describe_views, sorted_procedures, sorted_reducers, sorted_tables,
    sorted_types, sorted_views,
};
use std::io::IsTerminal;

const USAGE: &str = "spacetime describe [database] [entity_type [entity_name]] [--format text|json] [--no-config]";

pub fn cli() -> clap::Command {
    clap::Command::new("describe")
        .about(format!(
            "Describe the structure of a database or entities within it. {UNSTABLE_WARNING}"
        ))
        .arg(Arg::new("describe_parts").num_args(0..).help(
            "Describe arguments: [DATABASE] [ENTITY_TYPE [ENTITY_NAME]]. \
                     ENTITY_TYPE is one of tables, views, reducers, procedures, routes, env or types. \
                     A route is named by its path, and an environment variable by its key.",
        ))
        .arg(common_args::format().help("Output format for the schema"))
        .arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .conflicts_with("format")
                .help("Output the schema in JSON format. Shorthand for `--format json`."),
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

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
enum EntityType {
    EnvVar,
    HttpRoute,
    Procedure,
    Reducer,
    Table,
    Type,
    View,
}

/// What to describe: the whole module, every entity of one type, or a single named entity.
#[derive(Debug, PartialEq)]
enum Selection<'a> {
    Module,
    All(EntityType),
    One(EntityType, &'a str),
}

/// Parses an entity type. The plural is canonical, matching the section headings of the module
/// output, but the singular is accepted too. Environment variables are `env`, as in `ctx.env` and
/// `spacetime publish --env-only`, with `environment` accepted too.
fn parse_entity_type(entity_type: &str) -> anyhow::Result<EntityType> {
    match entity_type {
        "env" | "environment" => Ok(EntityType::EnvVar),
        "routes" | "route" => Ok(EntityType::HttpRoute),
        "procedures" | "procedure" => Ok(EntityType::Procedure),
        "reducers" | "reducer" => Ok(EntityType::Reducer),
        "tables" | "table" => Ok(EntityType::Table),
        "types" | "type" => Ok(EntityType::Type),
        "views" | "view" => Ok(EntityType::View),
        _ => {
            anyhow::bail!(
                "Invalid entity_type '{entity_type}'. \
                 Expected one of: env, procedures, reducers, routes, tables, types, views."
            )
        }
    }
}

/// Parses the describe arguments left over once the database has been resolved.
fn parse_selection(parts: &[String]) -> anyhow::Result<Selection<'_>> {
    match parts {
        [] => Ok(Selection::Module),
        [entity_type] => Ok(Selection::All(parse_entity_type(entity_type)?)),
        [entity_type, entity_name] => Ok(Selection::One(parse_entity_type(entity_type)?, entity_name)),
        _ => anyhow::bail!("Invalid describe arguments.\nUsage: {USAGE}"),
    }
}

fn output_format(args: &ArgMatches) -> Format {
    if args.get_flag("json") {
        Format::Json
    } else {
        *args.get_one::<Format>("format").unwrap()
    }
}

/// Colour only when stdout is a terminal and `NO_COLOR` is unset.
fn text_style() -> PrettyPrintStyle {
    if !std::io::stdout().is_terminal() {
        return PrettyPrintStyle::NoColor;
    }
    match pretty_print_style_from_env() {
        EnvStyle::AnsiColor => PrettyPrintStyle::AnsiColor,
        EnvStyle::NoColor => PrettyPrintStyle::NoColor,
    }
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    eprintln!("{UNSTABLE_WARNING}\n");

    let format = output_format(args);
    let no_config = args.get_flag("no_config");
    let raw_parts: Vec<String> = args
        .get_many::<String>("describe_parts")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();
    let config_targets = load_config_db_targets(no_config)?;
    let resolved = resolve_database_with_optional_parts(&raw_parts, config_targets.as_deref(), USAGE)?;
    let selection = parse_selection(&resolved.remaining_args)?;

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

    let raw = api.module_def().await?;

    fn sats_to_json<T: sats::Serialize>(v: &T) -> serde_json::Result<String> {
        serde_json::to_string_pretty(sats::serde::SerdeWrapper::from_ref(v))
    }

    // The whole-module JSON is the raw, unvalidated def.
    if format == Format::Json && matches!(selection, Selection::Module) {
        // TODO: validate the JSON output
        println!("{}", sats_to_json(&raw)?);
        return Ok(());
    }

    // Entity lookups and text rendering go through the validated `ModuleDef`, which resolves
    // canonical names and dot-qualified names from submodules (e.g. `lib.my_reducer`).
    let module_def: ModuleDef = raw.try_into()?;
    match selection {
        Selection::Module => print!("{}", describe_module(&module_def, text_style())),
        Selection::All(EntityType::Procedure) => match format {
            Format::Json => {
                let procedures: Vec<RawProcedureDefV10> = sorted_procedures(&module_def)
                    .into_iter()
                    .map(|(_, _, procedure)| procedure.clone().into())
                    .collect();
                println!("{}", sats_to_json(&procedures)?)
            }
            Format::Text => print!("{}", describe_procedures(&module_def, text_style())),
        },
        Selection::All(EntityType::Reducer) => match format {
            Format::Json => {
                let reducers: Vec<RawReducerDefV10> = sorted_reducers(&module_def)
                    .into_iter()
                    .map(|(_, _, reducer)| reducer.clone().into())
                    .collect();
                println!("{}", sats_to_json(&reducers)?)
            }
            Format::Text => print!("{}", describe_reducers(&module_def, text_style())),
        },
        Selection::All(EntityType::Table) => match format {
            Format::Json => {
                let tables: Vec<RawTableDefV10> = sorted_tables(&module_def)
                    .into_iter()
                    .map(|(_, _, table)| table.clone().into())
                    .collect();
                println!("{}", sats_to_json(&tables)?)
            }
            Format::Text => print!("{}", describe_tables(&module_def, text_style())),
        },
        Selection::All(EntityType::View) => match format {
            Format::Json => {
                let views: Vec<RawViewDefV10> = sorted_views(&module_def)
                    .into_iter()
                    .map(|(_, _, view)| view.clone().into())
                    .collect();
                println!("{}", sats_to_json(&views)?)
            }
            Format::Text => print!("{}", describe_views(&module_def, text_style())),
        },
        Selection::All(EntityType::HttpRoute) => match format {
            Format::Json => {
                let routes: Vec<RawHttpRouteDefV10> =
                    module_def.http_routes().iter().cloned().map(Into::into).collect();
                println!("{}", sats_to_json(&routes)?)
            }
            Format::Text => print!("{}", describe_http_routes(&module_def, text_style())),
        },
        Selection::All(EntityType::EnvVar) => match format {
            Format::Json => {
                let declarations: Vec<&EnvironmentDeclaration> = module_def.environment().declarations().collect();
                println!("{}", sats_to_json(&declarations)?)
            }
            Format::Text => print!("{}", describe_env_vars(&module_def, text_style())),
        },
        Selection::All(EntityType::Type) => match format {
            Format::Json => {
                let types: Vec<RawTypeDefV10> = sorted_types(&module_def)
                    .into_iter()
                    .map(|named| named.def.clone().into())
                    .collect();
                println!("{}", sats_to_json(&types)?)
            }
            Format::Text => print!("{}", describe_types(&module_def, text_style())),
        },
        Selection::One(EntityType::Procedure, procedure_name) => {
            let (prefix, owning, procedure) = module_def
                .all_procedures_with_prefix()
                .into_iter()
                .find(|(prefix, _, p)| format!("{prefix}{}", p.name) == *procedure_name)
                .context("no such procedure")?;
            match format {
                Format::Json => println!("{}", sats_to_json(&RawProcedureDefV10::from(procedure.clone()))?),
                Format::Text => print!("{}", describe_procedure(&prefix, owning, procedure, text_style())),
            }
        }
        Selection::One(EntityType::Reducer, reducer_name) => {
            let (_, reducer, owning) = module_def
                .reducer_by_name_with_module(reducer_name)
                .context("no such reducer")?;
            match format {
                Format::Json => println!("{}", sats_to_json(&RawReducerDefV10::from(reducer.clone()))?),
                Format::Text => print!("{}", describe_reducer(owning, reducer, text_style())),
            }
        }
        Selection::One(EntityType::Table, table_name) => {
            let (prefix, owning, table) = module_def
                .all_tables_with_prefix()
                .into_iter()
                .find(|(prefix, _, t)| format!("{}{}", prefix, &*t.name) == *table_name)
                .context("no such table")?;
            match format {
                Format::Json => println!("{}", sats_to_json(&RawTableDefV10::from(table.clone()))?),
                Format::Text => print!("{}", describe_table(&prefix, owning, table, text_style())),
            }
        }
        Selection::One(EntityType::View, view_name) => {
            let (prefix, owning, view) = module_def
                .all_views_with_prefix()
                .into_iter()
                .find(|(prefix, _, v)| format!("{prefix}{}", v.name) == *view_name)
                .context("no such view")?;
            match format {
                Format::Json => println!("{}", sats_to_json(&RawViewDefV10::from(view.clone()))?),
                Format::Text => print!("{}", describe_view(&prefix, owning, view, text_style())),
            }
        }
        Selection::One(EntityType::HttpRoute, path) => {
            // A path can be routed once per method, so this can match several routes.
            let routes: Vec<&HttpRouteDef> = module_def
                .http_routes()
                .iter()
                .filter(|route| &*route.path == path)
                .collect();
            anyhow::ensure!(!routes.is_empty(), "no such route");
            match format {
                Format::Json => {
                    let routes: Vec<RawHttpRouteDefV10> = routes.into_iter().cloned().map(Into::into).collect();
                    println!("{}", sats_to_json(&routes)?)
                }
                Format::Text => {
                    for route in routes {
                        print!("{}", describe_http_route(route, text_style()));
                    }
                }
            }
        }
        Selection::One(EntityType::EnvVar, key) => {
            let declaration = module_def
                .environment()
                .get(key)
                .context("no such environment variable")?;
            match format {
                Format::Json => println!("{}", sats_to_json(declaration)?),
                Format::Text => print!("{}", describe_env_var(declaration, text_style())),
            }
        }
        Selection::One(EntityType::Type, type_name) => {
            // Any named type can be looked up, including the row types the listing leaves out.
            let named = all_named_types(&module_def)
                .into_iter()
                .find(|named| named.qualified == *type_name)
                .context("no such type")?;
            match format {
                Format::Json => println!("{}", sats_to_json(&RawTypeDefV10::from(named.def.clone()))?),
                Format::Text => print!("{}", describe_type(&named, text_style())),
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    fn parse_format(args: &[&str]) -> Format {
        output_format(&cli().try_get_matches_from(args).unwrap())
    }

    fn parts(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    #[test]
    fn cli_is_well_formed() {
        cli().debug_assert();
    }

    #[test]
    fn format_defaults_to_text() {
        assert!(parse_format(&["describe", "db"]) == Format::Text);
    }

    #[test]
    fn json_flag_selects_json() {
        assert!(parse_format(&["describe", "db", "--json"]) == Format::Json);
    }

    #[test]
    fn format_json_selects_json() {
        assert!(parse_format(&["describe", "db", "--format", "json"]) == Format::Json);
    }

    #[test]
    fn format_text_aliases_select_text() {
        assert!(parse_format(&["describe", "db", "--format", "txt"]) == Format::Text);
        assert!(parse_format(&["describe", "db", "--format", "default"]) == Format::Text);
    }

    #[test]
    fn json_flag_conflicts_with_explicit_format() {
        for format in ["text", "json"] {
            let err = cli()
                .try_get_matches_from(["describe", "db", "--json", "--format", format])
                .err()
                .unwrap_or_else(|| panic!("`--json --format {format}` should be rejected"));
            assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
        }
    }

    #[test]
    fn json_flag_keeps_entity_parts() {
        let matches = cli()
            .try_get_matches_from(["describe", "db", "tables", "t", "--json"])
            .unwrap();
        let parts: Vec<&String> = matches.get_many::<String>("describe_parts").unwrap().collect();
        assert_eq!(parts, ["db", "tables", "t"]);
        assert!(output_format(&matches) == Format::Json);
    }

    #[test]
    fn no_entity_selects_the_module() {
        assert_eq!(parse_selection(&parts(&[])).unwrap(), Selection::Module);
    }

    #[test]
    fn entity_type_alone_selects_all_of_that_type() {
        for (entity_type, expected) in [
            ("env", EntityType::EnvVar),
            ("procedures", EntityType::Procedure),
            ("reducers", EntityType::Reducer),
            ("routes", EntityType::HttpRoute),
            ("tables", EntityType::Table),
            ("types", EntityType::Type),
            ("views", EntityType::View),
        ] {
            assert_eq!(
                parse_selection(&parts(&[entity_type])).unwrap(),
                Selection::All(expected)
            );
        }
    }

    #[test]
    fn entity_type_and_name_select_one_entity() {
        for (entity_type, name, expected) in [
            ("env", "API_KEY", EntityType::EnvVar),
            ("procedures", "lib.count", EntityType::Procedure),
            ("reducers", "lib.add", EntityType::Reducer),
            ("routes", "/webhook", EntityType::HttpRoute),
            ("tables", "person", EntityType::Table),
            ("types", "geo.Point", EntityType::Type),
            ("views", "lib.active", EntityType::View),
        ] {
            assert_eq!(
                parse_selection(&parts(&[entity_type, name])).unwrap(),
                Selection::One(expected, name)
            );
        }
    }

    #[test]
    fn singular_entity_types_are_accepted() {
        for (singular, plural) in [
            ("environment", "env"),
            ("procedure", "procedures"),
            ("reducer", "reducers"),
            ("route", "routes"),
            ("table", "tables"),
            ("type", "types"),
            ("view", "views"),
        ] {
            assert_eq!(
                parse_selection(&parts(&[singular])).unwrap(),
                parse_selection(&parts(&[plural])).unwrap()
            );
            assert_eq!(
                parse_selection(&parts(&[singular, "x"])).unwrap(),
                parse_selection(&parts(&[plural, "x"])).unwrap()
            );
        }
    }

    #[test]
    fn unknown_entity_type_is_rejected() {
        for bad in [parts(&["Tables"]), parts(&["columns", "name"])] {
            let err = parse_selection(&bad).unwrap_err().to_string();
            assert!(err.contains("Invalid entity_type"), "{err}");
            assert!(
                err.contains("env, procedures, reducers, routes, tables, types, views"),
                "{err}"
            );
        }
    }

    #[test]
    fn too_many_parts_are_rejected() {
        let err = parse_selection(&parts(&["table", "person", "extra"]))
            .unwrap_err()
            .to_string();
        assert!(err.contains("Invalid describe arguments"), "{err}");
    }
}
