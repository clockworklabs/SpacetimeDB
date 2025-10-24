use crate::common_args;
use crate::config::Config;
use crate::subcommands::init;
use crate::util::{
    add_auth_header_opt, database_identity, detect_module_language, get_auth_header, get_login_token_or_log_in,
    spacetime_reverse_dns, ResponseExt,
};
use crate::{publish, tasks};
use anyhow::Context;
use clap::{Arg, ArgAction, ArgMatches, Command};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect};
use futures::stream::{self, StreamExt};
use futures::{AsyncBufReadExt, TryStreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use std::borrow::Cow;
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

pub fn cli() -> Command {
    Command::new("dev")
        .about("Start development mode with auto-rebuild and publish")
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
            Arg::new("local")
                .long("local")
                .action(ArgAction::SetTrue)
                .help("Use local deployment instead of Maincloud"),
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
    let use_local = args.get_flag("local");
    let force = args.get_flag("force");

    let server = if let Some(s) = args.get_one::<String>("server") {
        Some(s.as_str())
    } else if use_local {
        None
    } else {
        Some("maincloud")
    };

    let mut spacetimedb_dir = project_path.join("spacetimedb");
    if !spacetimedb_dir.exists() || !spacetimedb_dir.is_dir() {
        println!("{}", "No SpacetimeDB project found in current directory.".yellow());
        let should_init = Confirm::new()
            .with_prompt("Would you like to initialize a new project?")
            .default(true)
            .interact()?;

        if should_init {
            let init_args = init::cli().get_matches_from(if use_local {
                vec!["init", "--local"]
            } else {
                vec!["init"]
            });
            let created_project_path = init::exec(config.clone(), &init_args).await?;

            let canonical_created_path = created_project_path
                .canonicalize()
                .context("Failed to canonicalize created project path")?;
            spacetimedb_dir = canonical_created_path.join("spacetimedb");

            if !spacetimedb_dir.exists() {
                anyhow::bail!("Project initialization did not create spacetimedb directory");
            }
        } else {
            anyhow::bail!("Not in a SpacetimeDB project directory");
        }
    }

    let use_local = if use_local {
        true
    } else if config.spacetimedb_token().is_some() {
        false
    } else {
        let should_login = Confirm::new()
            .with_prompt("Would you like to sign in to use Maincloud? (Select 'no' to use localhost)")
            .default(true)
            .interact()?;
        !should_login
    };

    let database_name = if let Some(name) = args.get_one::<String>("database") {
        name.clone()
    } else {
        println!("\n{}", "Found existing SpacetimeDB project.".green());
        println!("Now we need to select a database to publish to.\n");

        if use_local {
            generate_database_name()
        } else {
            let token = get_login_token_or_log_in(&mut config, server, !force).await?;

            let choice = FuzzySelect::with_theme(&ColorfulTheme::default())
                .with_prompt("Database selection")
                .items(&["Create new database with random name", "Select from existing databases"])
                .default(0)
                .interact()?;

            if choice == 0 {
                generate_database_name()
            } else {
                select_database(&config, server, &token).await?
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

    build_and_publish(&config, &spacetimedb_dir, &database_name, server, use_local).await?;

    let db_identity = database_identity(&config, &database_name, server).await?;
    let _log_handle = start_log_stream(config.clone(), db_identity.to_hex().to_string(), server).await?;

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
            match build_and_publish(&config, &spacetimedb_dir, &database_name, server, use_local).await {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("{} {}", "Error:".red().bold(), e);
                    println!("{}", "Waiting for next change...".dimmed());
                }
            }
        }
    }
}

async fn build_and_publish(
    config: &Config,
    project_path: &Path,
    database_name: &str,
    server: Option<&str>,
    _use_local: bool,
) -> Result<(), anyhow::Error> {
    detect_module_language(project_path)?;

    println!("{}", "Building...".cyan());
    let (_path_to_program, _host_type) =
        tasks::build(project_path, Some(Path::new("src")), false).context("Failed to build project")?;
    println!("{}", "Build complete!".green());

    println!("{}", "Publishing...".cyan());

    let project_path_str = project_path.to_str().unwrap();
    let server_arg = server.map(|s| s.to_string());

    let mut publish_args = vec!["publish", database_name, "--project-path", project_path_str, "--yes"];
    if let Some(ref s) = server_arg {
        publish_args.extend_from_slice(&["--server", s]);
    }

    let publish_cmd = publish::cli();
    let publish_matches = publish_cmd
        .try_get_matches_from(publish_args)
        .context("Failed to create publish arguments")?;

    publish::exec(config.clone(), &publish_matches).await?;

    println!("{}", "Published successfully!".green().bold());
    println!("{}", "---".dimmed());

    Ok(())
}

async fn select_database(config: &Config, server: Option<&str>, token: &str) -> Result<String, anyhow::Error> {
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
            config.get_host_url(server)?,
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
                    let names_response = spacetime_reverse_dns(&config, &identity_str, server).await?;
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
