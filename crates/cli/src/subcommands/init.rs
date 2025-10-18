use crate::Config;
use crate::{detect::find_executable, util::UNSTABLE_WARNING};
use anyhow::Context;
use clap::{Arg, ArgMatches};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use regex::Regex;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::subcommands::login::{spacetimedb_login_force, DEFAULT_AUTH_HOST};

const DEFAULT_TEMPLATES_REPO: &str = "clockworklabs/SpacetimeDB";
const DEFAULT_TEMPLATES_BRANCH: &str = env!("GIT_HASH");
const TEMPLATES_FILE_PATH: &str = "crates/cli/.init-templates.json";
const TYPESCRIPT_BINDINGS_PACKAGE_JSON: &str = include_str!("../../../bindings-typescript/package.json");

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
    None,
}

impl ServerLanguage {
    fn as_str(&self) -> &'static str {
        match self {
            ServerLanguage::Rust => "rust",
            ServerLanguage::Csharp => "csharp",
            ServerLanguage::TypeScript => "typescript",
            ServerLanguage::None => "none",
        }
    }

    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "rust" => ServerLanguage::Rust,
            "csharp" | "c#" => ServerLanguage::Csharp,
            "typescript" => ServerLanguage::TypeScript,
            _ => ServerLanguage::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientLanguage {
    Rust,
    Csharp,
    TypeScript,
    None,
}

impl ClientLanguage {
    fn as_str(&self) -> &'static str {
        match self {
            ClientLanguage::Rust => "rust",
            ClientLanguage::Csharp => "csharp",
            ClientLanguage::TypeScript => "typescript",
            ClientLanguage::None => "none",
        }
    }

    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "rust" => ClientLanguage::Rust,
            "csharp" | "c#" => ClientLanguage::Csharp,
            "typescript" => ClientLanguage::TypeScript,
            _ => ClientLanguage::None,
        }
    }
}

pub struct TemplateConfig {
    pub project_name: String,
    pub project_path: PathBuf,
    pub template_type: TemplateType,
    pub server_lang: ServerLanguage,
    pub client_lang: ClientLanguage,
    pub github_repo: Option<String>,
    pub template_def: Option<TemplateDefinition>,
    pub use_local: bool,
}

pub fn cli() -> clap::Command {
    clap::Command::new("init")
        .about(format!("Initializes a new spacetime project. {UNSTABLE_WARNING}"))
        .arg(
            Arg::new("name")
                .short('n')
                .long("name")
                .value_name("NAME")
                .help("Project name"),
        )
        .arg(
            Arg::new("project-path")
                .value_parser(clap::value_parser!(PathBuf))
                .help("The path where we will create the spacetime project (defaults to hyphenated project name)"),
        )
        .arg(
            Arg::new("server-lang")
                .long("server-lang")
                .value_name("LANG")
                .help("Server language: rust, csharp, typescript"),
        )
        .arg(
            Arg::new("template")
                .short('t')
                .long("template")
                .value_name("TEMPLATE")
                .help("Template ID or GitHub repository (owner/repo or URL)"),
        )
        .arg(
            Arg::new("client-lang")
                .long("client-lang")
                .value_name("LANG")
                .help("Client language: rust, csharp, typescript, none"),
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
                .help("Run in non-interactive mode with default or provided options"),
        )
}

pub async fn fetch_templates_list() -> anyhow::Result<(Vec<HighlightDefinition>, Vec<TemplateDefinition>)> {
    let content = if let Ok(file_path) = env::var("SPACETIMEDB_CLI_TEMPLATES_FILE") {
        eprintln!("Loading templates list from local file: {}", file_path);
        std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read templates file at {}", file_path))?
    } else {
        let repo =
            env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_REPO").unwrap_or_else(|_| DEFAULT_TEMPLATES_REPO.to_string());
        let branch =
            env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_BRANCH").unwrap_or_else(|_| DEFAULT_TEMPLATES_BRANCH.to_string());

        let url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}",
            repo, branch, TEMPLATES_FILE_PATH
        );

        eprintln!("Fetching templates list from {}...", url);

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch templates list from GitHub")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to fetch templates list: HTTP {}", response.status());
        }

        response
            .text()
            .await
            .context("Failed to read templates list response")?
    };

    let templates_list: TemplatesList =
        serde_json::from_str(&content).context("Failed to parse templates list JSON")?;

    Ok((templates_list.highlights, templates_list.templates))
}

