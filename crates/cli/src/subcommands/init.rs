use crate::Config;
use crate::{detect::find_executable, util::UNSTABLE_WARNING};
use anyhow::anyhow;
use anyhow::Context;
use clap::{Arg, ArgMatches};
use colored::Colorize;
use convert_case::{Case, Casing};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::{fmt, fs};
use toml_edit::{value, DocumentMut, Item};
use xmltree::{Element, XMLNode};

use crate::subcommands::login::{spacetimedb_login_force, DEFAULT_AUTH_HOST};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded_templates.rs"));
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemplateDefinition {
    pub id: String,
    pub description: String,
    pub server_source: String,
    pub client_source: String,
    #[serde(default)]
    pub server_lang: Option<String>,
    #[serde(default)]
    pub client_lang: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HighlightDefinition {
    pub name: String,
    pub template_id: String,
}

#[derive(Debug, Deserialize)]
struct TemplatesList {
    highlights: Vec<HighlightDefinition>,
    templates: Vec<TemplateDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateType {
    Builtin,
    GitHub,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerLanguage {
    Rust,
    Csharp,
    TypeScript,
}

impl ServerLanguage {
    fn as_str(&self) -> &'static str {
        match self {
            ServerLanguage::Rust => "rust",
            ServerLanguage::Csharp => "csharp",
            ServerLanguage::TypeScript => "typescript",
        }
    }

    fn from_str(s: &str) -> anyhow::Result<Option<Self>> {
        match s.to_lowercase().as_str() {
            "rust" => Ok(Some(ServerLanguage::Rust)),
            "csharp" | "c#" => Ok(Some(ServerLanguage::Csharp)),
            "typescript" => Ok(Some(ServerLanguage::TypeScript)),
            _ => Err(anyhow!("Unknown server language: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientLanguage {
    Rust,
    Csharp,
    TypeScript,
}

impl ClientLanguage {
    fn as_str(&self) -> &'static str {
        match self {
            ClientLanguage::Rust => "rust",
            ClientLanguage::Csharp => "csharp",
            ClientLanguage::TypeScript => "typescript",
        }
    }

    fn from_str(s: &str) -> anyhow::Result<Option<Self>> {
        match s.to_lowercase().as_str() {
            "rust" => Ok(Some(ClientLanguage::Rust)),
            "csharp" | "c#" => Ok(Some(ClientLanguage::Csharp)),
            "typescript" => Ok(Some(ClientLanguage::TypeScript)),
            _ => Err(anyhow!("Unknown client language: {}", s)),
        }
    }
}

pub struct TemplateConfig {
    pub project_name: String,
    pub project_path: PathBuf,
    pub template_type: TemplateType,
    pub server_lang: Option<ServerLanguage>,
    pub client_lang: Option<ClientLanguage>,
    pub github_repo: Option<String>,
    pub template_def: Option<TemplateDefinition>,
    pub use_local: bool,
}

pub fn cli() -> clap::Command {
    clap::Command::new("init")
        .about(format!("Initializes a new spacetime project. {UNSTABLE_WARNING}"))
        .arg(
            Arg::new("project-path")
                .long("project-path")
                .value_name("PATH")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Directory where the project will be created (defaults to ./<PROJECT_NAME>)"),
        )
        .arg(Arg::new("project-name").value_name("PROJECT_NAME").help("Project name"))
        .arg(
            Arg::new("server-only")
                .long("server-only")
                .help("Initialize server only from the template (no client)")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("lang").long("lang").value_name("LANG").help(
                "Server language: rust, csharp, typescript (it can only be used when --template is not specified)",
            ),
        )
        .arg(
            Arg::new("template")
                .short('t')
                .long("template")
                .value_name("TEMPLATE")
                .help("Template ID or GitHub repository (owner/repo or URL)"),
        )
        .arg(
            Arg::new("local")
                .long("local")
                .action(clap::ArgAction::SetTrue)
                .help("Use local deployment instead of Maincloud"),
        )
        .arg(
            Arg::new("non-interactive")
                .long("non-interactive")
                .action(clap::ArgAction::SetTrue)
                .help("Run in non-interactive mode"),
        )
}

pub async fn fetch_templates_list() -> anyhow::Result<(Vec<HighlightDefinition>, Vec<TemplateDefinition>)> {
    let content = embedded::get_templates_json();
    let templates_list: TemplatesList = serde_json::from_str(content).context("Failed to parse templates list JSON")?;

    Ok((templates_list.highlights, templates_list.templates))
}

pub async fn check_and_prompt_login(config: &mut Config) -> anyhow::Result<bool> {
    if config.spacetimedb_token().is_some() {
        println!("{}", "You are logged in to SpacetimeDB.".green());
        return Ok(true);
    }

    println!("{}", "You are not logged in to SpacetimeDB.".yellow());

    let theme = ColorfulTheme::default();
    let should_login = Confirm::with_theme(&theme)
        .with_prompt("Would you like to log in? (required for Maincloud deployment)")
        .default(true)
        .interact()?;

    if should_login {
        let host = Url::parse(DEFAULT_AUTH_HOST)?;
        spacetimedb_login_force(config, &host, false).await?;
        println!("{}", "Successfully logged in!".green());
        Ok(true)
    } else {
        println!("{}", "Continuing with local deployment.".yellow());
        Ok(false)
    }
}

fn slugify(name: &str) -> String {
    name.to_case(Case::Kebab)
}

async fn get_project_name(args: &ArgMatches, is_interactive: bool) -> anyhow::Result<String> {
    if let Some(name) = args.get_one::<String>("project-name") {
        if is_interactive {
            println!("{} {}", "Project name:".bold(), name);
        }
        return Ok(name.clone());
    }

    if !is_interactive {
        anyhow::bail!("PROJECT_NAME is required in non-interactive mode");
    }

    let theme = ColorfulTheme::default();
    let name = Input::with_theme(&theme)
        .with_prompt("Project name")
        .default("my-spacetime-app".to_string())
        .validate_with(|input: &String| -> Result<(), String> {
            if input.trim().is_empty() {
                return Err("Project name cannot be empty".to_string());
            }
            Ok(())
        })
        .interact_text()?
        .trim()
        .to_string();

    Ok(name)
}

async fn get_project_path(
    args: &ArgMatches,
    project_name: &str,
    is_interactive: bool,
    is_server_only: bool,
) -> anyhow::Result<PathBuf> {
    if let Some(path) = args.get_one::<PathBuf>("project-path") {
        if is_interactive {
            println!("{} {}", "Project path:".bold(), path.display());
        }
        return Ok(path.clone());
    }

    if !is_interactive {
        return Ok(PathBuf::from(slugify(project_name)));
    }

    let theme = ColorfulTheme::default();
    let path_str = Input::with_theme(&theme)
        .with_prompt("Project path")
        .default(format!("./{}", slugify(project_name)))
        .validate_with(|input: &String| -> Result<(), String> {
            if input.trim().is_empty() {
                return Err("Project path cannot be empty".to_string());
            }

            let path = Path::new(input);
            if path.exists() {
                if !path.is_dir() {
                    return Err(format!("A file exists at '{}'. Please choose a different path.", input));
                }
                match std::fs::read_dir(path) {
                    Ok(entries) => {
                        // If server-only, allow non-empty directories (client files won't be created)
                        // but only if the `spacetimedb` subdirectory does not already exist
                        let entries_vec = entries.collect::<Vec<_>>();
                        if is_server_only
                            && !entries_vec.iter().any(|e| match e {
                                Ok(dir_entry) => dir_entry.file_name() == "spacetimedb",
                                Err(_) => false,
                            })
                        {
                            return Ok(());
                        }
                        if entries_vec.iter().filter(|e| e.is_ok()).count() > 0 {
                            return Err(format!(
                                "Directory '{}' already exists and is not empty. Please choose a different path.",
                                input
                            ));
                        }
                    }
                    Err(_) => {
                        return Err(format!(
                            "Cannot access directory '{}'. Please choose a different path.",
                            input
                        ));
                    }
                }
            }
            Ok(())
        })
        .interact_text()?
        .trim()
        .to_string();

    Ok(PathBuf::from(path_str))
}

fn create_template_config_from_template_str(
    project_name: String,
    project_path: PathBuf,
    template_str: &str,
    templates: &[TemplateDefinition],
) -> anyhow::Result<TemplateConfig> {
    if let Some(template) = templates.iter().find(|t| t.id == template_str) {
        // Builtin template
        Ok(TemplateConfig {
            project_name,
            project_path,
            template_type: TemplateType::Builtin,
            server_lang: parse_server_lang(&template.server_lang)?,
            client_lang: parse_client_lang(&template.client_lang)?,
            github_repo: None,
            template_def: Some(template.clone()),
            use_local: true,
        })
    } else {
        // GitHub template
        Ok(TemplateConfig {
            project_name,
            project_path,
            template_type: TemplateType::GitHub,
            server_lang: None,
            client_lang: None,
            github_repo: Some(template_str.to_string()),
            template_def: None,
            use_local: true,
        })
    }
}

#[cfg(windows)]
fn run_pm(pm: PackageManager, args: &[&str], cwd: &Path) -> std::io::Result<std::process::ExitStatus> {
    // Use cmd to resolve .cmd/.bat/.exe shims properly on Windows
    std::process::Command::new("cmd")
        .arg("/C")
        .arg(pm.to_string())
        .args(args)
        .current_dir(cwd)
        .status()
}

#[cfg(not(windows))]
fn run_pm(pm: PackageManager, args: &[&str], cwd: &Path) -> std::io::Result<std::process::ExitStatus> {
    std::process::Command::new(pm.to_string())
        .args(args)
        .current_dir(cwd)
        .status()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl fmt::Display for PackageManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PackageManager::Npm => "npm",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Bun => "bun",
        };
        write!(f, "{s}")
    }
}

pub fn prompt_for_typescript_package_manager() -> anyhow::Result<Option<PackageManager>> {
    println!(
        "\n{}",
        "TypeScript server requires dependencies to be installed before publishing.".yellow()
    );

    // Prompt for package manager
    let theme = ColorfulTheme::default();
    let choices = vec!["npm", "pnpm", "yarn", "bun", "none"];
    let selection = Select::with_theme(&theme)
        .with_prompt("Which package manager would you like to use?")
        .items(&choices)
        .default(0)
        .interact()?;

    Ok(match selection {
        0 => Some(PackageManager::Npm),
        1 => Some(PackageManager::Pnpm),
        2 => Some(PackageManager::Yarn),
        3 => Some(PackageManager::Bun),
        _ => None,
    })
}

pub fn install_typescript_dependencies(
    package_dir: &Path,
    package_manager: Option<PackageManager>,
) -> anyhow::Result<()> {
    if let Some(pm) = package_manager {
        println!("Installing dependencies with {}...", pm);

        // Command arguments
        let mut args_map: HashMap<&str, Vec<&str>> = HashMap::new();
        args_map.insert("npm", vec!["install", "--no-fund", "--no-audit", "--loglevel=error"]);
        args_map.insert("yarn", vec!["install", "--no-fund"]);
        args_map.insert(
            "pnpm",
            vec!["install", "--ignore-workspace", "--config.ignore-scripts=false"],
        );
        args_map.insert("bun", vec!["install"]);

        let args: &[&str] = args_map
            .get(pm.to_string().as_str())
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        // Run and stream output cross-platform
        let status = run_pm(pm, args, package_dir);

        match status {
            Ok(s) if s.success() => {
                println!("{}", "Dependencies installed successfully!".green());
            }
            Ok(s) => {
                eprintln!(
                    "{}",
                    format!("Installation failed (exit code {}).", s.code().unwrap_or(-1)).red()
                );
                println!(
                    "{}",
                    format!("Please run '{} install' manually in {}.", pm, package_dir.display()).yellow()
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!(
                    "{}",
                    format!("Failed to find '{}'. Is it installed and on PATH?", pm).red()
                );
                println!(
                    "{}",
                    format!("Please run '{} install' manually in {}.", pm, package_dir.display()).yellow()
                );
            }
            Err(e) => {
                eprintln!("{}", format!("Failed to execute {}: {}", pm, e).red());
                println!(
                    "{}",
                    format!("Please run '{} install' manually in {}.", pm, package_dir.display()).yellow()
                );
            }
        }
    } else {
        println!(
            "{}",
            format!(
                "You have chosen not to use a package manager. Please install dependencies manually in {}.",
                package_dir.display()
            )
            .yellow()
        );
    }

    Ok(())
}

pub async fn exec_init(config: &mut Config, args: &ArgMatches, is_interactive: bool) -> anyhow::Result<PathBuf> {
    let use_local = if args.get_flag("local") {
        true
    } else if is_interactive {
        !check_and_prompt_login(config).await?
    } else {
        // In non-interactive mode, default to local deployment if not logged in
        config.spacetimedb_token().is_none()
    };

    let is_server_only = args.get_flag("server-only");

    let project_name = get_project_name(args, is_interactive).await?;
    let project_path = get_project_path(args, &project_name, is_interactive, is_server_only).await?;

    let mut template_config = if is_interactive {
        get_template_config_interactive(args, project_name, project_path.clone()).await?
    } else {
        get_template_config_non_interactive(args, project_name, project_path.clone()).await?
    };

    template_config.use_local = use_local;

    ensure_empty_directory(
        &template_config.project_name,
        &template_config.project_path,
        is_server_only,
    )?;
    init_from_template(&template_config, &template_config.project_path, is_server_only).await?;

    if template_config.server_lang == Some(ServerLanguage::TypeScript)
        && template_config.client_lang == Some(ClientLanguage::TypeScript)
    {
        // If server & client are TypeScript, handle dependency installation
        // NOTE: All server templates must have their server code in `spacetimedb/` directory
        // This is not a requirement in general, but is a requirement for all templates
        // i.e. `spacetime dev` is valid on non-templates.
        let pm = if is_interactive {
            prompt_for_typescript_package_manager()?
        } else {
            None
        };
        let client_dir = template_config.project_path;
        let server_dir = client_dir.join("spacetimedb");
        install_typescript_dependencies(&server_dir, pm)?;
        install_typescript_dependencies(&client_dir, pm)?;
    } else if template_config.client_lang == Some(ClientLanguage::TypeScript) {
        let pm = if is_interactive {
            prompt_for_typescript_package_manager()?
        } else {
            None
        };
        let client_dir = template_config.project_path;
        install_typescript_dependencies(&client_dir, pm)?;
    } else if template_config.server_lang == Some(ServerLanguage::TypeScript) {
        let pm = if is_interactive {
            prompt_for_typescript_package_manager()?
        } else {
            None
        };
        // NOTE: All server templates must have their server code in `spacetimedb/` directory
        // This is not a requirement in general, but is a requirement for all templates
        // i.e. `spacetime dev` is valid on non-templates.
        let server_dir = template_config.project_path.join("spacetimedb");
        install_typescript_dependencies(&server_dir, pm)?;
    }

    Ok(project_path)
}

async fn get_template_config_non_interactive(
    args: &ArgMatches,
    project_name: String,
    project_path: PathBuf,
) -> anyhow::Result<TemplateConfig> {
    // Check if template is provided
    if let Some(template_str) = args.get_one::<String>("template") {
        // Check if it's a builtin template
        let (_, templates) = fetch_templates_list().await?;
        return create_template_config_from_template_str(project_name, project_path, template_str, &templates);
    }

    // No template - require at least one language option
    let server_lang_str = args.get_one::<String>("lang").cloned();

    if server_lang_str.is_none() {
        anyhow::bail!("Either --template or --lang must be provided in non-interactive mode");
    }

    Ok(TemplateConfig {
        project_name,
        project_path,
        template_type: TemplateType::Empty,
        server_lang: parse_server_lang(&server_lang_str)?,
        client_lang: None,
        github_repo: None,
        template_def: None,
        use_local: true,
    })
}

pub fn ensure_empty_directory(_project_name: &str, project_path: &Path, is_server_only: bool) -> anyhow::Result<()> {
    if project_path.exists() {
        if !project_path.is_dir() {
            anyhow::bail!(
                "Path {} exists but is not a directory. A new SpacetimeDB project must be initialized in an empty directory.",
                project_path.display()
            );
        }

        if std::fs::read_dir(project_path).unwrap().count() > 0 {
            if is_server_only {
                let server_dir = project_path.join("spacetimedb");
                if server_dir.exists() && std::fs::read_dir(server_dir).unwrap().count() > 0 {
                    anyhow::bail!(
                        "A SpacetimeDB module already exists in the target directory: {}",
                        project_path.display()
                    );
                }
            } else {
                anyhow::bail!(
                    "Cannot create new SpacetimeDB project in non-empty directory: {}",
                    project_path.display()
                );
            }
        }
    } else {
        fs::create_dir_all(project_path).context("Failed to create directory")?;
    }
    Ok(())
}

async fn get_template_config_interactive(
    args: &ArgMatches,
    project_name: String,
    project_path: PathBuf,
) -> anyhow::Result<TemplateConfig> {
    let theme = ColorfulTheme::default();

    // Check if template is provided
    if let Some(template_str) = args.get_one::<String>("template") {
        println!("{} {}", "Template:".bold(), template_str);

        let (_, templates) = fetch_templates_list().await?;
        return create_template_config_from_template_str(project_name, project_path, template_str, &templates);
    }

    let server_lang_arg = args.get_one::<String>("lang");
    if server_lang_arg.is_some() {
        let server_lang = parse_server_lang(&server_lang_arg.cloned())?;
        if let Some(lang_str) = server_lang_arg {
            println!("{} {}", "Server language:".bold(), lang_str);
        }

        return Ok(TemplateConfig {
            project_name,
            project_path,
            template_type: TemplateType::Empty,
            server_lang,
            client_lang: None,
            github_repo: None,
            template_def: None,
            use_local: true,
        });
    }

    // Fully interactive mode - prompt for template/language selection
    let (highlights, templates) = fetch_templates_list().await?;

    let mut client_choices: Vec<String> = highlights
        .iter()
        .map(|h| {
            let template = templates.iter().find(|t| t.id == h.template_id);
            match template {
                Some(t) => format!("{} - {}", h.name, t.description),
                None => h.name.clone(),
            }
        })
        .collect();
    client_choices.push("Use Template - Choose from a list of built-in template projects or clone an existing SpacetimeDB project from GitHub".to_string());
    client_choices.push("None".to_string());

    let client_selection = Select::with_theme(&theme)
        .with_prompt("Select a client type for your project (you can add other clients later)")
        .items(&client_choices)
        .default(0)
        .interact()?;

    let other_index = highlights.len();
    let none_index = highlights.len() + 1;

    if client_selection < highlights.len() {
        let highlight = &highlights[client_selection];
        let template = templates
            .iter()
            .find(|t| t.id == highlight.template_id)
            .ok_or_else(|| anyhow::anyhow!("Template {} not found", highlight.template_id))?;

        Ok(TemplateConfig {
            project_name,
            project_path,
            template_type: TemplateType::Builtin,
            server_lang: parse_server_lang(&template.server_lang)?,
            client_lang: parse_client_lang(&template.client_lang)?,
            github_repo: None,
            template_def: Some(template.clone()),
            use_local: true,
        })
    } else if client_selection == other_index {
        println!("\n{}", "Available built-in templates:".bold());
        for template in &templates {
            println!("  {} - {}", template.id, template.description);
        }
        println!();

        loop {
            let template_id = Input::<String>::with_theme(&theme)
                .with_prompt("Template ID or GitHub repository (owner/repo) or git URL")
                .interact_text()?
                .trim()
                .to_string();
            let template_config = create_template_config_from_template_str(
                project_name.clone(),
                project_path.clone(),
                &template_id,
                &templates,
            );
            // If template_id looks like a builtin template ID (e.g. kebab-case, all lowercase, no slashes, alphanumeric and dashes only)
            // then ensure that it is a valid builtin template ID, if not reprompt
            let is_builtin_like = |s: &str| {
                !s.is_empty()
                    && !s.contains('/')
                    && s.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            };
            if !is_builtin_like(&template_id) {
                break template_config;
            }
            if templates.iter().any(|t| t.id == template_id) {
                break template_config;
            }
            eprintln!(
                "{}",
                "Unrecognized format. Enter a built-in ID (e.g. \"rust-chat\"), a GitHub repo (\"owner/repo\"), or a git URL."
                    .bold()
            );
        }
    } else if client_selection == none_index {
        // Ask for server language only
        let server_lang_choices = vec!["Rust", "C#", "TypeScript"];
        let server_selection = Select::with_theme(&theme)
            .with_prompt("Select server language")
            .items(&server_lang_choices)
            .default(0)
            .interact()?;

        let server_lang = match server_selection {
            0 => Some(ServerLanguage::Rust),
            1 => Some(ServerLanguage::Csharp),
            2 => Some(ServerLanguage::TypeScript),
            _ => unreachable!("Invalid server language selection"),
        };

        Ok(TemplateConfig {
            project_name,
            project_path,
            template_type: TemplateType::Empty,
            server_lang,
            client_lang: None,
            github_repo: None,
            template_def: None,
            use_local: true,
        })
    } else {
        unreachable!("Invalid selection index")
    }
}

fn clone_github_template(repo_input: &str, target: &Path, is_server_only: bool) -> anyhow::Result<()> {
    let is_git_url = |s: &str| {
        s.starts_with("git@") || s.starts_with("ssh://") || s.starts_with("http://") || s.starts_with("https://")
    };

    let repo_url = if is_git_url(repo_input) {
        repo_input.to_string()
    } else if repo_input.contains('/') {
        format!("https://github.com/{}", repo_input)
    } else {
        anyhow::bail!("Invalid repository format. Use 'owner/repo' or full git clone URL");
    };

    println!("  Cloning from {}...", repo_url);

    let temp_dir = tempfile::tempdir()?;

    let mut builder = git2::build::RepoBuilder::new();

    let mut fetch_options = git2::FetchOptions::new();
    let mut callbacks = git2::RemoteCallbacks::new();

    callbacks.credentials(|_url, username_from_url, allowed_types| {
        if allowed_types.contains(git2::CredentialType::SSH_KEY) {
            if let Some(username) = username_from_url {
                return git2::Cred::ssh_key_from_agent(username);
            }
        }
        if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            return git2::Cred::userpass_plaintext("", "");
        }
        if allowed_types.contains(git2::CredentialType::DEFAULT) {
            return git2::Cred::default();
        }
        Err(git2::Error::from_str("no auth method available"))
    });

    fetch_options.remote_callbacks(callbacks);
    builder.fetch_options(fetch_options);

    builder
        .clone(&repo_url, temp_dir.path())
        .context("Failed to clone repository")?;

    if is_server_only {
        let server_subdir = temp_dir.path().join("spacetimedb");
        let server_subdir_target = target.join("spacetimedb");
        copy_dir_all(&server_subdir, &server_subdir_target)?;
    } else {
        copy_dir_all(temp_dir.path(), target)?;
    }

    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if entry.file_name() == ".git" {
            continue;
        }

        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn get_spacetimedb_typescript_version() -> &'static str {
    embedded::get_typescript_bindings_version()
}

fn update_package_json(dir: &Path, package_name: &str) -> anyhow::Result<()> {
    let package_path = dir.join("package.json");
    if !package_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&package_path)?;
    let mut package: serde_json::Value = serde_json::from_str(&content)?;

    package["name"] = json!(package_name);

    // Update spacetimedb version if it exists in dependencies
    if let Some(deps) = package.get_mut("dependencies") {
        if deps.get("spacetimedb").is_some() {
            deps["spacetimedb"] = json!(format!("^{}", get_spacetimedb_typescript_version()));
        }
    }

    let updated_content = serde_json::to_string_pretty(&package)?;
    fs::write(package_path, updated_content)?;

    Ok(())
}

fn to_patch_wildcard(ver: &str) -> String {
    let mut parts: Vec<&str> = ver.split('.').collect();
    if parts.len() >= 3 {
        parts[2] = "*";
    }
    parts.join(".")
}

fn update_cargo_toml_name(dir: &Path, package_name: &str) -> anyhow::Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let patch_wildcard = to_patch_wildcard(version);
    let cargo_path = dir.join("Cargo.toml");
    if !cargo_path.exists() {
        return Ok(());
    }

    let original = fs::read_to_string(&cargo_path)?;
    let mut doc: DocumentMut = original.parse()?;

    let safe_name = package_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect::<String>();

    if let Some(package_item) = doc.get_mut("package") {
        if let Some(package_table) = package_item.as_table_mut() {
            package_table["name"] = value(safe_name);
            if let Some(edition_item) = package_table.get_mut("edition") {
                if edition_uses_workspace(edition_item) {
                    *edition_item = value(embedded::get_workspace_edition());
                }
            }
        }
    }

    if let Some(deps_item) = doc.get_mut("dependencies") {
        if let Some(deps_table) = deps_item.as_table_mut() {
            let keys: Vec<String> = deps_table.iter().map(|(k, _)| k.to_string()).collect();
            for key in keys {
                if let Some(dep_item) = deps_table.get_mut(&key) {
                    if dependency_uses_workspace(dep_item) {
                        if has_path(dep_item) {
                            if key == "spacetimedb" {
                                if let Some(version) = embedded::get_workspace_dependency_version(&key) {
                                    set_dependency_version(dep_item, version, true);
                                }
                            } else if key == "spacetimedb-sdk" {
                                set_dependency_version(dep_item, patch_wildcard.as_str(), true);
                            }
                            continue;
                        }

                        if uses_workspace(dep_item) {
                            if let Some(version) = embedded::get_workspace_dependency_version(&key) {
                                set_dependency_version(dep_item, version, key == "spacetimedb");
                            }
                        }
                    }
                }
            }
        }
    }

    let updated = doc.to_string();
    if updated != original {
        fs::write(cargo_path, updated)?;
    }
    Ok(())
}

pub fn update_csproj_server_to_nuget(dir: &Path) -> anyhow::Result<()> {
    if let Some(csproj_path) = find_first_csproj(dir)? {
        let original =
            fs::read_to_string(&csproj_path).with_context(|| format!("reading {}", csproj_path.display()))?;
        let mut root: Element =
            Element::parse(original.as_bytes()).with_context(|| format!("parsing xml {}", csproj_path.display()))?;

        upsert_packageref(
            &mut root,
            "SpacetimeDB.Runtime",
            &get_spacetimedb_csharp_runtime_version(),
        );
        remove_all_project_references(&mut root);

        write_if_changed(csproj_path, original, root)?;
    }
    Ok(())
}

pub fn update_csproj_client_to_nuget(dir: &Path) -> anyhow::Result<()> {
    if let Some(csproj_path) = find_first_csproj(dir)? {
        let original =
            fs::read_to_string(&csproj_path).with_context(|| format!("reading {}", csproj_path.display()))?;
        let mut root: Element =
            Element::parse(original.as_bytes()).with_context(|| format!("parsing xml {}", csproj_path.display()))?;

        upsert_packageref(
            &mut root,
            "SpacetimeDB.ClientSDK",
            &get_spacetimedb_csharp_clientsdk_version(),
        );
        remove_all_project_references(&mut root);

        write_if_changed(csproj_path, original, root)?;
    }
    Ok(())
}

// Helpers

fn write_if_changed(path: PathBuf, original: String, root: Element) -> anyhow::Result<()> {
    let mut out = Vec::new();
    root.write(&mut out)?;
    let compact = String::from_utf8(out)?;
    let updated = pretty_format_xml(&compact)?;
    if updated != original {
        fs::write(&path, updated).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

fn find_first_csproj(dir: &Path) -> anyhow::Result<Option<PathBuf>> {
    if !dir.is_dir() {
        return Ok(None);
    }
    for entry in fs::read_dir(dir)? {
        let p = entry?.path();
        if p.extension().map(|e| e == "csproj").unwrap_or(false) {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

/// Remove every <ProjectReference/> under any <ItemGroup>
fn remove_all_project_references(project: &mut Element) {
    for node in project.children.iter_mut() {
        if let XMLNode::Element(item_group) = node {
            if item_group.name == "ItemGroup" {
                item_group
                    .children
                    .retain(|n| !matches!(n, XMLNode::Element(el) if el.name == "ProjectReference"));
            }
        }
    }
    // Optional: prune empty ItemGroups
    project.children.retain(|n| {
        if let XMLNode::Element(el) = n {
            if el.name == "ItemGroup" {
                return el.children.iter().any(|c| matches!(c, XMLNode::Element(_)));
            }
        }
        true
    });
}

/// Insert or update <PackageReference Include="..." Version="..."/>
fn upsert_packageref(project: &mut Element, include: &str, version: &str) {
    // Try to find an existing PackageReference
    for node in project.children.iter_mut() {
        if let XMLNode::Element(item_group) = node {
            if item_group.name == "ItemGroup" {
                if let Some(XMLNode::Element(existing)) = item_group.children.iter_mut().find(|n| {
                    matches!(n,
                        XMLNode::Element(e)
                        if e.name == "PackageReference"
                           && e.attributes.get("Include").map(|v| v == include).unwrap_or(false)
                    )
                }) {
                    existing.attributes.insert("Version".to_string(), version.to_string());
                    return;
                }
            }
        }
    }
    // Otherwise create one in (or create) an ItemGroup
    let item_group = get_or_create_direct_child(project, "ItemGroup");
    let mut pr = Element::new("PackageReference");
    pr.attributes.insert("Include".into(), include.to_string());
    pr.attributes.insert("Version".into(), version.to_string());
    item_group.children.push(XMLNode::Element(pr));
}

fn get_or_create_direct_child<'a>(parent: &'a mut Element, name: &str) -> &'a mut Element {
    // First, scan IMMUTABLY to find the index of an existing child.
    if let Some(idx) = parent.children.iter().enumerate().find_map(|(i, n)| match n {
        XMLNode::Element(e) if e.name == name => Some(i),
        _ => None,
    }) {
        // Now borrow MUTABLY by index.
        if let XMLNode::Element(el) = &mut parent.children[idx] {
            return el;
        }
        unreachable!("Matched non-element while checking by name");
    }

    // Not found: create, then borrow by index.
    parent.children.push(XMLNode::Element(Element::new(name)));
    let idx = parent.children.len() - 1;
    match &mut parent.children[idx] {
        XMLNode::Element(el) => el,
        _ => unreachable!("just pushed an Element"),
    }
}

/// Pretty-print XML with indentation.
/// Keeps UTF-8 declaration if present.
fn pretty_format_xml(xml: &str) -> anyhow::Result<String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;
    use quick_xml::Writer;
    use std::io::Cursor;

    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,
            e => writer.write_event(e)?,
        }
        buf.clear();
    }

    let result = writer.into_inner().into_inner();
    Ok(String::from_utf8(result)?)
}

/// Just do 1.* for now
fn get_spacetimedb_csharp_runtime_version() -> String {
    "1.*".to_string()
}

fn get_spacetimedb_csharp_clientsdk_version() -> String {
    "1.*".to_string()
}

/// Writes a `.env.local` file that includes all common
/// frontend environment variable variants for SpacetimeDB.
fn write_typescript_client_env_file(client_dir: &Path, module_name: &str, use_local: bool) -> anyhow::Result<()> {
    let env_path = client_dir.join(".env.local");

    let db_name = module_name;
    let host = if use_local {
        "ws://localhost:3000"
    } else {
        "wss://maincloud.spacetimedb.com"
    };

    // Framework-agnostic variants
    let env_content = format!(
        "\
# Generic / backend
SPACETIMEDB_DB_NAME={db_name}
SPACETIMEDB_HOST={host}

# Vite
VITE_SPACETIMEDB_DB_NAME={db_name}
VITE_SPACETIMEDB_HOST={host}

# Next.js
NEXT_PUBLIC_SPACETIMEDB_DB_NAME={db_name}
NEXT_PUBLIC_SPACETIMEDB_HOST={host}

# Create React App
REACT_APP_SPACETIMEDB_DB_NAME={db_name}
REACT_APP_SPACETIMEDB_HOST={host}

# Expo
EXPO_PUBLIC_SPACETIMEDB_DB_NAME={db_name}
EXPO_PUBLIC_SPACETIMEDB_HOST={host}

# SvelteKit
PUBLIC_SPACETIMEDB_DB_NAME={db_name}
PUBLIC_SPACETIMEDB_HOST={host}
"
    );

    fs::write(&env_path, env_content)?;

    println!("✅ Wrote environment configuration to {}", env_path.display());
    Ok(())
}

pub async fn init_from_template(
    config: &TemplateConfig,
    project_path: &Path,
    is_server_only: bool,
) -> anyhow::Result<()> {
    println!("{}", "Initializing project from template...".cyan());

    match config.template_type {
        TemplateType::Builtin => init_builtin(config, project_path, is_server_only)?,
        TemplateType::GitHub => init_github_template(config, project_path, is_server_only)?,
        TemplateType::Empty => init_empty(config, project_path)?,
    }

    let cursorrules_content = embedded::get_cursorrules();
    let cursorrules_path = project_path.join(".cursor/rules/spacetimedb.mdc");
    fs::create_dir_all(cursorrules_path.parent().unwrap())?;
    fs::write(cursorrules_path, cursorrules_content)?;

    println!("{}", "Project initialized successfully!".green());
    print_next_steps(config, project_path)?;

    Ok(())
}

fn init_builtin(config: &TemplateConfig, project_path: &Path, is_server_only: bool) -> anyhow::Result<()> {
    let template_def = config
        .template_def
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Template definition missing"))?;

    let template_files = embedded::get_template_files();

    if !is_server_only {
        println!(
            "Setting up client ({})...",
            config.client_lang.map(|l| l.as_str()).unwrap_or("none")
        );
        let client_source = &template_def.client_source;
        if let Some(files) = template_files.get(client_source.as_str()) {
            copy_embedded_files(files, project_path)?;
        } else {
            anyhow::bail!("Client template not found: {}", client_source);
        }

        // Update client name
        match config.client_lang {
            Some(ClientLanguage::TypeScript) => {
                update_package_json(project_path, &config.project_name)?;
                write_typescript_client_env_file(project_path, &config.project_name, config.use_local)?;
                println!(
                    "{}",
                    "Note: Run 'npm install' in the project directory to install dependencies".yellow()
                );
            }
            Some(ClientLanguage::Rust) => {
                update_cargo_toml_name(project_path, &config.project_name)?;
            }
            Some(ClientLanguage::Csharp) => {
                update_csproj_client_to_nuget(project_path)?;
            }
            None => {}
        }
    }

    println!(
        "Setting up server ({})...",
        config.server_lang.map(|l| l.as_str()).unwrap_or("none")
    );
    let server_dir = project_path.join("spacetimedb");
    let server_source = &template_def.server_source;
    if let Some(files) = template_files.get(server_source.as_str()) {
        copy_embedded_files(files, &server_dir)?;
    } else {
        anyhow::bail!("Server template not found: {}", server_source);
    }

    // Update server name
    match config.server_lang {
        Some(ServerLanguage::TypeScript) => {
            update_package_json(&server_dir, &config.project_name)?;
        }
        Some(ServerLanguage::Rust) => {
            update_cargo_toml_name(&server_dir, &config.project_name)?;
        }
        Some(ServerLanguage::Csharp) => {
            update_csproj_server_to_nuget(&server_dir)?;
        }
        None => {}
    }

    Ok(())
}

fn copy_embedded_files(files: &HashMap<&str, &str>, target_dir: &Path) -> anyhow::Result<()> {
    for (file_path, content) in files {
        let full_path = target_dir.join(file_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full_path, content)?;
    }
    Ok(())
}

fn init_github_template(config: &TemplateConfig, project_path: &Path, is_server_only: bool) -> anyhow::Result<()> {
    let repo = config.github_repo.as_ref().unwrap();
    clone_github_template(repo, project_path, is_server_only)?;

    let package_path = project_path.join("package.json");
    if package_path.exists() {
        let content = fs::read_to_string(&package_path)?;
        let mut package: serde_json::Value = serde_json::from_str(&content)?;
        package["name"] = json!(config.project_name.clone());
        let updated_content = serde_json::to_string_pretty(&package)?;
        fs::write(package_path, updated_content)?;
    }

    println!("{}", "Note: Custom templates require manual configuration.".yellow());

    Ok(())
}

fn init_empty(config: &TemplateConfig, project_path: &Path) -> anyhow::Result<()> {
    match config.server_lang {
        Some(ServerLanguage::Rust) => {
            println!("Setting up Rust server...");
            let server_dir = project_path.join("spacetimedb");
            init_empty_rust_server(&server_dir, &config.project_name)?;
        }
        Some(ServerLanguage::Csharp) => {
            println!("Setting up C# server...");
            let server_dir = project_path.join("spacetimedb");
            init_empty_csharp_server(&server_dir, &config.project_name)?;
        }
        Some(ServerLanguage::TypeScript) => {
            println!("Setting up TypeScript server...");
            let server_dir = project_path.join("spacetimedb");
            init_empty_typescript_server(&server_dir, &config.project_name)?;
        }
        None => {}
    }

    Ok(())
}

fn init_empty_rust_server(server_dir: &Path, project_name: &str) -> anyhow::Result<()> {
    init_rust_project(server_dir)?;
    update_cargo_toml_name(server_dir, project_name)?;
    Ok(())
}

fn init_empty_csharp_server(server_dir: &Path, _project_name: &str) -> anyhow::Result<()> {
    init_csharp_project(server_dir)
}

fn init_empty_typescript_server(server_dir: &Path, project_name: &str) -> anyhow::Result<()> {
    init_typescript_project(server_dir)?;
    update_package_json(server_dir, project_name)?;
    Ok(())
}

fn print_next_steps(config: &TemplateConfig, _project_path: &Path) -> anyhow::Result<()> {
    println!();
    println!("{}", "Next steps:".bold());

    let rel_path = config
        .project_path
        .strip_prefix(std::env::current_dir()?)
        .unwrap_or(&config.project_path);

    if rel_path != Path::new(".") && rel_path != Path::new("") {
        println!("  cd {}", rel_path.display());
    }

    match (config.template_type, config.server_lang, config.client_lang) {
        (TemplateType::Builtin, Some(ServerLanguage::Rust), Some(ClientLanguage::Rust)) => {
            println!(
                "  spacetime publish --project-path spacetimedb {}{}",
                if config.use_local { "--server local " } else { "" },
                config.project_name
            );
            println!("  spacetime generate --lang rust --out-dir src/module_bindings --project-path spacetimedb");
            println!("  cargo run");
        }
        (TemplateType::Builtin, Some(ServerLanguage::TypeScript), Some(ClientLanguage::TypeScript)) => {
            println!("  npm install");
            println!(
                "  spacetime publish --project-path spacetimedb {}{}",
                if config.use_local { "--server local " } else { "" },
                config.project_name
            );
            println!("  spacetime generate --lang typescript --out-dir src/module_bindings --project-path spacetimedb");
            println!("  npm run dev");
        }
        (TemplateType::Builtin, Some(ServerLanguage::Csharp), Some(ClientLanguage::Csharp)) => {
            println!(
                "  spacetime publish --project-path spacetimedb {}{}",
                if config.use_local { "--server local " } else { "" },
                config.project_name
            );
            println!("  spacetime generate --lang csharp --out-dir module_bindings --project-path spacetimedb");
        }
        (TemplateType::Empty, _, Some(ClientLanguage::TypeScript)) => {
            println!("  npm install");
            if config.server_lang.is_some() {
                println!(
                    "  spacetime publish --project-path spacetimedb {}{}",
                    if config.use_local { "--server local " } else { "" },
                    config.project_name
                );
                println!(
                    "  spacetime generate --lang typescript --out-dir src/module_bindings --project-path spacetimedb"
                );
            }
            println!("  npm run dev");
        }
        (TemplateType::Empty, _, Some(ClientLanguage::Rust)) => {
            if config.server_lang.is_some() {
                println!(
                    "  spacetime publish --project-path spacetimedb {}{}",
                    if config.use_local { "--server local " } else { "" },
                    config.project_name
                );
                println!("  spacetime generate --lang rust --out-dir src/module_bindings --project-path spacetimedb");
            }
            println!("  cargo run");
        }
        (_, _, _) => {
            println!("  # Follow the template's README for setup instructions");
        }
    }

    println!();
    println!("Learn more: {}", "https://spacetimedb.com/docs".cyan());

    Ok(())
}

fn check_for_cargo() -> bool {
    match std::env::consts::OS {
        "linux" | "freebsd" | "netbsd" | "openbsd" | "solaris" | "macos" => {
            if find_executable("cargo").is_some() {
                return true;
            }
            println!("{}", "Warning: You have created a rust project, but you are missing cargo. You should install cargo with the following command:\n\n\tcurl https://sh.rustup.rs -sSf | sh\n".yellow());
        }
        "windows" => {
            if find_executable("cargo.exe").is_some() {
                return true;
            }
            println!("{}", "Warning: You have created a rust project, but you are missing `cargo`. Visit https://www.rust-lang.org/tools/install for installation instructions:\n\n\tYou have created a rust project, but you are missing cargo.\n".yellow());
        }
        unsupported_os => {
            println!("{}", format!("This OS may be unsupported: {unsupported_os}").yellow());
        }
    }
    false
}

fn check_for_dotnet() -> bool {
    use std::fmt::Write;

    let subpage = match std::env::consts::OS {
        "windows" => {
            if find_executable("dotnet.exe").is_some() {
                return true;
            }
            Some("windows")
        }
        os => {
            if find_executable("dotnet").is_some() {
                return true;
            }
            match os {
                "linux" | "macos" => Some(os),
                // can't give any hint for those other OS
                _ => None,
            }
        }
    };
    let mut msg = "Warning: You have created a C# project, but you are missing dotnet CLI.".to_owned();
    if let Some(subpage) = subpage {
        write!(
            msg,
            " Check out https://docs.microsoft.com/en-us/dotnet/core/install/{subpage}/ for installation instructions."
        )
        .unwrap();
    }
    println!("{}", msg.yellow());
    false
}

fn check_for_git() -> bool {
    match std::env::consts::OS {
        "linux" | "freebsd" | "netbsd" | "openbsd" | "solaris" => {
            if find_executable("git").is_some() {
                return true;
            }
            println!(
                "{}",
                "Warning: Git is not installed. You should install git using your package manager.\n".yellow()
            );
        }
        "macos" => {
            if find_executable("git").is_some() {
                return true;
            }
            println!(
                "{}",
                "Warning: Git is not installed. You can install git by invoking:\n\n\tgit --version\n".yellow()
            );
        }
        "windows" => {
            if find_executable("git.exe").is_some() {
                return true;
            }

            println!("{}", "Warning: You are missing git. You should install git from here:\n\n\thttps://git-scm.com/download/win\n".yellow());
        }
        unsupported_os => {
            println!("{}", format!("This OS may be unsupported: {unsupported_os}").yellow());
        }
    }
    false
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> anyhow::Result<PathBuf> {
    println!("{UNSTABLE_WARNING}\n");

    let is_interactive = !args.get_flag("non-interactive");
    let template = args.get_one::<String>("template");
    let server_lang = args.get_one::<String>("lang");
    let project_name_arg = args.get_one::<String>("project-name");

    // Validate that template and lang options are not used together
    if template.is_some() && server_lang.is_some() {
        anyhow::bail!("Cannot specify both --template and --lang. Language is determined by the template.");
    }

    if !is_interactive {
        // In non-interactive mode, validate all required args are present
        if project_name_arg.is_none() {
            anyhow::bail!("PROJECT_NAME is required in non-interactive mode");
        }
        if template.is_none() && server_lang.is_none() {
            anyhow::bail!("Either --template or --lang must be provided in non-interactive mode");
        }
    }

    exec_init(&mut config, args, is_interactive).await
}

pub fn init_rust_project(project_path: &Path) -> anyhow::Result<()> {
    let export_files = vec![
        (
            include_str!("../../templates/basic-rust/server/Cargo.toml"),
            "Cargo.toml",
        ),
        (
            include_str!("../../templates/basic-rust/server/src/lib.rs"),
            "src/lib.rs",
        ),
        (
            include_str!("../../templates/basic-rust/server/.gitignore"),
            ".gitignore",
        ),
        (
            include_str!("../../templates/basic-rust/server/.cargo/config.toml"),
            ".cargo/config.toml",
        ),
    ];

    for data_file in export_files {
        let path = project_path.join(data_file.1);
        create_directory(path.parent().unwrap())?;
        std::fs::write(path, data_file.0)?;
    }

    check_for_cargo();
    check_for_git();

    Ok(())
}

pub fn init_csharp_project(project_path: &Path) -> anyhow::Result<()> {
    let export_files = vec![
        (
            include_str!("../../templates/basic-c-sharp/server/StdbModule.csproj"),
            "StdbModule.csproj",
        ),
        (include_str!("../../templates/basic-c-sharp/server/Lib.cs"), "Lib.cs"),
        (
            include_str!("../../templates/basic-c-sharp/server/.gitignore"),
            ".gitignore",
        ),
        (
            include_str!("../../templates/basic-c-sharp/server/global.json"),
            "global.json",
        ),
    ];

    check_for_dotnet();
    check_for_git();

    for data_file in export_files {
        let path = project_path.join(data_file.1);
        create_directory(path.parent().unwrap())?;
        std::fs::write(path, data_file.0)?;
    }

    Ok(())
}

pub fn init_typescript_project(project_path: &Path) -> anyhow::Result<()> {
    let export_files = vec![
        (
            include_str!("../../templates/basic-typescript/server/package.json"),
            "package.json",
        ),
        (
            include_str!("../../templates/basic-typescript/server/tsconfig.json"),
            "tsconfig.json",
        ),
        (
            include_str!("../../templates/basic-typescript/server/src/index.ts"),
            "src/index.ts",
        ),
        (
            include_str!("../../templates/basic-typescript/server/.gitignore"),
            ".gitignore",
        ),
    ];

    check_for_git();

    for data_file in export_files {
        let path = project_path.join(data_file.1);
        create_directory(path.parent().unwrap())?;
        std::fs::write(path, data_file.0)?;
    }

    Ok(())
}

pub async fn exec_init_rust(args: &ArgMatches) -> anyhow::Result<()> {
    let project_path = args.get_one::<PathBuf>("project-path").unwrap();
    init_rust_project(project_path)?;

    println!(
        "{}",
        format!("Project successfully created at path: {}", project_path.display()).green()
    );

    Ok(())
}

pub async fn exec_init_csharp(args: &ArgMatches) -> anyhow::Result<()> {
    let project_path = args.get_one::<PathBuf>("project-path").unwrap();
    init_csharp_project(project_path)?;

    println!(
        "{}",
        format!("Project successfully created at path: {}", project_path.display()).green()
    );

    Ok(())
}

fn create_directory(path: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(path).context("Failed to create directory")
}

pub fn parse_server_lang(lang: &Option<String>) -> anyhow::Result<Option<ServerLanguage>> {
    match lang.as_deref() {
        Some(s) => Ok(ServerLanguage::from_str(s)?),
        None => Ok(None),
    }
}

pub fn parse_client_lang(lang: &Option<String>) -> anyhow::Result<Option<ClientLanguage>> {
    match lang.as_deref() {
        Some(s) => Ok(ClientLanguage::from_str(s)?),
        None => Ok(None),
    }
}

fn edition_uses_workspace(item: &Item) -> bool {
    match item {
        Item::Value(val) => val
            .as_inline_table()
            .map(|table| table.get("workspace").is_some())
            .unwrap_or(false),
        Item::Table(table) => table.get("workspace").is_some(),
        _ => false,
    }
}

fn dependency_uses_workspace(item: &Item) -> bool {
    uses_workspace(item) || has_path(item)
}

fn uses_workspace(item: &Item) -> bool {
    match item {
        Item::Value(val) => val
            .as_inline_table()
            .map(|table| table.get("workspace").is_some())
            .unwrap_or(false),
        Item::Table(table) => table.get("workspace").is_some(),
        _ => false,
    }
}

fn has_path(item: &Item) -> bool {
    match item {
        Item::Value(val) => val
            .as_inline_table()
            .map(|table| table.get("path").is_some())
            .unwrap_or(false),
        Item::Table(table) => table.get("path").is_some(),
        _ => false,
    }
}

fn set_dependency_version(item: &mut Item, version: &str, remove_path: bool) {
    if let Item::Value(val) = item {
        if let Some(inline) = val.as_inline_table_mut() {
            inline.remove("workspace");
            if remove_path {
                inline.remove("path");
            }
            inline.insert("version", toml_edit::Value::from(version.to_string()));
            return;
        }
    }

    if let Item::Table(table) = item {
        table.remove("workspace");
        if remove_path {
            table.remove("path");
        }
        table["version"] = value(version.to_string());
        return;
    }

    *item = value(version.to_string());
}
