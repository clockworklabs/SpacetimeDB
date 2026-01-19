//! The `spacetime code` subcommand - AI-assisted development TUI for SpacetimeDB.

mod ai;
mod app;
mod events;
mod state;
mod tools;
mod ui;

use crate::common_args;
use crate::common_args::ClearMode;
use crate::config::Config;
use crate::generate::Language;
use crate::subcommands::init;
use crate::util::{detect_module_language, get_login_token_or_log_in, ModuleLanguage};
use anyhow::Context;
use clap::{Arg, ArgMatches, Command};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect};
use state::AppState;
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded_templates.rs"));
}

pub fn cli() -> Command {
    Command::new("code")
        .about("Start AI-assisted development with an integrated TUI for SpacetimeDB")
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
                .help("The path to the module bindings directory relative to the project directory"),
        )
        .arg(
            Arg::new("module-project-path")
                .long("module-project-path")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value("spacetimedb")
                .help("The path to the SpacetimeDB server module project relative to the project directory"),
        )
        .arg(
            Arg::new("client-lang")
                .long("client-lang")
                .value_parser(clap::value_parser!(Language))
                .help("The programming language for the generated client module bindings"),
        )
        .arg(common_args::server().help("The nickname, host name or URL of the server to publish to"))
        .arg(common_args::yes())
        .arg(common_args::clear_database())
        .arg(
            Arg::new("template")
                .short('t')
                .long("template")
                .value_name("TEMPLATE")
                .help("Template ID or GitHub repository for project initialization"),
        )
        .arg(
            Arg::new("no-tty-check")
                .long("no-tty-check")
                .hide(true)
                .action(clap::ArgAction::SetTrue)
                .help("Skip TTY check (for development/testing)"),
        )
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> anyhow::Result<ExitCode> {
    let project_path = args.get_one::<PathBuf>("project-path").unwrap();
    let spacetimedb_project_path = args.get_one::<PathBuf>("module-project-path").unwrap();
    let module_bindings_path = args.get_one::<PathBuf>("module-bindings-path").unwrap();
    let _client_language = args.get_one::<Language>("client-lang").cloned();
    let _clear_database = args
        .get_one::<ClearMode>("clear-database")
        .copied()
        .unwrap_or(ClearMode::OnConflict);
    let force = args.get_flag("force");

    // Check if we're in a TTY - TUI requires interactive terminal
    if !std::io::stdout().is_terminal() {
        anyhow::bail!(
            "spacetime code requires an interactive terminal.\n\
             Use `spacetime dev` instead for non-interactive environments."
        );
    }

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

    // Handle authentication
    let mut auth_token = None;
    if resolved_server == "maincloud" && config.spacetimedb_token().is_none() {
        let should_login = Confirm::new()
            .with_prompt("Would you like to sign in now?")
            .default(true)
            .interact()?;
        if !should_login && server.is_some() {
            // The user explicitly provided --server maincloud but doesn't want to log in
            anyhow::bail!("Login required to publish to maincloud server");
        } else if !should_login {
            // Print warning saying that without logging in we will use local server
            println!(
                "{} {}",
                "Warning:".yellow().bold(),
                "Without logging in, the local server will be used and AI features will be limited.".dimmed()
            );
            resolved_server = "local";
        } else {
            // Login
            let token = get_login_token_or_log_in(&mut config, Some(resolved_server), !force).await?;
            auth_token = Some(token);
        }
    } else if let Some(token) = config.spacetimedb_token() {
        auth_token = Some(token.to_string());
    }

    // Check positional argument first, then deprecated --database flag
    let database_name = if let Some(name) = args
        .get_one::<String>("database")
        .or_else(|| args.get_one::<String>("database-flag"))
    {
        if args.get_one::<String>("database-flag").is_some() {
            println!(
                "{} {}",
                "Warning:".yellow().bold(),
                "--database flag is deprecated. Use positional argument instead: spacetime code <database>".dimmed()
            );
        }
        name.clone()
    } else {
        println!("\n{}", "Found existing SpacetimeDB project.".green());
        println!("Now we need to select a database to publish to.\n");

        if resolved_server == "local" {
            generate_database_name()
        } else {
            // If not logged in before, but login was successful just now, this will have the token
            let token = match &auth_token {
                Some(t) => t.clone(),
                None => get_login_token_or_log_in(&mut config, Some(resolved_server), !force).await?,
            };
            auth_token = Some(token.clone());

            let choice = FuzzySelect::with_theme(&ColorfulTheme::default())
                .with_prompt("Database selection")
                .items(&["Create new database with random name", "Select from existing databases"])
                .default(0)
                .interact()?;

            if choice == 0 {
                generate_database_name()
            } else {
                super::dev::select_database(&config, resolved_server, &token).await?
            }
        }
    };

    if args.get_one::<String>("database").is_none() && args.get_one::<String>("database-flag").is_none() {
        println!("\n{} {}", "Selected database:".green().bold(), database_name.cyan());
        println!(
            "{} {}",
            "Tip:".yellow().bold(),
            format!("Use `spacetime code {}` to skip this question next time", database_name).dimmed()
        );
    }

    // Detect module language for AI rules
    let module_language = detect_module_language(&spacetimedb_dir)?;

    // Ensure AGENTS.md exists for AI context
    ensure_agents_md(&project_dir, module_language)?;

    // Create .spacetime directory
    let spacetime_dir = project_dir.join(".spacetime");
    fs::create_dir_all(&spacetime_dir)?;

    println!();
    println!(
        "{} {}",
        "WARNING:".yellow().bold(),
        "spacetime code is in BETA. Features may be incomplete or change.".yellow()
    );
    println!();
    println!("{}", "Starting spacetime code TUI...".green().bold());
    println!("Database: {}", database_name.cyan());
    println!("Server: {}", resolved_server.cyan());
    println!("{}", "Press Ctrl+C to exit, ? for help".dimmed());
    println!();

    // Create app state
    let state = AppState::new(
        project_dir,
        spacetimedb_dir,
        module_bindings_dir,
        database_name,
        resolved_server.to_string(),
        module_language,
    );

    // Run the TUI app
    app::run(config, state, auth_token).await?;

    Ok(ExitCode::SUCCESS)
}

