use anyhow::{ensure, Context};
use clap::Arg;
use clap::ArgAction::{Set, SetTrue};
use clap::ArgMatches;
use reqwest::{StatusCode, Url};
use spacetimedb_client_api_messages::name::{is_identity, parse_database_name, PublishResult};
use spacetimedb_client_api_messages::name::{DatabaseNameError, PrePublishResult, PrettyPrintStyle, PublishOp};
use std::collections::HashMap;
use std::path::PathBuf;
use std::{env, fs};

use crate::common_args::ClearMode;
use crate::config::Config;
use crate::spacetime_config::{
    find_and_load_with_env, CommandConfig, CommandSchema, CommandSchemaBuilder, FlatTarget, Key, LoadedConfig,
    SpacetimeConfig,
};
use crate::util::{add_auth_header_opt, get_auth_header, strip_verbatim_prefix, AuthHeader, ResponseExt};
use crate::util::{decode_identity, y_or_n};
use crate::{build, common_args};

/// Build the CommandSchema for publish command
pub fn build_publish_schema(command: &clap::Command) -> Result<CommandSchema, anyhow::Error> {
    CommandSchemaBuilder::new()
        .key(Key::new("database").from_clap("name|identity").required())
        .key(Key::new("server"))
        .key(Key::new("module_path").module_specific())
        .key(Key::new("build_options").module_specific())
        .key(Key::new("wasm_file").module_specific())
        .key(Key::new("js_file").module_specific())
        .key(Key::new("num_replicas"))
        .key(Key::new("break_clients"))
        .key(Key::new("anon_identity"))
        .key(Key::new("parent"))
        .key(Key::new("organization"))
        .exclude("clear-database")
        .exclude("force")
        .exclude("no_config")
        .exclude("env")
        .build(command)
        .map_err(Into::into)
}

