use anyhow::{ensure, Context};
use clap::Arg;
use clap::ArgAction::{Set, SetTrue};
use clap::ArgMatches;
use reqwest::{StatusCode, Url};
use spacetimedb_client_api_messages::name::{is_identity, parse_database_name, PublishResult};
use spacetimedb_client_api_messages::name::{DatabaseNameError, PrePublishResult, PrettyPrintStyle, PublishOp};
use std::path::PathBuf;
use std::{env, fs};

use crate::common_args::ClearMode;
use crate::config::Config;
use crate::spacetime_config::{CommandConfig, CommandSchema, CommandSchemaBuilder, Key, SpacetimeConfig};
use crate::util::{add_auth_header_opt, get_auth_header, AuthHeader, ResponseExt};
use crate::util::{decode_identity, y_or_n};
use crate::{build, common_args};

/// Build the CommandSchema for publish command
pub fn build_publish_schema(command: &clap::Command) -> Result<CommandSchema, anyhow::Error> {
    CommandSchemaBuilder::new()
        .key(Key::new::<String>("database").from_clap("name|identity").required())
        .key(Key::new::<String>("server"))
        .key(
            Key::new::<PathBuf>("module_path")
                .from_clap("project_path")
                .module_specific(),
        )
        .key(Key::new::<String>("build_options").module_specific())
        .key(Key::new::<PathBuf>("wasm_file").module_specific())
        .key(Key::new::<PathBuf>("js_file").module_specific())
        .key(Key::new::<u8>("num_replicas"))
        .key(Key::new::<bool>("break_clients"))
        .key(Key::new::<bool>("anon_identity"))
        .key(Key::new::<String>("parent"))
        .exclude("clear-database")
        .exclude("force")
        .build(command)
        .map_err(Into::into)
}

