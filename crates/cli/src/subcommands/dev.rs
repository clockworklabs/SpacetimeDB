use crate::common_args::ClearMode;
use crate::config::Config;
use crate::generate::Language;
use crate::spacetime_config::{detect_client_command, CommandConfig, CommandSchema, SpacetimeConfig};
use crate::subcommands::init;
use crate::util::{
    add_auth_header_opt, database_identity, detect_module_language, get_auth_header, get_login_token_or_log_in,
    spacetime_reverse_dns, ResponseExt,
};
use crate::{common_args, generate};
use crate::{publish, tasks};
use anyhow::Context;
use clap::{Arg, ArgMatches, Command};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect};
use futures::stream::{self, StreamExt};
use futures::{AsyncBufReadExt, TryStreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use serde::Deserialize;
use serde_json::json;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;
use tabled::{
    settings::{object::Columns, Alignment, Modify, Style},
    Table, Tabled,
};
use termcolor::{Color, ColorSpec, WriteColor};
use tokio::process::{Child, Command as TokioCommand};
use tokio::task::JoinHandle;
use tokio::time::sleep;

pub fn cli() -> Command {
    Command::new("dev")
        .about("Start development mode with auto-regenerate client module bindings, auto-rebuild, and auto-publish on file changes.")
        .arg(
            Arg::new("database")
                .help("The database name/identity to publish to (optional, will prompt if not provided)"),
        )
        // Deprecated: --database flag for backwards compatibility
        .arg(
            Arg::new("database-flag")
                .long("database")
                .hide(true)
                .help("DEPRECATED: Use positional argument instead"),
        )
        .arg(
            Arg::new("project-path")
                .long("project-path")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(".")
                .help("The path to the project directory"),
        )
        .arg(
            Arg::new("module-bindings-path")
                .long("module-bindings-path")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value("src/module_bindings")
                .help("The path to the module bindings directory relative to the project directory, defaults to `<project-path>/src/module_bindings`"),
        )
        // NOTE: All server templates must have their server code in `spacetimedb/` directory
        // This is not a requirement in general, but is a requirement for all templates 
        // i.e. `spacetime dev` is valid on non-templates.
        .arg(
            Arg::new("module-project-path")
                .long("module-project-path")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value("spacetimedb")
                .help("The path to the SpacetimeDB server module project relative to the project directory, defaults to `<project-path>/spacetimedb`"),
        )
        .arg(
            Arg::new("client-lang")
                .long("client-lang")
                .value_parser(clap::value_parser!(Language))
                .help("The programming language for the generated client module bindings (e.g., typescript, csharp, python). If not specified, it will be detected from the project."),
        )
        .arg(common_args::server().help("The nickname, host name or URL of the server to publish to"))
        .arg(common_args::yes())
        .arg(common_args::clear_database())
        .arg(
            Arg::new("template")
                .short('t')
                .long("template")
                .value_name("TEMPLATE")
                .help("Template ID or GitHub repository (owner/repo or URL) for project initialization"),
        )
        .arg(
            Arg::new("run")
                .long("run")
                .value_name("COMMAND")
                .help("Command to run the client development server (overrides spacetime.json config)"),
        )
        .arg(
            Arg::new("server-only")
                .long("server-only")
                .action(clap::ArgAction::SetTrue)
                .help("Only run the server (module) without starting the client"),
        )
}

#[derive(Deserialize)]
struct DatabasesResult {
    pub identities: Vec<String>,
}

#[derive(Tabled, Clone)]
struct DatabaseRow {
    pub identity: String,
    pub name: String,
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let project_path = args.get_one::<PathBuf>("project-path").unwrap();
    let spacetimedb_project_path = args.get_one::<PathBuf>("module-project-path").unwrap();
    let module_bindings_path = args.get_one::<PathBuf>("module-bindings-path").unwrap();
    let client_language = args.get_one::<Language>("client-lang");
    let clear_database = args
        .get_one::<ClearMode>("clear-database")
        .copied()
        .unwrap_or(ClearMode::OnConflict);
    let force = args.get_flag("force");

    // If you don't specify a server, we default to your default server
    // If you don't have one of those, we default to "maincloud"
    let server_from_cli = args.get_one::<String>("server").map(|s| s.as_str());

    let default_server_name = config.default_server_name().map(|s| s.to_string());

    let mut resolved_server = server_from_cli
        .or(default_server_name.as_deref())
        .ok_or_else(|| anyhow::anyhow!("Server not specified and no default server configured."))?;

    let mut project_dir = project_path.clone();

    if module_bindings_path.is_absolute() {
        anyhow::bail!("Module bindings path must be a relative path");
    }
    let mut module_bindings_dir = project_dir.join(module_bindings_path);

    if spacetimedb_project_path.is_absolute() {
        anyhow::bail!("SpacetimeDB project path must be a relative path");
    }
    let mut spacetimedb_dir = project_dir.join(spacetimedb_project_path);

    // Load spacetime.json config early so we can use it for determining project
    // directories
    let spacetime_config = match SpacetimeConfig::load_from_dir(&project_dir) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{} Failed to load spacetime.json: {}", "✗".red(), e);
            std::process::exit(1);
        }
    };

    // Fetch the database name if it was passed through a CLI arg
    let database_name_from_cli: Option<String> = args
        .get_one::<String>("database")
        .or_else(|| args.get_one::<String>("database-flag"))
        .map(|name| {
            if args.get_one::<String>("database-flag").is_some() {
                println!(
                    "{} {}",
                    "Warning:".yellow().bold(),
                    "--database flag is deprecated. Use positional argument instead: spacetime dev <database>".dimmed()
                );
            }
            name.clone()
        });

    // Build publish configs. It is easier to work with one type of data,
    // so if we don't have publish configs from the config file, we build a single
    // publish config based on the CLI args
    let publish_cmd = publish::cli();
    let publish_schema = publish::build_publish_schema(&publish_cmd)?;

    // Create ArgMatches for publish command to use with get_one()
    let mut publish_argv: Vec<String> = vec!["publish".to_string()];
    if let Some(db) = &database_name_from_cli {
        publish_argv.push(db.clone());
    }
    if let Some(srv) = args.get_one::<String>("server") {
        publish_argv.push(srv.clone());
    }

    let publish_args = publish_cmd
        .clone()
        .try_get_matches_from(publish_argv)
        .context("Failed to create publish arguments")?;

    let mut publish_configs = determine_publish_configs(
        database_name_from_cli,
        spacetime_config.as_ref(),
        &publish_schema,
        &publish_args,
        resolved_server,
    )?;

    // Check if we are in a SpacetimeDB project directory, but only if we don't have any
    // publish_configs that would specify desired modules
    if publish_configs.is_empty() && (!spacetimedb_dir.exists() || !spacetimedb_dir.is_dir()) {
        println!("{}", "No SpacetimeDB project found in current directory.".yellow());
        let should_init = Confirm::new()
            .with_prompt("Would you like to initialize a new project?")
            .default(true)
            .interact()?;

        if should_init {
            let mut init_argv = vec!["init"];
            if resolved_server == "local" {
                init_argv.push("--local");
            }
            let template = args.get_one::<String>("template");
            if let Some(template_str) = template {
                init_argv.push("--template");
                init_argv.push(template_str);
            }
            let init_args = init::cli().get_matches_from(init_argv);
            let created_project_path = init::exec(config.clone(), &init_args).await?;

            let canonical_created_path = created_project_path
                .canonicalize()
                .context("Failed to canonicalize created project path")?;
            spacetimedb_dir = canonical_created_path.join(spacetimedb_project_path);
            module_bindings_dir = canonical_created_path.join(module_bindings_path);
            project_dir = canonical_created_path;

            if !spacetimedb_dir.exists() {
                anyhow::bail!("Project initialization did not create spacetimedb directory");
            }
        } else {
            anyhow::bail!("Not in a SpacetimeDB project directory");
        }
    } else if args.get_one::<String>("template").is_some() {
        println!(
            "{}",
            "Warning: --template option is ignored because a SpacetimeDB project already exists.".yellow()
        );
    }

    if let Some(config) = publish_configs.first() {
        // if we have publish configs and we're past spacetimedb_dir manipulation,
        // we should set spacetimedb_dir to the path of the first config as this will be
        // later used for next steps
        spacetimedb_dir = config.get_one::<PathBuf>("module_path").expect("module_path");
    }

    let use_local = resolved_server == "local";

    // If we don't have any publish configs by now, we need to ask the user about the
    // database they want to use. This should only happen if no configs are available
    // in the config file and no database name has been passed through the CLI
    if publish_configs.is_empty() {
        println!("\n{}", "Found existing SpacetimeDB project.".green());
        println!("Now we need to select a database to publish to.\n");

        let selected = if use_local {
            generate_database_name()
        } else {
            // If not logged in before, but login was successful just now, this will have the token
            let token = get_login_token_or_log_in(&mut config, Some(resolved_server), !force).await?;

            let choice = FuzzySelect::with_theme(&ColorfulTheme::default())
                .with_prompt("Database selection")
                .items(&["Create new database with random name", "Select from existing databases"])
                .default(0)
                .interact()?;

            if choice == 0 {
                generate_database_name()
            } else {
                select_database(&config, resolved_server, &token).await?
            }
        };

        println!("\n{} {}", "Selected database:".green().bold(), selected.cyan());
        println!(
            "{} {}",
            "Tip:".yellow().bold(),
            format!("Use `spacetime dev {}` to skip this question next time", selected).dimmed()
        );

        let mut config_map = HashMap::new();
        config_map.insert("database".to_string(), json!(selected));
        config_map.insert("server".to_string(), json!(resolved_server));

        publish_configs = vec![CommandConfig::new(&publish_schema, config_map, &publish_args)?];
    }

    if !module_bindings_dir.exists() {
        // Create the module bindings directory if it doesn't exist
        std::fs::create_dir_all(&module_bindings_dir).with_context(|| {
            format!(
                "Failed to create module bindings path {}",
                module_bindings_dir.display()
            )
        })?;
    } else if !module_bindings_dir.is_dir() {
        anyhow::bail!(
            "Module bindings path {} exists but is not a directory.",
            module_bindings_path.display()
        );
    }

    // Check if we need to login to maincloud
    // Either because --server maincloud was provided, or because any of the publish configs use maincloud
    let needs_maincloud_login = resolved_server == "maincloud"
        || spacetime_config
            .as_ref()
            .and_then(|c| c.publish.as_ref())
            .map(|publish| {
                publish.iter_all_targets().any(|target| {
                    target
                        .additional_fields
                        .get("server")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "maincloud")
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

    if needs_maincloud_login && config.spacetimedb_token().is_none() {
        let should_login = Confirm::new()
            .with_prompt("Would you like to sign in now?")
            .default(true)
            .interact()?;
        if !should_login && server_from_cli.is_some() {
            // The user explicitly provided --server maincloud but doesn't want to log in
            anyhow::bail!("Login required to publish to maincloud server");
        } else if !should_login {
            // Print warning saying that without logging in we will use local server regardless
            // of what their default server is in their config
            println!(
                "{} {}",
                "Warning:".yellow().bold(),
                "Without logging in, the local server will be used regardless of your default server.".dimmed()
            );
            // Switch the server to local
            resolved_server = "local";
        } else {
            // Login
            get_login_token_or_log_in(&mut config, Some(resolved_server), !force).await?;
        }
    }

    // Determine client command: CLI flag > config file > auto-detect (and save)
    let server_only = args.get_flag("server-only");

    let client_command = if server_only {
        None
    } else if let Some(cmd) = args.get_one::<String>("run") {
        // Explicit CLI flag takes priority
        Some(cmd.clone())
    } else {
        // Try to load config, handling errors properly
        match SpacetimeConfig::load_from_dir(&project_dir) {
            Ok(Some(config)) => {
                // Config file exists and parsed successfully
                let config_path = project_dir.join("spacetime.json");
                println!("{} Using configuration from {}", "✓".green(), config_path.display());

                // If config exists but dev.run is None, try to detect and update
                if config.dev.as_ref().and_then(|d| d.run.as_ref()).is_none() {
                    detect_and_save_client_command(&project_dir, Some(config))
                } else {
                    config.dev.and_then(|d| d.run)
                }
            }
            Ok(None) => {
                // No config file - try to detect and create new
                detect_and_save_client_command(&project_dir, None)
            }
            Err(e) => {
                // Config file exists but failed to parse - show error and exit
                eprintln!("{} Failed to load spacetime.json: {}", "✗".red(), e);
                std::process::exit(1);
            }
        }
    };

    // Extract database names from publish configs for log streaming
    let db_names_for_logging: Vec<String> = publish_configs
        .iter()
        .map(|config| {
            config
                .get_config_value("database")
                .and_then(|v| v.as_str())
                .expect("database is a required field")
                .to_string()
        })
        .collect();

    // Use first database for client process
    let db_name_for_client = &db_names_for_logging[0];

    // Extract watch directories from publish configs
    let watch_dirs = extract_watch_dirs(&publish_configs, &spacetimedb_dir);

    println!("\n{}", "Starting development mode...".green().bold());
    if db_names_for_logging.len() == 1 {
        println!("Database: {}", db_names_for_logging[0].cyan());
    } else {
        println!("Databases: {}", db_names_for_logging.join(", ").cyan());
    }

    // Announce watch directories
    if watch_dirs.len() == 1 {
        println!(
            "Watching for changes in: {}",
            watch_dirs.iter().next().unwrap().display().to_string().cyan()
        );
    } else {
        let watch_dirs_vec: Vec<_> = watch_dirs.iter().collect();
        println!("Watching for changes in {} directories:", watch_dirs.len());
        for dir in &watch_dirs_vec {
            println!("  - {}", dir.display().to_string().cyan());
        }
    }

    if let Some(ref cmd) = client_command {
        println!("Client command: {}", cmd.cyan());
    }
    println!("{}", "Press Ctrl+C to stop".dimmed());
    println!();

    generate_build_and_publish(
        &config,
        &project_dir,
        &spacetimedb_dir,
        &module_bindings_dir,
        client_language,
        clear_database,
        &publish_configs,
        server_from_cli,
    )
    .await?;

    // Sleep for a second to allow the database to be published on Maincloud
    sleep(Duration::from_secs(1)).await;

    // Start log streams for all targets
    let use_prefix = db_names_for_logging.len() > 1;
    let mut log_handles = Vec::new();
    for config_entry in &publish_configs {
        let db_name = config_entry
            .get_config_value("database")
            .and_then(|v| v.as_str())
            .expect("database is a required field");

        let server_opt = config_entry.get_one::<String>("server")?;
        let server_for_db = server_opt.as_deref().unwrap_or(resolved_server);

        let db_identity = database_identity(&config, db_name, Some(server_for_db)).await?;
        let prefix = if use_prefix { Some(db_name.to_string()) } else { None };
        let handle = start_log_stream(
            config.clone(),
            db_identity.to_hex().to_string(),
            Some(server_for_db),
            prefix,
        )
        .await?;
        log_handles.push(handle);
    }

    // Start the client development server if configured
    let server_opt_client = publish_configs
        .first()
        .and_then(|c| c.get_one::<String>("server").ok().flatten());
    let server_for_client = server_opt_client.as_deref().unwrap_or(resolved_server);
    let server_host_url = config.get_host_url(Some(server_for_client))?;
    let _client_handle = if let Some(ref cmd) = client_command {
        let mut child = start_client_process(cmd, &project_dir, db_name_for_client, &server_host_url)?;

        // Give the process a moment to fail fast (e.g., command not found, missing deps)
        sleep(Duration::from_millis(200)).await;
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => {
                anyhow::bail!(
                    "Client command '{}' failed immediately with exit code: {}",
                    cmd,
                    status
                        .code()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                );
            }
            Err(e) => {
                anyhow::bail!("Failed to check client process status: {}", e);
            }
            _ => {} // Still running or exited successfully (unusual but ok)
        }
        Some(child)
    } else {
        None
    };

    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    notify::EventKind::Modify(_) | notify::EventKind::Create(_) | notify::EventKind::Remove(_)
                ) {
                    let _ = tx.send(());
                }
            }
        },
        notify::Config::default().with_poll_interval(Duration::from_millis(500)),
    )?;

    // Watch all directories
    for watch_dir in &watch_dirs {
        watcher.watch(watch_dir, RecursiveMode::Recursive)?;
    }

    let mut debounce_timer;
    loop {
        if rx.recv().is_ok() {
            debounce_timer = std::time::Instant::now();
            while debounce_timer.elapsed() < Duration::from_millis(300) {
                if rx.recv_timeout(Duration::from_millis(100)).is_ok() {
                    debounce_timer = std::time::Instant::now();
                }
            }

            println!("\n{}", "File change detected, rebuilding...".yellow());
            match generate_build_and_publish(
                &config,
                &project_dir,
                &spacetimedb_dir,
                &module_bindings_dir,
                client_language,
                clear_database,
                &publish_configs,
                server_from_cli,
            )
            .await
            {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("{} {}", "Error:".red().bold(), e);
                    println!("{}", "Waiting for next change...".dimmed());
                }
            }
        }
    }
}