pub async fn check_and_prompt_login(config: &mut Config) -> anyhow::Result<bool> {
    if config.spacetimedb_token().is_some() {
        eprintln!("{}", "You are logged in to SpacetimeDB.".green());
        return Ok(true);
    }

    eprintln!("{}", "You are not logged in to SpacetimeDB.".yellow());

    let theme = ColorfulTheme::default();
    let should_login = Confirm::with_theme(&theme)
        .with_prompt("Would you like to log in? (required for Maincloud deployment)")
        .default(true)
        .interact()?;

    if should_login {
        let host = Url::parse(DEFAULT_AUTH_HOST)?;
        spacetimedb_login_force(config, &host, false).await?;
        eprintln!("{}", "Successfully logged in!".green());
        Ok(true)
    } else {
        eprintln!("{}", "Continuing with local deployment.".yellow());
        Ok(false)
    }
}

pub async fn exec_interactive_init(config: &mut Config, args: &ArgMatches) -> anyhow::Result<()> {
    let use_local = if args.get_flag("local") {
        true
    } else {
        !check_and_prompt_login(config).await?
    };

    let mut template_config = interactive_init_with_args(args).await?;
    template_config.use_local = use_local;

    ensure_empty_directory(&template_config.project_name, &template_config.project_path)?;

    init_from_template(&template_config, &template_config.project_path).await?;

    Ok(())
}

pub async fn exec_non_interactive_init(config: &mut Config, args: &ArgMatches) -> anyhow::Result<()> {
    let use_local = if args.get_flag("local") {
        true
    } else {
        !check_and_prompt_login(config).await?
    };

    // Get project name - required in non-interactive mode
    let project_name = args
        .get_one::<String>("name")
        .expect("name is required in non-interactive mode")
        .clone();

    // Determine project path - use provided path or derive from name
    let actual_project_path = if let Some(path) = args.get_one::<PathBuf>("project-path") {
        path.clone()
    } else {
        PathBuf::from(&project_name)
    };

    // Check if template is provided
    if let Some(template_str) = args.get_one::<String>("template") {
        // Check if it's a builtin template
        let (_, templates) = fetch_templates_list().await?;

        if let Some(template) = templates.iter().find(|t| t.id == *template_str) {
            // Builtin template
            let template_config = TemplateConfig {
                project_name: project_name.clone(),
                project_path: actual_project_path.clone(),
                template_type: TemplateType::Builtin,
                server_lang: ServerLanguage::from_str(template.server_lang.as_deref().unwrap_or("none")),
                client_lang: ClientLanguage::from_str(template.client_lang.as_deref().unwrap_or("none")),
                github_repo: None,
                template_def: Some(template.clone()),
                use_local,
            };

            ensure_empty_directory(&project_name, &actual_project_path)?;
            init_from_template(&template_config, &actual_project_path).await?;
        } else {
            // GitHub template
            let template_config = TemplateConfig {
                project_name: project_name.clone(),
                project_path: actual_project_path.clone(),
                template_type: TemplateType::GitHub,
                server_lang: ServerLanguage::Rust,
                client_lang: ClientLanguage::None,
                github_repo: Some(template_str.clone()),
                template_def: None,
                use_local,
            };

            ensure_empty_directory(&project_name, &actual_project_path)?;
            init_from_template(&template_config, &actual_project_path).await?;
        }
    } else {
        // No template - require at least one language option
        let server_lang_str = args.get_one::<String>("server-lang");
        let client_lang_str = args.get_one::<String>("client-lang");

        if server_lang_str.is_none() && client_lang_str.is_none() {
            anyhow::bail!(
                "Either --template, --server-lang, or --client-lang must be provided in non-interactive mode"
            );
        }

        // Determine server language
        let server_lang = if let Some(lang_str) = server_lang_str {
            ServerLanguage::from_str(lang_str)
        } else {
            ServerLanguage::None
        };

        // Determine client language
        let client_lang = if let Some(lang_str) = client_lang_str {
            ClientLanguage::from_str(lang_str)
        } else {
            ClientLanguage::None
        };

        let template_config = TemplateConfig {
            project_name: project_name.clone(),
            project_path: actual_project_path.clone(),
            template_type: TemplateType::Empty,
            server_lang,
            client_lang,
            github_repo: None,
            template_def: None,
            use_local,
        };

        ensure_empty_directory(&project_name, &actual_project_path)?;
        init_from_template(&template_config, &actual_project_path).await?;
    }

    Ok(())
}