/// Get filtered publish configs based on CLI arguments
pub fn get_filtered_publish_configs<'a>(
    spacetime_config: &'a SpacetimeConfig,
    schema: &'a CommandSchema,
    args: &'a ArgMatches,
) -> Result<Vec<CommandConfig<'a>>, anyhow::Error> {
    // Get all publish targets from config
    let all_targets: Vec<_> = spacetime_config
        .publish
        .as_ref()
        .map(|p| p.iter_all_targets().collect())
        .unwrap_or_default();

    // If no config file, return empty (will use CLI args only)
    if all_targets.is_empty() {
        return Ok(vec![]);
    }

    // Build CommandConfig for each target
    let all_configs: Vec<CommandConfig> = all_targets
        .into_iter()
        .map(|target| {
            let config = CommandConfig::new(schema, target.additional_fields.clone(), args)?;
            config.validate()?;
            Ok(config)
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()?;

    // Filter by database name if provided via CLI
    let filtered_configs: Vec<CommandConfig> = if schema.is_from_cli(args, "database") {
        let cli_database = schema.get_clap_arg::<String>(args, "database")?;
        all_configs
            .into_iter()
            .filter(|config| {
                // Get config-only value (not merged with CLI) for filtering
                let config_database = config
                    .get_config_value("database")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                config_database.as_deref() == cli_database.as_deref()
            })
            .collect()
    } else {
        all_configs
    };

    // Validate module-specific args aren't used with multiple targets
    if filtered_configs.len() > 1 {
        let module_specific_args = schema.module_specific_cli_args(args);
        if !module_specific_args.is_empty() {
            anyhow::bail!(
                "Cannot use module-specific arguments ({}) when publishing to multiple targets. \
                 Please specify a database filter (--database) or remove these arguments.",
                module_specific_args.join(", ")
            );
        }
    }

    Ok(filtered_configs)
}

pub fn cli() -> clap::Command {
    clap::Command::new("publish")
        .about("Create and update a SpacetimeDB database")
        .arg(
            common_args::clear_database()
                .requires("name|identity")
        )
        .arg(
            Arg::new("build_options")
                .long("build-options")
                .alias("build-opts")
                .action(Set)
                .default_value("")
                .help("Options to pass to the build command, for example --build-options='--lint-dir='")
        )
        .arg(
            Arg::new("project_path")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(".")
                .long("project-path")
                .short('p')
                .help("The system path (absolute or relative) to the module project")
        )
        .arg(
            Arg::new("wasm_file")
                .value_parser(clap::value_parser!(PathBuf))
                .long("bin-path")
                .short('b')
                .conflicts_with("project_path")
                .conflicts_with("build_options")
                .conflicts_with("js_file")
                .help("The system path (absolute or relative) to the compiled wasm binary we should publish, instead of building the project."),
        )
        .arg(
            Arg::new("js_file")
                .value_parser(clap::value_parser!(PathBuf))
                .long("js-path")
                .short('j')
                .conflicts_with("project_path")
                .conflicts_with("build_options")
                .conflicts_with("wasm_file")
                .help("UNSTABLE: The system path (absolute or relative) to the javascript file we should publish, instead of building the project."),
        )
        .arg(
            Arg::new("num_replicas")
                .value_parser(clap::value_parser!(u8))
                .long("num-replicas")
                .hide(true)
                .help("UNSTABLE: The number of replicas the database should have")
        )
        .arg(
            Arg::new("break_clients")
                .long("break-clients")
                .action(SetTrue)
                .help("Allow breaking changes when publishing to an existing database identity. This will force publish even if it will break existing clients, but will NOT force publish if it would cause deletion of any data in the database. See --yes and --delete-data for details.")
        )
        .arg(
            common_args::anonymous()
        )
        .arg(
            Arg::new("parent")
            .help("Domain or identity of a parent for this database")
            .long("parent")
            .long_help(
"A valid domain or identity of an existing database that should be the parent of this database.

If a parent is given, the new database inherits the team permissions from the parent.
A parent can only be set when a database is created, not when it is updated."
            )
        )
        .arg(
            Arg::new("name|identity")
                .help("A valid domain or identity for this database")
                .long_help(
"A valid domain or identity for this database.

Database names must match the regex `/^[a-z0-9]+(-[a-z0-9]+)*$/`,
i.e. only lowercase ASCII letters and numbers, separated by dashes."),
        )
        .arg(common_args::server()
                .help("The nickname, domain name or URL of the server to host the database."),
        )
        .arg(
            common_args::yes()
        )
        .after_help("Run `spacetime help publish` for more detailed information.")
}

fn confirm_and_clear(
    name_or_identity: &str,
    force: bool,
    mut builder: reqwest::RequestBuilder,
) -> Result<reqwest::RequestBuilder, anyhow::Error> {
    println!(
        "This will DESTROY the current {} module, and ALL corresponding data.",
        name_or_identity
    );
    if !y_or_n(
        force,
        format!("Are you sure you want to proceed? [deleting {}]", name_or_identity).as_str(),
    )? {
        anyhow::bail!("Aborted.");
    }

    builder = builder.query(&[("clear", true)]);
    Ok(builder)
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    exec_with_options(config, args, false).await
}

/// This function can be used when calling publish programatically rather than straight from the
/// CLI, like we do in `spacetime dev`. When calling from `spacetime dev` we don't want to display
/// information about using the `spacetime.json` file as it's already announced as part of the
/// `dev` command
pub async fn exec_with_options(mut config: Config, args: &ArgMatches, quiet_config: bool) -> Result<(), anyhow::Error> {
    // Build schema
    let cmd = cli();
    let schema = build_publish_schema(&cmd)?;

    // Get publish configs (from spacetime.json or empty)
    let spacetime_config_opt = SpacetimeConfig::find_and_load()?;
    let (using_config, publish_configs) = if let Some((config_path, ref spacetime_config)) = spacetime_config_opt {
        if !quiet_config {
            println!("Using configuration from {}", config_path.display());
        }
        let filtered = get_filtered_publish_configs(spacetime_config, &schema, args)?;
        // If filtering resulted in no matches, use CLI args with empty config
        if filtered.is_empty() {
            (
                false,
                vec![CommandConfig::new(&schema, std::collections::HashMap::new(), args)?],
            )
        } else {
            (true, filtered)
        }
    } else {
        (
            false,
            vec![CommandConfig::new(&schema, std::collections::HashMap::new(), args)?],
        )
    };

    // Execute publish for each config
    for command_config in publish_configs {
        // Get values using command_config.get_one() which merges CLI + config
        let server_opt = command_config.get_one::<String>("server")?;
        let server = server_opt.as_deref();
        let name_or_identity_opt = command_config.get_one::<String>("database")?;
        let name_or_identity = name_or_identity_opt.as_deref();
        let path_to_project = command_config
            .get_one::<PathBuf>("module_path")?
            .unwrap_or_else(|| PathBuf::from("."));

        if using_config {
            println!(
                "Publishing module {} to database '{}'",
                path_to_project.display(),
                name_or_identity.unwrap()
            );
        }
        let clear_database = args
            .get_one::<ClearMode>("clear-database")
            .copied()
            .unwrap_or(ClearMode::Never);
        let force = args.get_flag("force");
        let anon_identity = command_config.get_one::<bool>("anon_identity")?.unwrap_or(false);
        let wasm_file = command_config.get_one::<PathBuf>("wasm_file")?;
        let js_file = command_config.get_one::<PathBuf>("js_file")?;
        let database_host = config.get_host_url(server)?;
        let build_options = command_config
            .get_one::<String>("build_options")?
            .unwrap_or_else(|| String::new());
        let num_replicas = command_config.get_one::<u8>("num_replicas")?;
        let force_break_clients = command_config.get_one::<bool>("break_clients")?.unwrap_or(false);
        let parent_opt = command_config.get_one::<String>("parent")?;
        let parent = parent_opt.as_deref();

        // If the user didn't specify an identity and we didn't specify an anonymous identity, then
        // we want to use the default identity
        // TODO(jdetter): We should maybe have some sort of user prompt here for them to be able to
        //  easily create a new identity with an email
        let auth_header = get_auth_header(&mut config, anon_identity, server, !force).await?;

        let (name_or_identity, parent) = validate_name_and_parent(name_or_identity, parent)?;

        if !path_to_project.exists() {
            return Err(anyhow::anyhow!(
                "Project path does not exist: {}",
                path_to_project.display()
            ));
        }

        // Decide program file path and read program.
        // Optionally build the program.
        let (path_to_program, host_type) = if let Some(path) = wasm_file {
            println!("(WASM) Skipping build. Instead we are publishing {}", path.display());
            (path.clone(), "Wasm")
        } else if let Some(path) = js_file {
            println!("(JS) Skipping build. Instead we are publishing {}", path.display());
            (path.clone(), "Js")
        } else {
            build::exec_with_argstring(config.clone(), &path_to_project, &build_options).await?
        };
        let program_bytes = fs::read(path_to_program)?;

        let server_address = {
            let url = Url::parse(&database_host)?;
            url.host_str().unwrap_or("<default>").to_string()
        };
        if server_address != "localhost" && server_address != "127.0.0.1" {
            println!("You are about to publish to a non-local server: {server_address}");
            if !y_or_n(force, "Are you sure you want to proceed?")? {
                println!("Aborting");
                return Ok(());
            }
        }

        println!(
            "Uploading to {} => {}",
            server.unwrap_or(config.default_server_name().unwrap_or("<default>")),
            database_host
        );

        let client = reqwest::Client::new();
        // If a name was given, ensure to percent-encode it.
        // We also use PUT with a name or identity, and POST otherwise.
        let mut builder = if let Some(name_or_identity) = name_or_identity {
            let encode_set = const { &percent_encoding::NON_ALPHANUMERIC.remove(b'_').remove(b'-') };
            let domain = percent_encoding::percent_encode(name_or_identity.as_bytes(), encode_set);
            let mut builder = client.put(format!("{database_host}/v1/database/{domain}"));

            // note that this only happens in the case where we've passed a `name_or_identity`, but that's required if we pass `--clear-database`.
            if clear_database == ClearMode::Always {
                builder = confirm_and_clear(name_or_identity, force, builder)?;
            } else {
                builder = apply_pre_publish_if_needed(
                    builder,
                    &client,
                    &database_host,
                    name_or_identity,
                    &domain.to_string(),
                    host_type,
                    &program_bytes,
                    &auth_header,
                    clear_database,
                    force_break_clients,
                    force,
                )
                .await?;
            }

            builder
        } else {
            client.post(format!("{database_host}/v1/database"))
        };

        if let Some(n) = num_replicas {
            eprintln!("WARNING: Use of unstable option `--num-replicas`.\n");
            builder = builder.query(&[("num_replicas", n)]);
        }
        if let Some(parent) = parent {
            builder = builder.query(&[("parent", parent)]);
        }

        println!("Publishing module...");

        builder = add_auth_header_opt(builder, &auth_header);

        // Set the host type.
        builder = builder.query(&[("host_type", host_type)]);

        // JS/TS is beta quality atm.
        if host_type == "Js" {
            println!("JavaScript / TypeScript support is currently in BETA.");
            println!("There may be bugs. Please file issues if you encounter any.");
            println!("<https://github.com/clockworklabs/SpacetimeDB/issues/new>");
        }

        let res = builder.body(program_bytes).send().await?;
        let response: PublishResult = res.json_or_error().await?;
        match response {
            PublishResult::Success {
                domain,
                database_identity,
                op,
            } => {
                let op = match op {
                    PublishOp::Created => "Created new",
                    PublishOp::Updated => "Updated",
                };
                if let Some(domain) = domain {
                    println!("{op} database with name: {domain}, identity: {database_identity}");
                } else {
                    println!("{op} database with identity: {database_identity}");
                }
            }
            PublishResult::PermissionDenied { name } => {
                if anon_identity {
                    anyhow::bail!("You need to be logged in as the owner of {name} to publish to {name}",);
                }
                // If we're not in the `anon_identity` case, then we have already forced the user to log in above (using `get_auth_header`), so this should be safe to unwrap.
                let token = config.spacetimedb_token().unwrap();
                let identity = decode_identity(token)?;
                //TODO(jdetter): Have a nice name generator here, instead of using some abstract characters
                // we should perhaps generate fun names like 'green-fire-dragon' instead
                let suggested_tld: String = identity.chars().take(12).collect();
                return Err(anyhow::anyhow!(
                    "The database {name} is not registered to the identity you provided.\n\
                    We suggest you push to either a domain owned by you, or a new domain like:\n\
                    \tspacetime publish {suggested_tld}\n",
                ));
            }
        }
    }

    Ok(())
}

fn validate_name_or_identity(name_or_identity: &str) -> Result<(), DatabaseNameError> {
    if is_identity(name_or_identity) {
        Ok(())
    } else {
        parse_database_name(name_or_identity).map(drop)
    }
}

fn invalid_parent_name(name: &str) -> String {
    format!("invalid parent database name `{name}`")
}

fn validate_name_and_parent<'a>(
    name: Option<&'a str>,
    parent: Option<&'a str>,
) -> anyhow::Result<(Option<&'a str>, Option<&'a str>)> {
    if let Some(parent) = parent.as_ref() {
        validate_name_or_identity(parent).with_context(|| invalid_parent_name(parent))?;
    }

    match name {
        Some(name) => match name.split_once('/') {
            Some((parent_alt, child)) => {
                ensure!(
                    parent.is_none() || parent.is_some_and(|parent| parent == parent_alt),
                    "cannot specify both --parent and <parent>/<child>"
                );
                validate_name_or_identity(parent_alt).with_context(|| invalid_parent_name(parent_alt))?;
                validate_name_or_identity(child)?;

                Ok((Some(child), Some(parent_alt)))
            }
            None => {
                validate_name_or_identity(name)?;
                Ok((Some(name), parent))
            }
        },
        None => Ok((None, parent)),
    }
}

/// Determine the pretty print style based on the NO_COLOR environment variable.
///
/// See: https://no-color.org
pub fn pretty_print_style_from_env() -> PrettyPrintStyle {
    match env::var("NO_COLOR") {
        Ok(_) => PrettyPrintStyle::NoColor,
        Err(_) => PrettyPrintStyle::AnsiColor,
    }
}

/// Applies pre-publish logic: checking for migration plan, prompting user, and
/// modifying the request builder accordingly.
#[allow(clippy::too_many_arguments)]
async fn apply_pre_publish_if_needed(
    mut builder: reqwest::RequestBuilder,
    client: &reqwest::Client,
    base_url: &str,
    name_or_identity: &str,
    domain: &String,
    host_type: &str,
    program_bytes: &[u8],
    auth_header: &AuthHeader,
    clear_database: ClearMode,
    force_break_clients: bool,
    force: bool,
) -> Result<reqwest::RequestBuilder, anyhow::Error> {
    // The caller enforces this
    assert!(clear_database != ClearMode::Always);

    if let Some(pre) = call_pre_publish(
        client,
        base_url,
        &domain.to_string(),
        host_type,
        program_bytes,
        auth_header,
    )
    .await?
    {
        match pre {
            PrePublishResult::ManualMigrate(manual) => {
                if clear_database == ClearMode::Never {
                    println!("{}", manual.reason);
                    println!("Aborting publish due to required manual migration.");
                    anyhow::bail!("Aborting because publishing would require manual migration or deletion of data and --delete-data was not specified.");
                }
                println!("{}", manual.reason);
                println!("Proceeding with database clear due to --delete-data=on-conflict.");

                builder = confirm_and_clear(name_or_identity, force, builder)?;
            }
            PrePublishResult::AutoMigrate(auto) => {
                println!("{}", auto.migrate_plan);
                // We only arrive here if you have not specified ClearMode::Always AND there was no
                // conflict that required manual migration.
                if auto.break_clients
                    && !y_or_n(
                        force_break_clients || force,
                        "The above changes will BREAK existing clients. Do you want to proceed?",
                    )?
                {
                    println!("Aborting");
                    // Early exit: return an error or a special signal. Here we bail out by returning Err.
                    anyhow::bail!("Publishing aborted by user");
                }
                builder = builder
                    .query(&[("token", auto.token)])
                    .query(&[("policy", "BreakClients")]);
            }
        }
    }

    Ok(builder)
}

async fn call_pre_publish(
    client: &reqwest::Client,
    database_host: &str,
    domain: &String,
    host_type: &str,
    program_bytes: &[u8],
    auth_header: &AuthHeader,
) -> Result<Option<PrePublishResult>, anyhow::Error> {
    let mut builder = client.post(format!("{database_host}/v1/database/{domain}/pre_publish"));
    let style: PrettyPrintStyle = pretty_print_style_from_env();
    builder = builder
        .query(&[("pretty_print_style", style)])
        .query(&[("host_type", host_type)]);

    builder = add_auth_header_opt(builder, auth_header);

    println!("Checking for breaking changes...");
    let res = builder.body(program_bytes.to_vec()).send().await?;

    if res.status() == StatusCode::NOT_FOUND {
        // This is a new database, so there are no breaking changes
        return Ok(None);
    }

    if !res.status().is_success() {
        anyhow::bail!(
            "Pre-publish check failed with status {}: {}",
            res.status(),
            res.text().await?
        );
    }

    let pre_publish_result: PrePublishResult = res.json_or_error().await?;
    Ok(Some(pre_publish_result))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_matches;
    use spacetimedb_lib::Identity;

    use super::*;

    #[test]
    fn validate_none_arguments_returns_none_values() {
        assert_matches!(validate_name_and_parent(None, None), Ok((None, None)));
        assert_matches!(validate_name_and_parent(Some("foo"), None), Ok((Some(_), None)));
        assert_matches!(validate_name_and_parent(None, Some("foo")), Ok((None, Some(_))));
    }

    #[test]
    fn validate_valid_arguments_returns_arguments() {
        let name = "child";
        let parent = "parent";
        let result = (Some(name), Some(parent));
        assert_matches!(
            validate_name_and_parent(Some(name), Some(parent)),
            Ok(val) if val == result
        );
    }

    #[test]
    fn validate_parent_and_path_name_returns_error_unless_parent_equal() {
        assert_matches!(
            validate_name_and_parent(Some("parent/child"), Some("parent")),
            Ok((Some("child"), Some("parent")))
        );
        assert_matches!(validate_name_and_parent(Some("parent/child"), Some("cousin")), Err(_));
    }

    #[test]
    fn validate_more_than_two_path_segments_are_an_error() {
        assert_matches!(validate_name_and_parent(Some("proc/net/tcp"), None), Err(_));
        assert_matches!(validate_name_and_parent(Some("proc//net"), None), Err(_));
    }

    #[test]
    fn validate_trailing_slash_is_an_error() {
        assert_matches!(validate_name_and_parent(Some("foo//"), None), Err(_));
        assert_matches!(validate_name_and_parent(Some("foo/bar/"), None), Err(_));
    }

    #[test]
    fn validate_parent_cant_have_slash() {
        assert_matches!(validate_name_and_parent(Some("child"), Some("par/ent")), Err(_));
        assert_matches!(validate_name_and_parent(Some("child"), Some("parent/")), Err(_));
    }

    #[test]
    fn validate_name_or_parent_can_be_identities() {
        let parent = Identity::ZERO.to_string();
        let child = Identity::ONE.to_string();

        assert_matches!(
            validate_name_and_parent(Some(&child), Some(&parent)),
            Ok(res) if res == (Some(&child), Some(&parent))
        );
    }

    #[test]
    fn test_filter_by_database_from_cli() {
        use crate::spacetime_config::*;
        use std::collections::HashMap;

        let cmd = cli();
        let schema = build_publish_schema(&cmd).unwrap();

        let mut config1 = HashMap::new();
        config1.insert("database".to_string(), serde_json::Value::String("db1".to_string()));

        let mut config2 = HashMap::new();
        config2.insert("database".to_string(), serde_json::Value::String("db2".to_string()));

        let mut parent_config = HashMap::new();
        parent_config.insert(
            "database".to_string(),
            serde_json::Value::String("parent-db".to_string()),
        );

        let spacetime_config = SpacetimeConfig {
            publish: Some(PublishConfig {
                additional_fields: parent_config,
                children: Some(vec![
                    PublishConfig {
                        additional_fields: config1,
                        children: None,
                    },
                    PublishConfig {
                        additional_fields: config2,
                        children: None,
                    },
                ]),
            }),
            ..Default::default()
        };

        // Filter by db1 (should only match config1, not parent or config2)
        let matches = cmd.clone().get_matches_from(vec!["publish", "db1"]);
        let filtered = get_filtered_publish_configs(&spacetime_config, &schema, &matches).unwrap();

        assert_eq!(filtered.len(), 1, "Should only match db1");
        assert_eq!(
            filtered[0].get_one::<String>("database").unwrap(),
            Some("db1".to_string())
        );
    }

    #[test]
    fn test_no_filter_when_database_not_from_cli() {
        use crate::spacetime_config::*;
        use std::collections::HashMap;

        let cmd = cli();
        let schema = build_publish_schema(&cmd).unwrap();

        let mut config1 = HashMap::new();
        config1.insert("database".to_string(), serde_json::Value::String("db1".to_string()));

        let mut config2 = HashMap::new();
        config2.insert("database".to_string(), serde_json::Value::String("db2".to_string()));

        let mut parent_config = HashMap::new();
        parent_config.insert(
            "database".to_string(),
            serde_json::Value::String("parent-db".to_string()),
        );

        let spacetime_config = SpacetimeConfig {
            publish: Some(PublishConfig {
                additional_fields: parent_config,
                children: Some(vec![
                    PublishConfig {
                        additional_fields: config1,
                        children: None,
                    },
                    PublishConfig {
                        additional_fields: config2,
                        children: None,
                    },
                ]),
            }),
            ..Default::default()
        };

        // No database provided via CLI
        let matches = cmd.clone().get_matches_from(vec!["publish"]);
        let filtered = get_filtered_publish_configs(&spacetime_config, &schema, &matches).unwrap();

        // Should return all configs (parent + 2 children)
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn test_empty_result_when_filter_no_match() {
        use crate::spacetime_config::*;
        use std::collections::HashMap;

        let cmd = cli();
        let schema = build_publish_schema(&cmd).unwrap();

        let mut config1 = HashMap::new();
        config1.insert("database".to_string(), serde_json::Value::String("db1".to_string()));

        let mut parent_config = HashMap::new();
        parent_config.insert(
            "database".to_string(),
            serde_json::Value::String("parent-db".to_string()),
        );

        let spacetime_config = SpacetimeConfig {
            publish: Some(PublishConfig {
                additional_fields: parent_config,
                children: Some(vec![PublishConfig {
                    additional_fields: config1,
                    children: None,
                }]),
            }),
            ..Default::default()
        };

        // Filter by non-existent database
        let matches = cmd.clone().get_matches_from(vec!["publish", "nonexistent"]);
        let filtered = get_filtered_publish_configs(&spacetime_config, &schema, &matches).unwrap();

        assert_eq!(filtered.len(), 0);
    }
}