fn determine_publish_configs<'a>(
    database_name: Option<String>,
    spacetime_config: Option<&'a SpacetimeConfig>,
    publish_schema: &'a CommandSchema,
    publish_args: &'a ArgMatches,
    resolved_server: &str,
) -> anyhow::Result<Vec<CommandConfig<'a>>> {
    // Build publish configs. It is easier to work with one type of data,
    // so if we don't have publish configs from the config file, we build a single
    // publish config based on the CLI args
    let mut publish_configs: Vec<CommandConfig> = vec![];

    if let Some(config) = spacetime_config {
        // Get and filter publish configs
        if config.publish.is_some() {
            publish_configs = publish::get_filtered_publish_configs(config, publish_schema, publish_args)?;
        }
    }

    if !publish_configs.is_empty() {
        return Ok(publish_configs);
    }

    // If we still have no configs, it means that filtering by the database name filtered out
    // all configs, we assume the user wants to run with a different DB
    if let Some(ref db_name) = database_name {
        let mut config_map = HashMap::new();
        config_map.insert("database".to_string(), json!(db_name));
        config_map.insert("server".to_string(), json!(resolved_server));
        config_map.insert("module-path".to_string(), json!("spacetimedb"));

        Ok(vec![CommandConfig::new(publish_schema, config_map, publish_args)?])
    } else {
        // If there is no provided database name nor publish configs return no
        // configs, we will handle it by asking user for a database or auto-generate one
        Ok(vec![])
    }
}

