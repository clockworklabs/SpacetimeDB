use crate::Config;
use crate::{detect::find_executable, util::UNSTABLE_WARNING};
use anyhow::anyhow;
use anyhow::Context;
use clap::{Arg, ArgMatches};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use git2::{Cred, FetchOptions};
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
const DEFAULT_TEMPLATES_REFERENCE: &str = env!("GIT_HASH");
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
            Arg::new("server-lang").long("server-lang").value_name("LANG").help(
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
            Arg::new("client-lang").long("client-lang").value_name("LANG").help(
                "Client language: rust, csharp, typescript (it can only be used when --template is not specified)",
            ),
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
    let content = if let Ok(file_path) = env::var("SPACETIMEDB_CLI_TEMPLATES_FILE") {
        println!("Loading templates list from local file: {}", file_path);
        std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read templates file at {}", file_path))?
    } else {
        let repo =
            env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_REPO").unwrap_or_else(|_| DEFAULT_TEMPLATES_REPO.to_string());
        let branch = env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_REFERENCE")
            .unwrap_or_else(|_| DEFAULT_TEMPLATES_REFERENCE.to_string());

        let url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}",
            repo, branch, TEMPLATES_FILE_PATH
        );

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
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c.is_whitespace() || c == '_' {
                '-'
            } else {
                c
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

async fn get_project_name(args: &ArgMatches, is_interactive: bool) -> anyhow::Result<String> {
    if let Some(name) = args.get_one::<String>("name") {
        if is_interactive {
            println!("{} {}", "Project name:".bold(), name);
        }
        return Ok(name.clone());
    }

    if !is_interactive {
        anyhow::bail!("--name is required in non-interactive mode");
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

async fn get_project_path(args: &ArgMatches, project_name: &str, is_interactive: bool) -> anyhow::Result<PathBuf> {
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

fn install_typescript_dependencies(server_dir: &Path, is_interactive: bool) -> anyhow::Result<()> {
    println!(
        "\n{}",
        "TypeScript server requires dependencies to be installed before publ
    ishing."
            .yellow()
    );

    let package_manager = if is_interactive {
        let theme = ColorfulTheme::default();
        let choices = vec!["npm", "pnpm", "yarn", "bun", "other (I'll install manually)"];
        let selection = Select::with_theme(&theme)
            .with_prompt("Which package manager would you like to use?")
            .items(&choices)
            .default(0)
            .interact()?;

        match selection {
            0 => Some("npm"),
            1 => Some("pnpm"),
            2 => Some("yarn"),
            3 => Some("bun"),
            _ => None,
        }
    } else {
        // In non-interactive mode, just print a message
        None
    };

    if let Some(pm) = package_manager {
        println!("Installing dependencies with {}...", pm);
        let output = std::process::Command::new(pm)
            .arg("install")
            .current_dir(server_dir)
            .output()?;

        if output.status.success() {
            println!("{}", "Dependencies installed successfully!".green());
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("{}", format!("Failed to install dependencies: {}", stderr).red());
            println!(
                "{}",
                format!(
                    "Please run '{} install' in the {} directory manually.",
                    pm,
                    server_dir.display()
                )
                .yellow()
            );
        }
    } else {
        println!(
            "{}",
            format!(
                "Please install dependencies by running your package manager's install command in the {} directory.",
                server_dir.display()
            )
            .yellow()
        );
    }

    Ok(())
}

pub async fn exec_init(config: &mut Config, args: &ArgMatches, is_interactive: bool) -> anyhow::Result<()> {
    let use_local = if args.get_flag("local") {
        true
    } else {
        !check_and_prompt_login(config).await?
    };

    let project_name = get_project_name(args, is_interactive).await?;
    let project_path = get_project_path(args, &project_name, is_interactive).await?;

    let mut template_config = if is_interactive {
        get_template_config_interactive(args, project_name, project_path).await?
    } else {
        get_template_config_non_interactive(args, project_name, project_path).await?
    };

    template_config.use_local = use_local;

    ensure_empty_directory(&template_config.project_name, &template_config.project_path)?;
    init_from_template(&template_config, &template_config.project_path).await?;

    // If server is TypeScript, handle dependency installation
    if template_config.server_lang == Some(ServerLanguage::TypeScript) {
        let server_dir = template_config.project_path.join("spacetimedb");
        install_typescript_dependencies(&server_dir, is_interactive)?;
    }

    Ok(())
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
    let server_lang_str = args.get_one::<String>("server-lang").cloned();
    let client_lang_str = args.get_one::<String>("client-lang").cloned();

    if server_lang_str.is_none() && client_lang_str.is_none() {
        anyhow::bail!("Either --template, --server-lang, or --client-lang must be provided in non-interactive mode");
    }

    Ok(TemplateConfig {
        project_name,
        project_path,
        template_type: TemplateType::Empty,
        server_lang: parse_server_lang(&server_lang_str)?,
        client_lang: parse_client_lang(&client_lang_str)?,
        github_repo: None,
        template_def: None,
        use_local: true,
    })
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

    // Check if server-lang or client-lang is provided
    let server_lang_arg = args.get_one::<String>("server-lang");
    let client_lang_arg = args.get_one::<String>("client-lang");

    if server_lang_arg.is_some() || client_lang_arg.is_some() {
        // Use provided languages
        let server_lang = parse_server_lang(&server_lang_arg.cloned())?;
        if let Some(lang_str) = server_lang_arg {
            println!("{} {}", "Server language:".bold(), lang_str);
        }

        let client_lang = parse_client_lang(&client_lang_arg.cloned())?;
        if let Some(lang_str) = client_lang_arg {
            println!("{} {}", "Client language:".bold(), lang_str);
        }

        return Ok(TemplateConfig {
            project_name,
            project_path,
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

    let client_selection = Select::with_theme(&theme)
        .with_prompt("Select client")
        .items(&client_choices)
        .default(0)
        .interact()?;

    let other_index = highlights.len();

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
        loop {
            let template_id: String = Input::<String>::with_theme(&theme)
                .with_prompt("Template ID or GitHub repository (owner/repo). Press 'l' to list available templates")
                .interact_text()?
                .trim()
                .to_string();

            if template_id == "l" || template_id == "L" {
                println!("\n{}", "Available templates:".bold());
                for template in &templates {
                    println!("  {} - {}", template.id, template.description);
                }
                println!();
                continue;
            }

            return create_template_config_from_template_str(
                project_name.clone(),
                project_path.clone(),
                &template_id,
                &templates,
            );
        }
    } else {
        unreachable!("Invalid selection index")
    }
}

fn clone_git_subdirectory(repo_url: &str, subdir: &str, target: &Path, reference: Option<&str>) -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.path();

    let reference_display = reference.map(|b| format!(" (reference: {})", b)).unwrap_or_default();
    println!("  Cloning repository from {}{}...", repo_url, reference_display);

    // Setup callbacks
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(|url, username_from_url, allowed_types| {
        if allowed_types.contains(git2::CredentialType::SSH_KEY) {
            if let Some(username) = username_from_url {
                return Cred::ssh_key_from_agent(username);
            }
        }
        if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            return Cred::userpass_plaintext("git", "");
        }
        if allowed_types.contains(git2::CredentialType::DEFAULT) {
            return Cred::default();
        }
        if url.starts_with("https://") {
            return Cred::userpass_plaintext("git", "");
        }
        Err(git2::Error::from_str("no auth method available"))
    });

    // Normal full clone
    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_opts);

    let repo = builder
        .clone(repo_url, temp_path)
        .context("Failed to clone repository")?;

    // Checkout commit if specified
    if let Some(commit_sha) = reference {
        let oid = git2::Oid::from_str(commit_sha).context("Invalid commit SHA format")?;
        let commit = repo
            .find_commit(oid)
            .context("Commit not found in repository (may not be reachable from default branch)")?;

        repo.checkout_tree(commit.as_object(), None)
            .context("Failed to checkout commit tree")?;
        repo.set_head_detached(oid).context("Failed to detach HEAD")?;
    }

    //  Copy requested subdir
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

fn update_rust_client_name(client_dir: &Path, project_name: &str) -> anyhow::Result<()> {
    let cargo_path = client_dir.join("Cargo.toml");
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
        .replace(&content, format!(r#"name = "{}-client""#, safe_name))
        .to_string();

    fs::write(&cargo_path, content)?;
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
    let branch = env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_REFERENCE")
        .unwrap_or_else(|_| DEFAULT_TEMPLATES_REFERENCE.to_string());

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
    println!("{}", "Initializing project from template...".cyan());

    match config.template_type {
        TemplateType::Builtin => init_builtin(config, project_path)?,
        TemplateType::GitHub => init_github_template(config, project_path)?,
        TemplateType::Empty => init_empty(config, project_path)?,
    }

    // Copy .cursorrules file from the repository
    copy_cursorrules(project_path).await?;

    println!("{}", "Project initialized successfully!".green());
    print_next_steps(config, project_path)?;

    Ok(())
}

fn init_builtin(config: &TemplateConfig, project_path: &Path) -> anyhow::Result<()> {
    let template_def = config
        .template_def
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Template definition missing"))?;

    // Use the same branch as the templates list if specified
    let branch = env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_REFERENCE").ok().or_else(|| {
        if env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_REPO").is_ok() {
            None
        } else {
            Some(DEFAULT_TEMPLATES_REFERENCE.to_string())
        }
    });

    println!(
        "Setting up client ({})...",
        config.client_lang.map(|l| l.as_str()).unwrap_or("none")
    );
    let client_source = &template_def.client_source;
    let (repo, subdir) = parse_repo_source(client_source);
    clone_git_subdirectory(
        &format!("https://github.com/{}", repo),
        subdir,
        project_path,
        branch.as_deref(),
    )?;

    println!(
        "Setting up server ({})...",
        config.server_lang.map(|l| l.as_str()).unwrap_or("none")
    );
    let server_dir = project_path.join("spacetimedb");
    let server_source = &template_def.server_source;
    let (repo, subdir) = parse_repo_source(server_source);
    clone_git_subdirectory(
        &format!("https://github.com/{}", repo),
        subdir,
        &server_dir,
        branch.as_deref(),
    )?;

    match config.client_lang {
        Some(ClientLanguage::TypeScript) => {
            update_client_package_json(project_path, &config.project_name)?;
            update_typescript_client_config(project_path, &config.project_name, config.use_local)?;
            println!(
                "{}",
                "Note: Run 'npm install' in the project directory to install dependencies".yellow()
            );
        }
        Some(ClientLanguage::Rust) => {
            update_rust_client_name(project_path, &config.project_name)?;
        }
        Some(ClientLanguage::Csharp) => {}
        None => {}
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

    match config.client_lang {
        Some(ClientLanguage::TypeScript) => {
            println!("Setting up TypeScript client...");
            let client_dir = project_path.join("client");

            let branch = env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_REFERENCE").ok().or_else(|| {
                if env::var("SPACETIMEDB_CLI_TEMPLATES_LIST_REPO").is_ok() {
                    None
                } else {
                    Some(DEFAULT_TEMPLATES_REFERENCE.to_string())
                }
            });

            clone_git_subdirectory(
                "https://github.com/clockworklabs/SpacetimeDB",
                "crates/bindings-typescript/examples/empty",
                &client_dir,
                branch.as_deref(),
            )?;

            update_client_package_json(&client_dir, &config.project_name)?;

            if config.server_lang.is_some() {
                // Create package.json with boilerplate for working with the server (like
                // `spacetime publish`
                create_root_package_json(project_path, &config.project_name, config.use_local)?;
            }

            println!(
                "{}",
                "Note: Run 'npm install' in the project directory to install dependencies".yellow()
            );
        }
        Some(ClientLanguage::Rust) => {}
        Some(ClientLanguage::Csharp) => {}
        None => {}
    }

    Ok(())
}

fn init_empty_rust_server(server_dir: &Path, _project_name: &str) -> anyhow::Result<()> {
    init_rust_project(server_dir)
}

fn init_empty_csharp_server(server_dir: &Path, _project_name: &str) -> anyhow::Result<()> {
    init_csharp_project(server_dir)
}

fn init_empty_typescript_server(server_dir: &Path, _project_name: &str) -> anyhow::Result<()> {
    init_typescript_project(server_dir)
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
            println!("  spacetime generate --lang csharp --out-dir src/module_bindings --project-path spacetimedb");
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

pub async fn exec(mut config: Config, args: &ArgMatches) -> anyhow::Result<()> {
    println!("{UNSTABLE_WARNING}\n");

    let is_interactive = !args.get_flag("non-interactive");
    let template = args.get_one::<String>("template");
    let server_lang = args.get_one::<String>("server-lang");
    let client_lang = args.get_one::<String>("client-lang");
    let name = args.get_one::<String>("name");

    // Validate that template and language options are not used together
    if template.is_some() && (server_lang.is_some() || client_lang.is_some()) {
        anyhow::bail!(
            "Cannot specify both --template and --server-lang/--client-lang. Language is determined by the template."
        );
    }

    if !is_interactive {
        // In non-interactive mode, validate all required args are present
        if name.is_none() {
            anyhow::bail!("--name is required in non-interactive mode");
        }
        if template.is_none() && server_lang.is_none() && client_lang.is_none() {
            anyhow::bail!(
                "Either --template, --server-lang, or --client-lang must be provided in non-interactive mode"
            );
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