/// Get filtered publish configs based on CLI arguments.
/// Uses glob matching on database names when a pattern is provided via CLI.
pub fn get_filtered_publish_configs<'a>(
    spacetime_config: &SpacetimeConfig,
    command: &clap::Command,
    schema: &'a CommandSchema,
    args: &'a ArgMatches,
) -> Result<Vec<CommandConfig<'a>>, anyhow::Error> {
    // Get all database targets from config with parent→child inheritance
    let all_targets = spacetime_config.collect_all_targets_with_inheritance();

    // If no targets, return empty (will use CLI args only)
    if all_targets.is_empty() {
        return Ok(vec![]);
    }

    // Filter by database name pattern (glob) if provided via CLI
    let filtered_targets: Vec<FlatTarget> = if schema.is_from_cli(args, "database") {
        let cli_database = schema.get_clap_arg::<String>(args, "database")?.unwrap_or_default();

        let pattern =
            glob::Pattern::new(&cli_database).with_context(|| format!("Invalid glob pattern: {cli_database}"))?;

        let matched: Vec<FlatTarget> = all_targets
            .into_iter()
            .filter(|target| {
                target
                    .fields
                    .get("database")
                    .and_then(|v| v.as_str())
                    .is_some_and(|db| pattern.matches(db))
            })
            .collect();

        if matched.is_empty() {
            anyhow::bail!(
                "No database target matches '{}'. Available databases: {}",
                cli_database,
                spacetime_config
                    .collect_all_targets_with_inheritance()
                    .iter()
                    .filter_map(|t| t.fields.get("database").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        matched
    } else {
        all_targets
    };

    // Build CommandConfig for each target
    let configs: Vec<CommandConfig> = filtered_targets
        .into_iter()
        .map(|target| {
            let config = CommandConfig::new(schema, target.fields, args)?;
            config.validate()?;
            Ok(config)
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()?;

    schema.validate_no_module_specific_cli_args_for_multiple_targets(
        command,
        args,
        configs.len(),
        "publishing to multiple targets",
        "Please specify the database name or identity to select a single target, or remove these arguments.",
    )?;

    Ok(configs)
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
            Arg::new("module_path")
                .value_parser(clap::value_parser!(PathBuf))
                .long("module-path")
                .short('p')
                .help("The system path (absolute or relative) to the module project. Defaults to spacetimedb/ subdirectory, then current directory.")
        )
        .arg(
            Arg::new("wasm_file")
                .value_parser(clap::value_parser!(PathBuf))
                .long("bin-path")
                .short('b')
                .conflicts_with("module_path")
                .conflicts_with("build_options")
                .conflicts_with("js_file")
                .help("The system path (absolute or relative) to the compiled wasm binary we should publish, instead of building the project."),
        )
        .arg(
            Arg::new("js_file")
                .value_parser(clap::value_parser!(PathBuf))
                .long("js-path")
                .short('j')
                .conflicts_with("module_path")
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
            Arg::new("organization")
            .help("Name or identity of an organization for this database")
            .long("organization")
            .alias("org")
            .long_help(
"The name or identity of an existing organization this database should be created under.

If an organization is given, the organization member's permissions apply to the new database.
An organization can only be set when a database is created, not when it is updated."
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
        .arg(
            Arg::new("no_config")
                .long("no-config")
                .action(SetTrue)
                .help("Ignore spacetime.json configuration")
        )
        .arg(
            Arg::new("env")
                .long("env")
                .value_name("ENV")
                .action(Set)
                .help("Environment name for config file layering (e.g., dev, staging)")
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

fn confirm_major_version_upgrade() -> Result<(), anyhow::Error> {
    println!(
        "It looks like you're trying to do a major version upgrade from 1.0 to 2.0. We recommend first looking at the upgrade notes before committing to this upgrade: https://spacetimedb.com/docs/upgrade"
    );
    println!();
    println!("WARNING: Once you publish you cannot revert back to version 1.0.");
    println!();

    let mut input = String::new();
    print!("Please type 'upgrade' to accept this change: ");
    let mut stdout = std::io::stdout();
    std::io::Write::flush(&mut stdout)?;
    std::io::stdin().read_line(&mut input)?;

    if input.trim() == "upgrade" {
        return Ok(());
    }

    anyhow::bail!("Aborting because major version upgrade was not accepted.");
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    exec_with_options(config, args, false, None).await
}

/// This function can be used when calling publish programatically rather than straight from the
/// CLI, like we do in `spacetime dev`. When calling from `spacetime dev` we don't want to display
/// information about using the `spacetime.json` file as it's already announced as part of the
/// `dev` command
pub async fn exec_with_options(
    mut config: Config,
    args: &ArgMatches,
    quiet_config: bool,
    pre_loaded_config: Option<&LoadedConfig>,
) -> Result<(), anyhow::Error> {
    // Build schema
    let cmd = cli();
    let schema = build_publish_schema(&cmd)?;

    let no_config = args.get_flag("no_config");
    let env = args.get_one::<String>("env").map(|s| s.as_str());

    // Get publish configs (from spacetime.json or empty)
    let owned_loaded;
    let loaded_config_ref = if no_config {
        None
    } else if let Some(pre) = pre_loaded_config {
        Some(pre)
    } else {
        owned_loaded = find_and_load_with_env(env)?;
        owned_loaded.as_ref().inspect(|loaded| {
            if !quiet_config {
                for path in &loaded.loaded_files {
                    println!("Using configuration from {}", path.display());
                }
            }
        })
    };

    let (using_config, publish_configs) = if let Some(loaded) = loaded_config_ref {
        let filtered = get_filtered_publish_configs(&loaded.config, &cmd, &schema, args)?;
        if filtered.is_empty() {
            anyhow::bail!(
                "No matching target found in spacetime.json for the provided arguments. \
                 Use --no-config to ignore the config file."
            );
        }
        (true, filtered)
    } else {
        (
            false,
            vec![CommandConfig::new(&schema, std::collections::HashMap::new(), args)?],
        )
    };

    let clear_database = args
        .get_one::<ClearMode>("clear-database")
        .copied()
        .unwrap_or(ClearMode::Never);
    let force = args.get_flag("force");
    let config_dir = loaded_config_ref.map(|lc| lc.config_dir.as_path());

    execute_publish_configs(
        &mut config,
        publish_configs,
        using_config,
        config_dir,
        clear_database,
        force,
    )
    .await
}

pub async fn exec_from_entry(
    mut config: Config,
    entry: HashMap<String, serde_json::Value>,
    config_dir: Option<&std::path::Path>,
    clear_database: ClearMode,
    force: bool,
) -> Result<(), anyhow::Error> {
    let cmd = cli();
    let schema = build_publish_schema(&cmd)?;
    let matches = cmd.get_matches_from(vec!["publish"]);

    let command_config = CommandConfig::new(&schema, entry, &matches)?;
    command_config.validate()?;

    execute_publish_configs(
        &mut config,
        vec![command_config],
        true,
        config_dir,
        clear_database,
        force,
    )
    .await
}

async fn execute_publish_configs<'a>(
    config: &mut Config,
    publish_configs: Vec<CommandConfig<'a>>,
    using_config: bool,
    config_dir: Option<&std::path::Path>,
    clear_database: ClearMode,
    force: bool,
) -> Result<(), anyhow::Error> {
    // Execute publish for each config
    for command_config in publish_configs {
        // Get values using command_config.get_one() which merges CLI + config
        let server_opt = command_config.get_one::<String>("server")?;
        let server = server_opt.as_deref();
        let name_or_identity_opt = command_config.get_one::<String>("database")?;
        let name_or_identity = name_or_identity_opt.as_deref();
        let anon_identity = command_config.get_one::<bool>("anon_identity")?.unwrap_or(false);
        let wasm_file = command_config.get_one::<PathBuf>("wasm_file")?;
        let js_file = command_config.get_one::<PathBuf>("js_file")?;
        let resolved_module_path = command_config.get_resolved_path("module_path", config_dir)?;
        let path_to_project = if wasm_file.is_some() || js_file.is_some() {
            resolved_module_path
        } else {
            Some(match resolved_module_path {
                Some(path) => path,
                None => default_publish_module_path(&std::env::current_dir()?),
            })
        };

        if using_config {
            if let Some(path_to_project) = path_to_project.as_ref() {
                println!(
                    "Publishing module {} to database '{}'",
                    strip_verbatim_prefix(path_to_project).display(),
                    name_or_identity.unwrap()
                );
            } else {
                println!(
                    "Publishing precompiled module to database '{}'",
                    name_or_identity.unwrap()
                );
            }
        }
        let database_host = config.get_host_url(server)?;
        let build_options = command_config
            .get_one::<String>("build_options")?
            .unwrap_or_else(String::new);
        let num_replicas = command_config.get_one::<u8>("num_replicas")?;
        let force_break_clients = command_config.get_one::<bool>("break_clients")?.unwrap_or(false);
        let parent_opt = command_config.get_one::<String>("parent")?;
        let parent = parent_opt.as_deref();
        let org_opt = command_config.get_one::<String>("organization")?;
        let org = org_opt.as_deref();

        // If the user didn't specify an identity and we didn't specify an anonymous identity, then
        // we want to use the default identity
        // TODO(jdetter): We should maybe have some sort of user prompt here for them to be able to
        //  easily create a new identity with an email
        let auth_header = get_auth_header(config, anon_identity, server, !force).await?;

        let (name_or_identity, parent) = validate_name_and_parent(name_or_identity, parent)?;

        if let Some(path_to_project) = path_to_project.as_ref() {
            if !path_to_project.exists() {
                return Err(anyhow::anyhow!(
                    "Project path does not exist: {}",
                    path_to_project.display()
                ));
            }
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
            build::exec_with_argstring(
                path_to_project
                    .as_ref()
                    .expect("path_to_project must exist when publishing from source"),
                &build_options,
            )
            .await?
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
        if let Some(org) = org {
            builder = builder.query(&[("org", org)]);
        }

        println!("Publishing module...");

        builder = add_auth_header_opt(builder, &auth_header);

        // Set the host type.
        builder = builder.query(&[("host_type", host_type)]);

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
                if let Some(ref domain) = domain {
                    println!("{op} database with name: {domain}, identity: {database_identity}");
                } else {
                    println!("{op} database with identity: {database_identity}");
                }

                if is_maincloud_host(&database_host) {
                    if let Some(domain) = domain.as_ref() {
                        println!("Dashboard: https://spacetimedb.com/{}", domain.as_ref());
                    }
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

fn default_publish_module_path(current_dir: &std::path::Path) -> PathBuf {
    let spacetimedb_dir = current_dir.join("spacetimedb");
    if spacetimedb_dir.is_dir() {
        spacetimedb_dir
    } else {
        current_dir.to_path_buf()
    }
}

fn is_maincloud_host(database_host: &str) -> bool {
    Url::parse(database_host)
        .ok()
        .and_then(|url| {
            url.host_str()
                .map(|h| h.eq_ignore_ascii_case("maincloud.spacetimedb.com"))
        })
        .unwrap_or(false)
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
        let major_version_upgrade = match &pre {
            PrePublishResult::AutoMigrate(auto) => auto.major_version_upgrade,
            PrePublishResult::ManualMigrate(manual) => manual.major_version_upgrade,
        };
        if major_version_upgrade {
            confirm_major_version_upgrade()?;
        }

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
    use std::collections::HashMap;

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

    /// Helper to build a SpacetimeConfig with additional_fields (database-centric).
    fn make_config(fields: HashMap<String, serde_json::Value>) -> SpacetimeConfig {
        SpacetimeConfig {
            additional_fields: fields,
            ..Default::default()
        }
    }

    fn make_config_with_children(
        fields: HashMap<String, serde_json::Value>,
        children: Vec<SpacetimeConfig>,
    ) -> SpacetimeConfig {
        SpacetimeConfig {
            additional_fields: fields,
            children: Some(children),
            ..Default::default()
        }
    }

    #[test]
    fn test_filter_by_database_from_cli() {
        use std::collections::HashMap;

        let cmd = cli();
        let schema = build_publish_schema(&cmd).unwrap();

        let mut parent_fields = HashMap::new();
        parent_fields.insert("database".to_string(), serde_json::json!("parent-db"));

        let mut child1_fields = HashMap::new();
        child1_fields.insert("database".to_string(), serde_json::json!("db1"));

        let mut child2_fields = HashMap::new();
        child2_fields.insert("database".to_string(), serde_json::json!("db2"));

        let spacetime_config = make_config_with_children(
            parent_fields,
            vec![make_config(child1_fields), make_config(child2_fields)],
        );

        // Filter by db1 (should only match child1, not parent or child2)
        let matches = cmd.clone().get_matches_from(vec!["publish", "db1"]);
        let filtered = get_filtered_publish_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        assert_eq!(filtered.len(), 1, "Should only match db1");
        assert_eq!(
            filtered[0].get_one::<String>("database").unwrap(),
            Some("db1".to_string())
        );
    }

    #[test]
    fn test_no_filter_when_database_not_from_cli() {
        use std::collections::HashMap;

        let cmd = cli();
        let schema = build_publish_schema(&cmd).unwrap();

        let mut parent_fields = HashMap::new();
        parent_fields.insert("database".to_string(), serde_json::json!("parent-db"));

        let mut child1_fields = HashMap::new();
        child1_fields.insert("database".to_string(), serde_json::json!("db1"));

        let mut child2_fields = HashMap::new();
        child2_fields.insert("database".to_string(), serde_json::json!("db2"));

        let spacetime_config = make_config_with_children(
            parent_fields,
            vec![make_config(child1_fields), make_config(child2_fields)],
        );

        // No database provided via CLI
        let matches = cmd.clone().get_matches_from(vec!["publish"]);
        let filtered = get_filtered_publish_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        // Should return all configs (parent + 2 children)
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn test_error_when_filter_no_match() {
        use std::collections::HashMap;

        let cmd = cli();
        let schema = build_publish_schema(&cmd).unwrap();

        let mut parent_fields = HashMap::new();
        parent_fields.insert("database".to_string(), serde_json::json!("parent-db"));

        let mut child1_fields = HashMap::new();
        child1_fields.insert("database".to_string(), serde_json::json!("db1"));

        let spacetime_config = make_config_with_children(parent_fields, vec![make_config(child1_fields)]);

        // Filter by non-existent database — now errors instead of returning empty
        let matches = cmd.clone().get_matches_from(vec!["publish", "nonexistent"]);
        let result = get_filtered_publish_configs(&spacetime_config, &cmd, &schema, &matches);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("No database target matches"));
    }

    #[test]
    fn test_default_publish_module_path_prefers_spacetimedb_dir() {
        let temp = tempfile::TempDir::new().unwrap();
        let cwd = temp.path().to_path_buf();
        let spacetimedb_dir = cwd.join("spacetimedb");
        std::fs::create_dir_all(&spacetimedb_dir).unwrap();

        let resolved = default_publish_module_path(&cwd);
        assert_eq!(resolved, spacetimedb_dir);
    }

    #[test]
    fn test_default_publish_module_path_falls_back_to_current_dir() {
        let temp = tempfile::TempDir::new().unwrap();
        let cwd = temp.path().to_path_buf();

        let resolved = default_publish_module_path(&cwd);
        assert_eq!(resolved, cwd);
    }

    #[test]
    fn test_is_maincloud_host_true_for_maincloud_url() {
        assert!(is_maincloud_host("https://maincloud.spacetimedb.com"));
    }

    #[test]
    fn test_is_maincloud_host_false_for_non_maincloud_url() {
        assert!(!is_maincloud_host("http://localhost:3000"));
        assert!(!is_maincloud_host("https://testnet.spacetimedb.com"));
    }

    #[test]
    fn test_glob_filter_matches_pattern() {
        use std::collections::HashMap;

        let cmd = cli();
        let schema = build_publish_schema(&cmd).unwrap();

        let mut parent_fields = HashMap::new();
        parent_fields.insert("server".to_string(), serde_json::json!("local"));

        let spacetime_config = make_config_with_children(
            parent_fields,
            vec![
                make_config({
                    let mut m = HashMap::new();
                    m.insert("database".to_string(), serde_json::json!("region-1"));
                    m
                }),
                make_config({
                    let mut m = HashMap::new();
                    m.insert("database".to_string(), serde_json::json!("region-2"));
                    m
                }),
                make_config({
                    let mut m = HashMap::new();
                    m.insert("database".to_string(), serde_json::json!("global"));
                    m
                }),
            ],
        );

        // Glob: region-* should match region-1 and region-2 but not global
        let matches = cmd.clone().get_matches_from(vec!["publish", "region-*"]);
        let filtered = get_filtered_publish_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_publish_filter_inherits_parent_fields() {
        use std::collections::HashMap;

        let cmd = cli();
        let schema = build_publish_schema(&cmd).unwrap();

        let mut parent_fields = HashMap::new();
        parent_fields.insert("database".to_string(), serde_json::json!("parent-db"));
        parent_fields.insert("server".to_string(), serde_json::json!("local"));

        let mut child_fields = HashMap::new();
        child_fields.insert("database".to_string(), serde_json::json!("child-db"));
        // child does NOT set "server" — should inherit from parent

        let spacetime_config = make_config_with_children(parent_fields, vec![make_config(child_fields)]);

        // Filter to the child target
        let matches = cmd.clone().get_matches_from(vec!["publish", "child-db"]);
        let filtered = get_filtered_publish_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        assert_eq!(filtered.len(), 1);
        // The child should have inherited "server" from the parent
        assert_eq!(
            filtered[0].get_one::<String>("server").unwrap(),
            Some("local".to_string())
        );
    }

    #[test]
    fn test_glob_star_matches_all() {
        use std::collections::HashMap;

        let cmd = cli();
        let schema = build_publish_schema(&cmd).unwrap();

        let spacetime_config = make_config_with_children(
            HashMap::new(),
            vec![
                make_config({
                    let mut m = HashMap::new();
                    m.insert("database".to_string(), serde_json::json!("alpha"));
                    m
                }),
                make_config({
                    let mut m = HashMap::new();
                    m.insert("database".to_string(), serde_json::json!("beta"));
                    m
                }),
                make_config({
                    let mut m = HashMap::new();
                    m.insert("database".to_string(), serde_json::json!("gamma"));
                    m
                }),
            ],
        );

        // Glob: * should match all databases
        let matches = cmd.clone().get_matches_from(vec!["publish", "*"]);
        let filtered = get_filtered_publish_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn test_glob_exact_match() {
        use std::collections::HashMap;

        let cmd = cli();
        let schema = build_publish_schema(&cmd).unwrap();

        let spacetime_config = make_config_with_children(
            HashMap::new(),
            vec![
                make_config({
                    let mut m = HashMap::new();
                    m.insert("database".to_string(), serde_json::json!("region-1"));
                    m
                }),
                make_config({
                    let mut m = HashMap::new();
                    m.insert("database".to_string(), serde_json::json!("region-2"));
                    m
                }),
            ],
        );

        // Exact match should return only one
        let matches = cmd.clone().get_matches_from(vec!["publish", "region-1"]);
        let filtered = get_filtered_publish_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].get_one::<String>("database").unwrap(),
            Some("region-1".to_string())
        );
    }

    #[test]
    fn test_glob_multiple_wildcards() {
        use std::collections::HashMap;

        let cmd = cli();
        let schema = build_publish_schema(&cmd).unwrap();

        let spacetime_config = make_config_with_children(
            HashMap::new(),
            vec![
                make_config({
                    let mut m = HashMap::new();
                    m.insert("database".to_string(), serde_json::json!("us-east-prod"));
                    m
                }),
                make_config({
                    let mut m = HashMap::new();
                    m.insert("database".to_string(), serde_json::json!("us-west-prod"));
                    m
                }),
                make_config({
                    let mut m = HashMap::new();
                    m.insert("database".to_string(), serde_json::json!("eu-east-staging"));
                    m
                }),
                make_config({
                    let mut m = HashMap::new();
                    m.insert("database".to_string(), serde_json::json!("us-east-staging"));
                    m
                }),
            ],
        );

        // Pattern with multiple wildcards: *-east-*
        let matches = cmd.clone().get_matches_from(vec!["publish", "*-east-*"]);
        let filtered = get_filtered_publish_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        assert_eq!(filtered.len(), 3); // us-east-prod, eu-east-staging, us-east-staging
    }

    #[test]
    fn test_glob_empty_pattern_error() {
        use std::collections::HashMap;

        let cmd = cli();
        let schema = build_publish_schema(&cmd).unwrap();

        let spacetime_config = make_config_with_children(
            HashMap::new(),
            vec![make_config({
                let mut m = HashMap::new();
                m.insert("database".to_string(), serde_json::json!("my-db"));
                m
            })],
        );

        // Empty string as pattern — won't match anything
        let matches = cmd.clone().get_matches_from(vec!["publish", ""]);
        let result = get_filtered_publish_configs(&spacetime_config, &cmd, &schema, &matches);
        assert!(result.is_err());
    }
}