pub fn ensure_empty_directory(_project_name: &str, project_path: &Path) -> anyhow::Result<()> {
    if project_path.exists() {
        if !project_path.is_dir() {
            anyhow::bail!(
                "Path {} exists but is not a directory. A new SpacetimeDB project must be initialized in an empty directory.",
                project_path.display()
            );
        }

        if std::fs::read_dir(project_path).unwrap().count() > 0 {
            anyhow::bail!(
                "Cannot create new SpacetimeDB project in non-empty directory: {}",
                project_path.display()
            );
        }
    } else {
        fs::create_dir_all(project_path).context("Failed to create directory")?;
    }
    Ok(())
}

pub async fn interactive_init_with_args(args: &ArgMatches) -> anyhow::Result<TemplateConfig> {
    let theme = ColorfulTheme::default();

    // Get or prompt for project name
    let project_name: String = if let Some(name) = args.get_one::<String>("name") {
        eprintln!("{} {}", "Project name:".bold(), name);
        name.clone()
    } else {
        Input::with_theme(&theme)
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
            .to_string()
    };

    // Get or prompt for project path
    let project_path: String = if let Some(path) = args.get_one::<PathBuf>("project-path") {
        let path_str = path.to_string_lossy().to_string();
        eprintln!("{} {}", "Project path:".bold(), path_str);
        path_str
    } else {
        Input::with_theme(&theme)
            .with_prompt("Project path")
            .default(format!("./{}", project_name))
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
                            if entries.count() > 0 {
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
            .to_string()
    };

    // Check if template is provided
    if let Some(template_str) = args.get_one::<String>("template") {
        eprintln!("{} {}", "Template:".bold(), template_str);

        let (_, templates) = fetch_templates_list().await?;

        if let Some(template) = templates.iter().find(|t| t.id == *template_str) {
            // Builtin template
            return Ok(TemplateConfig {
                project_name,
                project_path: PathBuf::from(project_path),
                template_type: TemplateType::Builtin,
                server_lang: ServerLanguage::from_str(template.server_lang.as_deref().unwrap_or("none")),
                client_lang: ClientLanguage::from_str(template.client_lang.as_deref().unwrap_or("none")),
                github_repo: None,
                template_def: Some(template.clone()),
                use_local: true,
            });
        } else {
            // GitHub template
            return Ok(TemplateConfig {
                project_name,
                project_path: PathBuf::from(project_path),
                template_type: TemplateType::GitHub,
                server_lang: ServerLanguage::Rust,
                client_lang: ClientLanguage::None,
                github_repo: Some(template_str.clone()),
                template_def: None,
                use_local: true,
            });
        }
    }

    // Check if server-lang or client-lang is provided
    let server_lang_arg = args.get_one::<String>("server-lang");
    let client_lang_arg = args.get_one::<String>("client-lang");

    if server_lang_arg.is_some() || client_lang_arg.is_some() {
        // Use provided languages
        let server_lang = if let Some(lang_str) = server_lang_arg {
            eprintln!("{} {}", "Server language:".bold(), lang_str);
            ServerLanguage::from_str(lang_str)
        } else {
            ServerLanguage::None
        };

        let client_lang = if let Some(lang_str) = client_lang_arg {
            eprintln!("{} {}", "Client language:".bold(), lang_str);
            ClientLanguage::from_str(lang_str)
        } else {
            ClientLanguage::None
        };

        return Ok(TemplateConfig {
            project_name,
            project_path: PathBuf::from(project_path),
            template_type: TemplateType::Empty,
            server_lang,
            client_lang,
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
    client_choices.push("other".to_string());
    client_choices.push("none".to_string());

    let client_selection = Select::with_theme(&theme)
        .with_prompt("Select client")
        .items(&client_choices)
        .default(0)
        .interact()?;

    let other_index = highlights.len();
    let _none_index = highlights.len() + 1;

    if client_selection < highlights.len() {
        let highlight = &highlights[client_selection];
        let template = templates
            .iter()
            .find(|t| t.id == highlight.template_id)
            .ok_or_else(|| anyhow::anyhow!("Template {} not found", highlight.template_id))?;

        Ok(TemplateConfig {
            project_name,
            project_path: PathBuf::from(project_path),
            template_type: TemplateType::Builtin,
            server_lang: ServerLanguage::from_str(template.server_lang.as_deref().unwrap_or("none")),
            client_lang: ClientLanguage::from_str(template.client_lang.as_deref().unwrap_or("none")),
            github_repo: None,
            template_def: Some(template.clone()),
            use_local: true,
        })
    } else if client_selection == other_index {
        loop {
            let template_id: String = Input::<String>::with_theme(&theme)
                .with_prompt("Template ID or GitHub repository (owner/repo). Press 'l' to list available templates")
                .interact_text()?
                .trim()
                .to_string();

            if template_id == "l" || template_id == "L" {
                eprintln!("\n{}", "Available templates:".bold());
                for template in &templates {
                    eprintln!("  {} - {}", template.id, template.description);
                }
                eprintln!();
                continue;
            }

            if let Some(template) = templates.iter().find(|t| t.id == template_id) {
                return Ok(TemplateConfig {
                    project_name: project_name.clone(),
                    project_path: PathBuf::from(&project_path),
                    template_type: TemplateType::Builtin,
                    server_lang: ServerLanguage::from_str(template.server_lang.as_deref().unwrap_or("none")),
                    client_lang: ClientLanguage::from_str(template.client_lang.as_deref().unwrap_or("none")),
                    github_repo: None,
                    template_def: Some(template.clone()),
                    use_local: true,
                });
            } else {
                return Ok(TemplateConfig {
                    project_name: project_name.clone(),
                    project_path: PathBuf::from(&project_path),
                    template_type: TemplateType::GitHub,
                    server_lang: ServerLanguage::Rust,
                    client_lang: ClientLanguage::None,
                    github_repo: Some(template_id),
                    template_def: None,
                    use_local: true,
                });
            }
        }
    } else {
        let server_lang_choices = vec!["Rust", "C#", "TypeScript"];
        let server_lang_selection = Select::with_theme(&theme)
            .with_prompt("Select server language")
            .items(&server_lang_choices)
            .default(0)
            .interact()?;

        let server_lang = match server_lang_selection {
            0 => ServerLanguage::Rust,
            1 => ServerLanguage::Csharp,
            2 => ServerLanguage::TypeScript,
            _ => ServerLanguage::Rust,
        };

        Ok(TemplateConfig {
            project_name,
            project_path: PathBuf::from(project_path),
            template_type: TemplateType::Empty,
            server_lang,
            client_lang: ClientLanguage::None,
            github_repo: None,
            template_def: None,
            use_local: true,
        })
    }
}

fn clone_git_subdirectory(repo_url: &str, subdir: &str, target: &Path, branch: Option<&str>) -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.path();

    let branch_display = branch.map(|b| format!(" (branch: {})", b)).unwrap_or_default();
    eprintln!("  Cloning repository from {}{}...", repo_url, branch_display);

    let mut builder = git2::build::RepoBuilder::new();

    if let Some(branch_name) = branch {
        builder.branch(branch_name);
    }

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
        .clone(repo_url, temp_path)
        .context("Failed to clone repository")?;

    let source_path = temp_path.join(subdir);
    if !source_path.exists() {
        anyhow::bail!("Subdirectory '{}' not found in repository", subdir);
    }

    copy_dir_all(&source_path, target)?;

    Ok(())
}

fn clone_github_template(repo_input: &str, target: &Path) -> anyhow::Result<()> {
    let repo_url = if repo_input.starts_with("http") {
        repo_input.to_string()
    } else if repo_input.contains('/') {
        format!("https://github.com/{}", repo_input)
    } else {
        anyhow::bail!("Invalid repository format. Use 'owner/repo' or full URL");
    };

    eprintln!("  Cloning from {}...", repo_url);

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

    copy_dir_all(temp_dir.path(), target)?;

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

fn configure_rust_server(server_dir: &Path, project_name: &str) -> anyhow::Result<()> {
    let cargo_path = server_dir.join("Cargo.toml");
    if !cargo_path.exists() {
        return Ok(());
    }

    let mut content = fs::read_to_string(&cargo_path)?;

    let safe_name = project_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect::<String>();

    let name_regex = Regex::new(r#"(?m)^name = .*$"#)?;
    content = name_regex
        .replace(&content, format!(r#"name = "{}""#, safe_name))
        .to_string();

    fs::write(&cargo_path, content)?;
    Ok(())
}

fn create_root_package_json(root: &Path, project_name: &str, _use_local: bool) -> anyhow::Result<()> {
    let package_json = json!({
        "name": project_name,
        "version": "0.1.0",
        "private": true,
        "scripts": {
            "dev": "cd client && npm run dev",
            "build": "cd spacetimedb && spacetime build && cd ../client && npm run build",
            "deploy": format!("npm run build && spacetime publish --project-path spacetimedb --server maincloud {} && spacetime generate --project-path spacetimedb --lang typescript --out-dir client/src/module_bindings", project_name),
            "local": format!("npm run build && spacetime publish --project-path spacetimedb --server local {} --yes && spacetime generate --project-path spacetimedb --lang typescript --out-dir client/src/module_bindings", project_name)
        },
        "workspaces": ["client"]
    });

    let package_path = root.join("package.json");
    let content = serde_json::to_string_pretty(&package_json)?;
    fs::write(package_path, content)?;

    Ok(())
}

fn get_spacetimedb_typescript_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let package: serde_json::Value = serde_json::from_str(TYPESCRIPT_BINDINGS_PACKAGE_JSON)
            .expect("Failed to parse TypeScript bindings package.json");
        package["version"]
            .as_str()
            .expect("Version not found in package.json")
            .to_string()
    })
}

fn update_client_package_json(client_dir: &Path, project_name: &str) -> anyhow::Result<()> {
    let package_path = client_dir.join("package.json");
    if !package_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&package_path)?;
    let mut package: serde_json::Value = serde_json::from_str(&content)?;

    package["name"] = json!(format!("{}-client", project_name));

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

fn update_typescript_client_config(client_dir: &Path, module_name: &str, use_local: bool) -> anyhow::Result<()> {
    let main_path = client_dir.join("src/main.tsx");
    if !main_path.exists() {
        return Ok(());
    }

    let mut content = fs::read_to_string(&main_path)?;

    let target_uri = if use_local {
        "ws://localhost:3000"
    } else {
        "wss://maincloud.spacetimedb.com"
    };

    let module_regex = Regex::new(r#"\.withModuleName\(['"][^'"]*['"]\)"#)?;
    content = module_regex
        .replace_all(&content, format!(r#".withModuleName('{}')"#, module_name))
        .to_string();

    let uri_regex = Regex::new(r#"\.withUri\(['"]ws://localhost:3000['"]\)"#)?;
    content = uri_regex
        .replace_all(&content, format!(r#".withUri('{}')"#, target_uri))
        .to_string();

    fs::write(main_path, content)?;

    Ok(())
}

async fn copy_cursorrules(project_path: &Path) -> anyhow::Result<()> {
    let repo = env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_REPO").unwrap_or_else(|_| DEFAULT_TEMPLATES_REPO.to_string());
    let branch =
        env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_BRANCH").unwrap_or_else(|_| DEFAULT_TEMPLATES_BRANCH.to_string());

    let url = format!(
        "https://raw.githubusercontent.com/{}/{}/docs/.cursor/rules/spacetimedb.md",
        repo, branch
    );

    let client = reqwest::Client::new();
    match client.get(&url).send().await {
        Ok(response) if response.status().is_success() => {
            let content = response.text().await?;
            let cursorrules_path = project_path.join(".cursorrules");
            fs::write(cursorrules_path, content)?;
        }
        _ => {
            // Silently skip if file doesn't exist or can't be fetched
        }
    }

    Ok(())
}

pub async fn init_from_template(config: &TemplateConfig, project_path: &Path) -> anyhow::Result<()> {
    eprintln!("{}", "Initializing project from template...".cyan());

    match config.template_type {
        TemplateType::Builtin => init_builtin(config, project_path)?,
        TemplateType::GitHub => init_github_template(config, project_path)?,
        TemplateType::Empty => init_empty(config, project_path)?,
    }

    // Copy .cursorrules file from the repository
    copy_cursorrules(project_path).await?;

    eprintln!("{}", "Project initialized successfully!".green());
    print_next_steps(config, project_path)?;

    Ok(())
}

fn init_builtin(config: &TemplateConfig, project_path: &Path) -> anyhow::Result<()> {
    let template_def = config
        .template_def
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Template definition missing"))?;

    // Use the same branch as the templates list if specified
    let branch = env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_BRANCH").ok().or_else(|| {
        if env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_REPO").is_ok() {
            None
        } else {
            Some(DEFAULT_TEMPLATES_BRANCH.to_string())
        }
    });

    eprintln!("Setting up client ({})...", config.client_lang.as_str());
    let client_source = &template_def.client_source;
    let (repo, subdir) = parse_repo_source(client_source);
    clone_git_subdirectory(
        &format!("https://github.com/{}", repo),
        subdir,
        project_path,
        branch.as_deref(),
    )?;

    eprintln!("Setting up server ({})...", config.server_lang.as_str());
    let server_dir = project_path.join("spacetimedb");
    let server_source = &template_def.server_source;
    let (repo, subdir) = parse_repo_source(server_source);
    clone_git_subdirectory(
        &format!("https://github.com/{}", repo),
        subdir,
        &server_dir,
        branch.as_deref(),
    )?;

    // TODO: figure out adjustments we may need to do for other client and server langs
    if config.server_lang == ServerLanguage::Rust {
        configure_rust_server(&server_dir, &config.project_name)?;
    }

    if config.client_lang == ClientLanguage::TypeScript {
        update_client_package_json(project_path, &config.project_name)?;
        update_typescript_client_config(project_path, &config.project_name, config.use_local)?;
        eprintln!(
            "{}",
            "Note: Run 'npm install' in the project directory to install dependencies".yellow()
        );
    }

    Ok(())
}

fn parse_repo_source(source: &str) -> (String, &str) {
    let parts: Vec<&str> = source.splitn(3, '/').collect();
    if parts.len() >= 3 {
        let repo = format!("{}/{}", parts[0], parts[1]);
        let subdir = parts[2];
        return (repo, subdir);
    }
    (source.to_string(), "")
}

fn init_github_template(config: &TemplateConfig, project_path: &Path) -> anyhow::Result<()> {
    let repo = config.github_repo.as_ref().unwrap();
    clone_github_template(repo, project_path)?;

    let package_path = project_path.join("package.json");
    if package_path.exists() {
        let content = fs::read_to_string(&package_path)?;
        let mut package: serde_json::Value = serde_json::from_str(&content)?;
        package["name"] = json!(config.project_name.clone());
        let updated_content = serde_json::to_string_pretty(&package)?;
        fs::write(package_path, updated_content)?;
    }

    eprintln!("{}", "Note: Custom templates require manual configuration.".yellow());

    Ok(())
}

fn init_empty(config: &TemplateConfig, project_path: &Path) -> anyhow::Result<()> {
    match config.server_lang {
        ServerLanguage::Rust => {
            eprintln!("Setting up Rust server...");
            let server_dir = project_path.join("spacetimedb");
            init_empty_rust_server(&server_dir, &config.project_name)?;
        }
        ServerLanguage::Csharp => {
            eprintln!("Setting up C# server...");
            let server_dir = project_path.join("spacetimedb");
            init_empty_csharp_server(&server_dir, &config.project_name)?;
        }
        ServerLanguage::TypeScript => {
            eprintln!("Setting up TypeScript server...");
            let server_dir = project_path.join("spacetimedb");
            init_empty_typescript_server(&server_dir, &config.project_name)?;
        }
        ServerLanguage::None => {}
    }

    match config.client_lang {
        ClientLanguage::TypeScript => {
            eprintln!("Setting up TypeScript client...");
            let client_dir = project_path.join("client");

            let branch = env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_BRANCH").ok().or_else(|| {
                if env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_REPO").is_ok() {
                    None
                } else {
                    Some(DEFAULT_TEMPLATES_BRANCH.to_string())
                }
            });

            clone_git_subdirectory(
                "https://github.com/clockworklabs/SpacetimeDB",
                "crates/bindings-typescript/examples/empty",
                &client_dir,
                branch.as_deref(),
            )?;

            update_client_package_json(&client_dir, &config.project_name)?;

            if config.server_lang != ServerLanguage::None {
                // Create package.json with boilerplate for working with the server (like
                // `spacetime publish`
                create_root_package_json(project_path, &config.project_name, config.use_local)?;
            }

            eprintln!(
                "{}",
                "Note: Run 'npm install' in the project directory to install dependencies".yellow()
            );
        }
        ClientLanguage::Rust => {
            eprintln!("Setting up Rust client...");
            eprintln!("{}", "Rust client setup not yet implemented".yellow());
        }
        ClientLanguage::Csharp => {
            eprintln!("Setting up C# client...");
            eprintln!("{}", "C# client setup not yet implemented".yellow());
        }
        ClientLanguage::None => {}
    }

    Ok(())
}

fn init_empty_rust_server(server_dir: &Path, _project_name: &str) -> anyhow::Result<()> {
    init_rust_project(server_dir)
}

fn init_empty_csharp_server(server_dir: &Path, _project_name: &str) -> anyhow::Result<()> {
    init_csharp_project(server_dir)
}

fn init_empty_typescript_server(_server_dir: &Path, _project_name: &str) -> anyhow::Result<()> {
    todo!()
}

fn print_next_steps(config: &TemplateConfig, _project_path: &Path) -> anyhow::Result<()> {
    eprintln!();
    eprintln!("{}", "Next steps:".bold());

    let rel_path = config
        .project_path
        .strip_prefix(std::env::current_dir()?)
        .unwrap_or(&config.project_path);

    if rel_path != Path::new(".") && rel_path != Path::new("") {
        eprintln!("  cd {}", rel_path.display());
    }

    match (config.template_type, config.server_lang, config.client_lang) {
        (TemplateType::Builtin, ServerLanguage::Rust, ClientLanguage::Rust) => {
            eprintln!(
                "  spacetime publish --project-path spacetimedb {}{}",
                if config.use_local { "--server local " } else { "" },
                config.project_name
            );
            eprintln!("  spacetime generate --lang rust --out-dir src/module_bindings --project-path spacetimedb");
            eprintln!("  cargo run");
        }
        (TemplateType::Builtin, ServerLanguage::TypeScript, ClientLanguage::TypeScript) => {
            eprintln!("  npm install");
            eprintln!(
                "  spacetime publish --project-path spacetimedb {}{}",
                if config.use_local { "--server local " } else { "" },
                config.project_name
            );
            eprintln!(
                "  spacetime generate --lang typescript --out-dir src/module_bindings --project-path spacetimedb"
            );
            eprintln!("  npm run dev");
        }
        (TemplateType::Builtin, ServerLanguage::Csharp, ClientLanguage::Csharp) => {
            eprintln!(
                "  spacetime publish --project-path spacetimedb {}{}",
                if config.use_local { "--server local " } else { "" },
                config.project_name
            );
            eprintln!("  spacetime generate --lang csharp --out-dir src/module_bindings --project-path spacetimedb");
        }
        (TemplateType::Empty, _, ClientLanguage::TypeScript) => {
            eprintln!("  npm install");
            if config.server_lang != ServerLanguage::None {
                eprintln!(
                    "  spacetime publish --project-path spacetimedb {}{}",
                    if config.use_local { "--server local " } else { "" },
                    config.project_name
                );
                eprintln!(
                    "  spacetime generate --lang typescript --out-dir src/module_bindings --project-path spacetimedb"
                );
            }
            eprintln!("  npm run dev");
        }
        (TemplateType::Empty, _, ClientLanguage::Rust) => {
            if config.server_lang != ServerLanguage::None {
                eprintln!(
                    "  spacetime publish --project-path spacetimedb {}{}",
                    if config.use_local { "--server local " } else { "" },
                    config.project_name
                );
                eprintln!("  spacetime generate --lang rust --out-dir src/module_bindings --project-path spacetimedb");
            }
            eprintln!("  cargo run");
        }
        (_, _, _) => {
            eprintln!("  # Follow the template's README for setup instructions");
        }
    }

    eprintln!();
    eprintln!("Learn more: {}", "https://spacetimedb.com/docs".cyan());

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

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    eprintln!("{UNSTABLE_WARNING}\n");

    let non_interactive = args.get_flag("non-interactive");
    let template = args.get_one::<String>("template");
    let server_lang = args.get_one::<String>("server-lang");
    let client_lang = args.get_one::<String>("client-lang");
    let name = args.get_one::<String>("name");

    if non_interactive {
        // In non-interactive mode, validate all required args are present
        if name.is_none() {
            anyhow::bail!("--name is required in non-interactive mode");
        }
        if template.is_none() && server_lang.is_none() && client_lang.is_none() {
            anyhow::bail!(
                "Either --template, --server-lang, or --client-lang must be provided in non-interactive mode"
            );
        }
        return exec_non_interactive_init(&mut config, args).await;
    }

    // Interactive or partial interactive mode
    exec_interactive_init(&mut config, args).await
}

pub fn init_rust_project(project_path: &Path) -> Result<(), anyhow::Error> {
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

pub fn init_csharp_project(project_path: &Path) -> Result<(), anyhow::Error> {
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

pub async fn exec_init_rust(args: &ArgMatches) -> Result<(), anyhow::Error> {
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

fn create_directory(path: &Path) -> Result<(), anyhow::Error> {
    std::fs::create_dir_all(path).context("Failed to create directory")
}
