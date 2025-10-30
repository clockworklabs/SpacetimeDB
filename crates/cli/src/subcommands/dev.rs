use crate::config::Config;
use crate::generate::Language;
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
use std::borrow::Cow;
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
use tokio::task::JoinHandle;
use tokio::time::sleep;

pub fn cli() -> Command {
    Command::new("dev")
        .about("Start development mode with auto-regenerate client module bindings, auto-rebuild, and auto-publish on file changes.")
        .arg(
            Arg::new("database")
                .long("database")
                .help("The database name/identity to publish to (optional, will prompt if not provided)"),
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
    let force = args.get_flag("force");

    // If you don't specify a server, we default to your default server
    // If you don't have one of those, we default to "maincloud"
    let server = args.get_one::<String>("server").map(|s| s.as_str());

    let default_server_name = config.default_server_name().map(|s| s.to_string());

    let mut resolved_server = server
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

    // Check if we are in a SpacetimeDB project directory
    if !spacetimedb_dir.exists() || !spacetimedb_dir.is_dir() {
        println!("{}", "No SpacetimeDB project found in current directory.".yellow());
        let should_init = Confirm::new()
            .with_prompt("Would you like to initialize a new project?")
            .default(true)
            .interact()?;

        if should_init {
            let init_args = init::cli().get_matches_from(if resolved_server == "local" {
                vec!["init", "--local"]
            } else {
                vec!["init"]
            });
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

    if resolved_server == "maincloud" && config.spacetimedb_token().is_none() {
        let should_login = Confirm::new()
            .with_prompt("Would you like to sign in now?")
            .default(true)
            .interact()?;
        if !should_login && server.is_some() {
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
    let use_local = resolved_server == "local";

    let database_name = if let Some(name) = args.get_one::<String>("database") {
        name.clone()
    } else {
        println!("\n{}", "Found existing SpacetimeDB project.".green());
        println!("Now we need to select a database to publish to.\n");

        if use_local {
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
        }
    };

    if !args.contains_id("database") {
        println!("\n{} {}", "Selected database:".green().bold(), database_name.cyan());
        println!(
            "{} {}",
            "Tip:".yellow().bold(),
            format!("Use `--database {}` to skip this question next time", database_name).dimmed()
        );
    }

    println!("\n{}", "Starting development mode...".green().bold());
    println!("Database: {}", database_name.cyan());
    println!(
        "Watching for changes in: {}",
        spacetimedb_dir.display().to_string().cyan()
    );
    println!("{}", "Press Ctrl+C to stop".dimmed());
    println!();

    generate_build_and_publish(
        &config,
        &project_dir,
        &spacetimedb_dir,
        &module_bindings_dir,
        &database_name,
        client_language,
        resolved_server,
    )
    .await?;

    // Sleep for a second to allow the database to be published on Maincloud
    sleep(Duration::from_secs(1)).await;

    let db_identity = database_identity(&config, &database_name, Some(resolved_server)).await?;
    let _log_handle = start_log_stream(config.clone(), db_identity.to_hex().to_string(), Some(resolved_server)).await?;

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

    let src_dir = spacetimedb_dir.join("src");
    watcher.watch(&src_dir, RecursiveMode::Recursive)?;

    println!("{}", "Watching for file changes...".dimmed());

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
                &database_name,
                client_language,
                resolved_server,
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

    fs::write(env_path, contents)?;
    Ok(())
}

async fn generate_build_and_publish(
    config: &Config,
    project_dir: &Path,
    spacetimedb_dir: &Path,
    module_bindings_dir: &Path,
    database_name: &str,
    client_language: Option<&Language>,
    server: &str,
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

    if client_language == &Language::TypeScript {
        // Update SPACETIMEDB_DBNAME environment variables in `.env.local` for TypeScript client
        println!(
            "{} {}...",
            "Updating .env.local with database name".cyan(),
            database_name
        );
        let env_path = project_dir.join(".env.local");
        let server_host_url = config.get_host_url(Some(server))?;
        upsert_env_db_names_and_hosts(&env_path, &server_host_url, database_name)?;
    }

    println!("{}", "Building...".cyan());
    let (_path_to_program, _host_type) =
        tasks::build(spacetimedb_dir, Some(Path::new("src")), false).context("Failed to build project")?;
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
    generate::exec(config.clone(), &generate_args).await?;

    println!("{}", "Publishing...".cyan());

    let project_path_str = spacetimedb_dir.to_str().unwrap();

    let mut publish_args = vec!["publish", database_name, "--project-path", project_path_str, "--yes"];
    publish_args.extend_from_slice(&["--server", server]);

    let publish_cmd = publish::cli();
    let publish_matches = publish_cmd
        .try_get_matches_from(publish_args)
        .context("Failed to create publish arguments")?;

    publish::exec(config.clone(), &publish_matches).await?;

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
) -> Result<JoinHandle<()>, anyhow::Error> {
    let server = server.map(|s| s.to_string());
    let host_url = config.get_host_url(server.as_deref())?;
    let auth_header = get_auth_header(&mut config, false, server.as_deref(), false).await?;

    let handle = tokio::spawn(async move {
        loop {
            if let Err(e) = stream_logs(&host_url, &database_identity, &auth_header).await {
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
        format_log_record(&mut out, &record)?;
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

fn format_log_record<W: WriteColor>(out: &mut W, record: &LogRecord<'_>) -> Result<(), std::io::Error> {
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