/// Strip YAML frontmatter from .mdc files (the --- delimited section at the start)
fn strip_mdc_frontmatter(content: &str) -> &str {
    // Look for frontmatter: starts with --- and ends with ---
    if let Some(after_opening) = content.strip_prefix("---") {
        if let Some(end_idx) = after_opening.find("\n---") {
            // Skip past the closing --- and the newline after it
            let remaining = &after_opening[end_idx + 4..]; // 4 for \n---
            // Skip any leading newlines after frontmatter
            return remaining.trim_start_matches('\n');
        }
    }
    content
}

/// Ensure AGENTS.md exists in the project directory.
/// If it doesn't exist, create it with SpacetimeDB AI rules.
fn ensure_agents_md(project_dir: &std::path::Path, module_language: ModuleLanguage) -> anyhow::Result<()> {
    let agents_md_path = project_dir.join("AGENTS.md");

    // Don't overwrite if it already exists
    if agents_md_path.exists() {
        return Ok(());
    }

    let base_rules = embedded::get_ai_rules_base();
    let base_content = strip_mdc_frontmatter(base_rules);
    let mut combined_content = base_content.to_string();

    // Add language-specific rules based on module language
    match module_language {
        ModuleLanguage::Rust => {
            let rust_rules = embedded::get_ai_rules_rust();
            let rust_content = strip_mdc_frontmatter(rust_rules);
            combined_content.push_str("\n\n");
            combined_content.push_str(rust_content);
        }
        ModuleLanguage::Csharp => {
            let csharp_rules = embedded::get_ai_rules_csharp();
            let csharp_content = strip_mdc_frontmatter(csharp_rules);
            combined_content.push_str("\n\n");
            combined_content.push_str(csharp_content);
        }
        ModuleLanguage::Javascript => {
            let ts_rules = embedded::get_ai_rules_typescript();
            let ts_content = strip_mdc_frontmatter(ts_rules);
            combined_content.push_str("\n\n");
            combined_content.push_str(ts_content);
        }
    }

    fs::write(&agents_md_path, &combined_content)?;
    println!(
        "{} {}",
        "Created".green(),
        agents_md_path.display().to_string().cyan()
    );
    Ok(())
}

fn generate_database_name() -> String {
    let mut generator = names::Generator::with_naming(names::Name::Numbered);
    generator.next().unwrap()
}