/// Upserts all SPACETIMEDB_DB_NAME and SPACETIMEDB_HOST variants into `.env.local`,
/// preserving comments/formatting and leaving unrelated keys unchanged.
fn upsert_env_db_names_and_hosts(env_path: &Path, server_host_url: &str, database_name: &str) -> anyhow::Result<()> {
    // Framework-agnostic variants (same list for both DB_NAME and HOST)
    let prefixes = [
        "SPACETIMEDB",             // generic / backend
        "VITE_SPACETIMEDB",        // Vite
        "NEXT_PUBLIC_SPACETIMEDB", // Next.js
        "REACT_APP_SPACETIMEDB",   // CRA
        "EXPO_PUBLIC_SPACETIMEDB", // Expo
        "PUBLIC_SPACETIMEDB",      // SvelteKit
    ];

    let mut contents = if env_path.exists() {
        fs::read_to_string(env_path)?
    } else {
        String::new()
    };
    let original_contents = contents.clone();

    for prefix in prefixes {
        for (suffix, value) in [("DB_NAME", database_name), ("HOST", server_host_url)] {
            let key = format!("{prefix}_{suffix}");
            let re = Regex::new(&format!(r"(?m)^(?P<prefix>\s*{key}\s*=\s*)(?P<val>.*)$"))?;
            if re.is_match(&contents) {
                contents = re.replace_all(&contents, format!("${{prefix}}{value}")).to_string();
            } else {
                if !contents.is_empty() && !contents.ends_with('\n') {
                    contents.push('\n');
                }
                contents.push_str(&format!("{key}={value}\n"));
            }
        }
    }

    if !contents.ends_with('\n') {
        contents.push('\n');
    }

    if contents != original_contents {
        fs::write(env_path, contents)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn generate_build_and_publish(
    config: &Config,
    project_dir: &Path,
    spacetimedb_dir: &Path,
    module_bindings_dir: &Path,
    client_language: Option<&Language>,
    clear_database: ClearMode,
    publish_configs: &[CommandConfig<'_>],
    server: Option<&str>,
) -> Result<(), anyhow::Error> {
    let module_language = detect_module_language(spacetimedb_dir)?;
    let client_language = client_language.unwrap_or(match module_language {
        crate::util::ModuleLanguage::Rust => &Language::Rust,
        crate::util::ModuleLanguage::Csharp => &Language::Csharp,
        crate::util::ModuleLanguage::Javascript => &Language::TypeScript,
    });
    let client_language_str = match client_language {
        Language::Rust => "rust",
        Language::Csharp => "csharp",
        Language::TypeScript => "typescript",
        Language::UnrealCpp => "unrealcpp",
    };

    // For TypeScript client, update .env.local with first database name
    if client_language == &Language::TypeScript {
        let first_config = publish_configs.first().expect("publish_configs cannot be empty");
        let first_db_name = first_config
            .get_config_value("database")
            .and_then(|v| v.as_str())
            .expect("database is a required field");

        // CLI server takes precedence, otherwise use server from config
        let server_for_env = server.or_else(|| first_config.get_config_value("server").and_then(|v| v.as_str()));

        println!(
            "{} {}...",
            "Updating .env.local with database name".cyan(),
            first_db_name
        );
        let env_path = project_dir.join(".env.local");
        let server_host_url = config.get_host_url(server_for_env)?;
        upsert_env_db_names_and_hosts(&env_path, &server_host_url, first_db_name)?;
    }

    println!("{}", "Building...".cyan());
    let (_path_to_program, _host_type) =
        tasks::build(spacetimedb_dir, Some(Path::new("src")), false, None).context("Failed to build project")?;
    println!("{}", "Build complete!".green());

    println!("{}", "Generating module bindings...".cyan());
    let generate_args = generate::cli().get_matches_from(vec![
        "generate",
        "--lang",
        client_language_str,
        "--project-path",
        spacetimedb_dir.to_str().unwrap(),
        "--out-dir",
        module_bindings_dir.to_str().unwrap(),
    ]);
    generate::exec_ex(
        config.clone(),
        &generate_args,
        crate::generate::extract_descriptions,
        true,
    )
    .await?;

    println!("{}", "Publishing...".cyan());

    let project_path_str = spacetimedb_dir.to_str().unwrap();
    let clear_flag = match clear_database {
        ClearMode::Always => "always",
        ClearMode::Never => "never",
        ClearMode::OnConflict => "on-conflict",
    };

    // Loop through all publish configs
    for config_entry in publish_configs {
        let db_name = config_entry
            .get_config_value("database")
            .and_then(|v| v.as_str())
            .expect("database is a required field");

        if publish_configs.len() > 1 {
            println!("{} {}...", "Publishing to".cyan(), db_name.cyan().bold());
        }

        let mut publish_args = vec![
            "publish".to_string(),
            db_name.to_string(),
            "--project-path".to_string(),
            project_path_str.to_string(),
            "--yes".to_string(),
            format!("--delete-data={}", clear_flag),
        ];

        // Only pass --server if it was explicitly provided via CLI
        if let Some(srv) = server {
            publish_args.extend_from_slice(&["--server".to_string(), srv.to_string()]);
        }

        let publish_cmd = publish::cli();
        let publish_matches = publish_cmd
            .try_get_matches_from(publish_args)
            .context("Failed to create publish arguments")?;

        publish::exec_with_options(config.clone(), &publish_matches, true).await?;
    }

    println!("{}", "Published successfully!".green().bold());
    println!("{}", "---".dimmed());

    Ok(())
}

async fn select_database(config: &Config, server: &str, token: &str) -> Result<String, anyhow::Error> {
    let identity = crate::util::decode_identity(&token.to_string())?;

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message("Fetching database list...");
    spinner.enable_steady_tick(Duration::from_millis(100));

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{}/v1/identity/{}/databases",
            config.get_host_url(Some(server))?,
            identity
        ))
        .bearer_auth(token)
        .send()
        .await?;

    let result: DatabasesResult = res
        .json_or_error()
        .await
        .context("Unable to retrieve databases for identity")?;

    if result.identities.is_empty() {
        spinner.finish_and_clear();
        println!("{}", "No existing databases found.".yellow());
        Ok(generate_database_name())
    } else {
        let total = result.identities.len();
        spinner.set_message(format!("Fetching names for {} databases...", total));

        // Fetch database names with HTTP queries to /database/{identity}/names
        // It's parallelyzed in case a user has a lot of databases
        // TODO: we should introduce an endpoint that returns user's databases with names
        let databases: Vec<DatabaseRow> = stream::iter(result.identities.into_iter())
            .map(|identity_str| {
                let config = config.clone();
                async move {
                    let names_response = spacetime_reverse_dns(&config, &identity_str, Some(server)).await?;
                    let name = if names_response.names.is_empty() {
                        identity_str.clone()
                    } else {
                        names_response.names[0].as_ref().to_string()
                    };
                    Ok::<DatabaseRow, anyhow::Error>(DatabaseRow {
                        identity: identity_str,
                        name,
                    })
                }
            })
            .buffer_unordered(30)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        spinner.finish_and_clear();

        let display_limit = 10;
        if databases.len() <= display_limit {
            let mut table = Table::new(&databases);
            table
                .with(Style::psql())
                .with(Modify::new(Columns::first()).with(Alignment::left()));
            println!("\nYour databases:\n");
            println!("{table}");
            println!();
        } else {
            let display_databases: Vec<_> = databases.iter().take(display_limit).cloned().collect();
            let mut table = Table::new(&display_databases);
            table
                .with(Style::psql())
                .with(Modify::new(Columns::first()).with(Alignment::left()));
            println!("\nYour databases (showing {} of {}):\n", display_limit, databases.len());
            println!("{table}");
            println!();
        }

        let items: Vec<String> = databases
            .iter()
            .map(|db| {
                let truncated_identity = truncate_identity(&db.identity);
                format!("{} ({})", db.name, truncated_identity)
            })
            .collect();

        let selection = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Select database (type to filter)")
            .items(&items)
            .default(0)
            .interact()?;

        Ok(databases[selection].name.clone())
    }
}

fn truncate_identity(identity: &str) -> String {
    if identity.len() <= 16 {
        identity.to_string()
    } else {
        format!("{}...{}", &identity[..8], &identity[identity.len() - 8..])
    }
}

async fn start_log_stream(
    mut config: Config,
    database_identity: String,
    server: Option<&str>,
    prefix: Option<String>,
) -> Result<JoinHandle<()>, anyhow::Error> {
    let server = server.map(|s| s.to_string());
    let host_url = config.get_host_url(server.as_deref())?;
    let auth_header = get_auth_header(&mut config, false, server.as_deref(), false).await?;

    let handle = tokio::spawn(async move {
        loop {
            if let Err(e) = stream_logs(&host_url, &database_identity, &auth_header, prefix.as_deref()).await {
                eprintln!("\n{} Log streaming error: {}", "Error:".red().bold(), e);
                eprintln!("{}", "Reconnecting in 10 seconds...".yellow());
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
    });

    Ok(handle)
}

async fn stream_logs(
    host_url: &str,
    database_identity: &str,
    auth_header: &crate::util::AuthHeader,
    prefix: Option<&str>,
) -> Result<(), anyhow::Error> {
    let client = reqwest::Client::new();
    let builder = client.get(format!("{host_url}/v1/database/{database_identity}/logs"));
    let builder = add_auth_header_opt(builder, auth_header);
    let res = builder.query(&[("num_lines", "10"), ("follow", "true")]).send().await?;

    let status = res.status();
    if status.is_client_error() || status.is_server_error() {
        let err = res.text().await?;
        anyhow::bail!(err)
    }

    let term_color = if std::io::stdout().is_terminal() {
        termcolor::ColorChoice::Auto
    } else {
        termcolor::ColorChoice::Never
    };

    let mut rdr = res.bytes_stream().map_err(std::io::Error::other).into_async_read();
    let mut line = String::new();
    while rdr.read_line(&mut line).await? != 0 {
        let record = serde_json::from_str::<LogRecord<'_>>(&line)?;
        let out = termcolor::StandardStream::stdout(term_color);
        let mut out = out.lock();
        format_log_record(&mut out, &record, prefix)?;
        drop(out);
        line.clear();
    }

    Ok(())
}

const SENTINEL: &str = "__spacetimedb__";

#[derive(serde::Deserialize)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Panic,
}

#[serde_with::serde_as]
#[derive(serde::Deserialize)]
struct LogRecord<'a> {
    #[serde_as(as = "Option<serde_with::TimestampMicroSeconds>")]
    ts: Option<chrono::DateTime<chrono::Utc>>,
    level: LogLevel,
    #[serde(borrow)]
    #[allow(unused)]
    target: Option<Cow<'a, str>>,
    #[serde(borrow)]
    filename: Option<Cow<'a, str>>,
    line_number: Option<u32>,
    #[serde(borrow)]
    function: Option<Cow<'a, str>>,
    #[serde(borrow)]
    message: Cow<'a, str>,
}

fn format_log_record<W: WriteColor>(
    out: &mut W,
    record: &LogRecord<'_>,
    prefix: Option<&str>,
) -> Result<(), std::io::Error> {
    // Write prefix if provided
    if let Some(prefix) = prefix {
        out.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        write!(out, "[{}] ", prefix)?;
        out.reset()?;
    }

    if let Some(ts) = record.ts {
        out.set_color(ColorSpec::new().set_dimmed(true))?;
        write!(out, "{ts:?} ")?;
    }
    let mut color = ColorSpec::new();
    let level = match record.level {
        LogLevel::Error => {
            color.set_fg(Some(Color::Red));
            "ERROR"
        }
        LogLevel::Warn => {
            color.set_fg(Some(Color::Yellow));
            "WARN"
        }
        LogLevel::Info => {
            color.set_fg(Some(Color::Blue));
            "INFO"
        }
        LogLevel::Debug => {
            color.set_dimmed(true).set_bold(true);
            "DEBUG"
        }
        LogLevel::Trace => {
            color.set_dimmed(true);
            "TRACE"
        }
        LogLevel::Panic => {
            color.set_fg(Some(Color::Red)).set_bold(true).set_intense(true);
            "PANIC"
        }
    };
    out.set_color(&color)?;
    write!(out, "{level:>5}: ")?;
    out.reset()?;
    let mut need_space_before_filename = false;
    let mut need_colon_sep = false;
    let dimmed = ColorSpec::new().set_dimmed(true).clone();
    if let Some(function) = &record.function {
        if function.as_ref() != SENTINEL {
            out.set_color(&dimmed)?;
            write!(out, "{function}")?;
            out.reset()?;
            need_space_before_filename = true;
            need_colon_sep = true;
        }
    }
    if let Some(filename) = &record.filename {
        if filename.as_ref() != SENTINEL {
            out.set_color(&dimmed)?;
            if need_space_before_filename {
                write!(out, " ")?;
            }
            write!(out, "{filename}")?;
            if let Some(line) = record.line_number {
                write!(out, ":{line}")?;
            }
            out.reset()?;
            need_colon_sep = true;
        }
    }
    if need_colon_sep {
        write!(out, ": ")?;
    }
    writeln!(out, "{}", record.message)?;
    Ok(())
}

fn generate_database_name() -> String {
    let mut generator = names::Generator::with_naming(names::Name::Numbered);
    generator.next().unwrap()
}

/// Extract unique watch directories from publish configs
fn extract_watch_dirs(
    publish_configs: &[CommandConfig<'_>],
    default_spacetimedb_dir: &Path,
) -> std::collections::HashSet<PathBuf> {
    use std::collections::HashSet;
    let mut watch_dirs = HashSet::new();

    for config_entry in publish_configs {
        let module_path = config_entry
            .get_config_value("module_path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_spacetimedb_dir.to_path_buf());

        // Canonicalize to handle relative paths
        let canonical_path = module_path.canonicalize().unwrap_or(module_path);

        watch_dirs.insert(canonical_path);
    }

    watch_dirs
}

/// Detect client command and save to config (updating existing config if present)
fn detect_and_save_client_command(project_dir: &Path, existing_config: Option<SpacetimeConfig>) -> Option<String> {
    if let Some((detected_cmd, _detected_pm)) = detect_client_command(project_dir) {
        // Update existing config or create new one
        let config_to_save = if let Some(mut config) = existing_config {
            config.dev = Some(crate::spacetime_config::DevConfig {
                run: Some(detected_cmd.clone()),
            });
            config
        } else {
            SpacetimeConfig::with_run_command(&detected_cmd)
        };

        if let Ok(path) = config_to_save.save_to_dir(project_dir) {
            println!(
                "{} Detected client command and saved to {}",
                "✓".green(),
                path.display()
            );
        }
        Some(detected_cmd)
    } else {
        None
    }
}

/// Start the client development server as a child process.
/// The process inherits stdout/stderr so the user can see the output.
/// Sets SPACETIMEDB_DB_NAME and SPACETIMEDB_HOST environment variables for the client.
fn start_client_process(
    command: &str,
    working_dir: &Path,
    database_name: &str,
    host_url: &str,
) -> Result<Child, anyhow::Error> {
    println!("{} {}", "Starting client:".cyan(), command.dimmed());

    if command.trim().is_empty() {
        anyhow::bail!("Empty client command");
    }

    // Use shell to handle PATH resolution and .cmd/.bat scripts on Windows
    #[cfg(windows)]
    let child = TokioCommand::new("cmd")
        .args(["/C", command])
        .current_dir(working_dir)
        .env("SPACETIMEDB_DB_NAME", database_name)
        .env("SPACETIMEDB_HOST", host_url)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("Failed to start client command: {}", command))?;

    #[cfg(not(windows))]
    let child = TokioCommand::new("sh")
        .args(["-c", command])
        .current_dir(working_dir)
        .env("SPACETIMEDB_DB_NAME", database_name)
        .env("SPACETIMEDB_HOST", host_url)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("Failed to start client command: {}", command))?;

    Ok(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_and_save_preserves_existing_config() {
        let temp = TempDir::new().unwrap();

        // Create a config with generate and publish but no dev-run
        let initial_config = r#"{
            "generate": [
                { "out-dir": "./foo-client/src/module_bindings", "module-path": "foo", "language": "rust" }
            ],
            "publish": {
                "database": "test-db",
                "server": "maincloud"
            }
        }"#;

        let config_path = temp.path().join("spacetime.json");
        fs::write(&config_path, initial_config).unwrap();

        // Create a package.json to enable detection
        let package_json = r#"{
            "name": "test",
            "scripts": {
                "dev": "vite"
            }
        }"#;
        fs::write(temp.path().join("package.json"), package_json).unwrap();

        // Load the config
        let loaded_config = SpacetimeConfig::load(&config_path).unwrap();
        assert!(loaded_config.dev.is_none());
        assert!(loaded_config.generate.is_some());
        assert!(loaded_config.publish.is_some());

        // Call detect_and_save_client_command which should detect "npm run dev"
        let detected = detect_and_save_client_command(temp.path(), Some(loaded_config));
        assert!(detected.is_some(), "Should detect client command from package.json");

        // Load again and verify all fields are preserved
        let reloaded_config = SpacetimeConfig::load(&config_path).unwrap();
        assert!(
            reloaded_config.dev.as_ref().and_then(|d| d.run.as_ref()).is_some(),
            "dev.run should be set"
        );
        assert!(reloaded_config.generate.is_some(), "generate field should be preserved");
        assert!(reloaded_config.publish.is_some(), "publish field should be preserved");

        // Verify the generate array has the expected content
        let generate = reloaded_config.generate.unwrap();
        assert_eq!(generate.len(), 1);
        assert_eq!(
            generate[0].get("out-dir").unwrap().as_str().unwrap(),
            "./foo-client/src/module_bindings"
        );

        // Verify the publish object has the expected content
        let publish = reloaded_config.publish.unwrap();
        assert_eq!(
            publish.additional_fields.get("database").unwrap().as_str().unwrap(),
            "test-db"
        );
    }
}
