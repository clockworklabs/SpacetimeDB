use anyhow::{Context, Result};
use clap::ArgMatches;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use git2::Repository;
use regex::Regex;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::subcommands::login::{spacetimedb_login_force, DEFAULT_AUTH_HOST};
use crate::Config;

const DEFAULT_TEMPLATES_REPO: &str = "clockworklabs/SpacetimeDB";
const DEFAULT_TEMPLATES_BRANCH: &str = "master";
const TEMPLATES_FILE_PATH: &str = "crates/cli/.init-templates.json";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemplateDefinition {
    pub name: String,
    pub server_lang: String,
    pub client_lang: String,
    pub server_source: String,
    pub client_source: String,
}

#[derive(Debug, Deserialize)]
struct TemplatesList {
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
}

pub struct TemplateConfig {
    pub project_name: String,
    pub template_type: TemplateType,
    pub server_lang: ServerLanguage,
    pub client_lang: ClientLanguage,
    pub github_repo: Option<String>,
    pub template_def: Option<TemplateDefinition>,
    pub use_local: bool,
}

pub async fn fetch_templates_list() -> Result<Vec<TemplateDefinition>> {
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

    Ok(templates_list.templates)
}

pub async fn check_and_prompt_login(config: &mut Config) -> Result<bool> {
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

pub async fn exec_interactive_init(config: &mut Config, project_path: &Path) -> Result<()> {
    let use_local = !check_and_prompt_login(config).await?;

    let mut template_config = interactive_init().await?;
    template_config.use_local = use_local;

    let actual_project_path = if project_path == Path::new(".") {
        PathBuf::from(&template_config.project_name)
    } else {
        project_path.to_path_buf()
    };

    ensure_empty_directory(&template_config.project_name, &actual_project_path)?;

    init_from_template(&template_config, &actual_project_path)?;

    Ok(())
}

pub async fn exec_template_init(
    config: &mut Config,
    args: &ArgMatches,
    project_path: &Path,
    template_str: &str,
) -> Result<()> {
    let use_local = if args.get_flag("local") {
        true
    } else {
        !check_and_prompt_login(config).await?
    };

    let (project_name, actual_project_path) = if project_path == Path::new(".") {
        let name = "my-spacetime-app".to_string();
        (name.clone(), PathBuf::from(name))
    } else {
        let name = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("my-spacetime-app")
            .to_string();
        (name, project_path.to_path_buf())
    };

    let template_config = TemplateConfig {
        project_name: project_name.clone(),
        template_type: TemplateType::GitHub,
        server_lang: ServerLanguage::Rust,
        client_lang: ClientLanguage::None,
        github_repo: Some(template_str.to_string()),
        template_def: None,
        use_local,
    };

    ensure_empty_directory(&project_name, &actual_project_path)?;

    init_from_template(&template_config, &actual_project_path)?;

    Ok(())
}

pub fn ensure_empty_directory(project_name: &str, project_path: &Path) -> Result<()> {
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

pub async fn interactive_init() -> Result<TemplateConfig> {
    let theme = ColorfulTheme::default();

    let project_name: String = Input::with_theme(&theme)
        .with_prompt("Project name")
        .default("my-spacetime-app".to_string())
        .validate_with(|input: &String| -> Result<(), String> {
            if input.trim().is_empty() {
                return Err("Project name cannot be empty".to_string());
            }

            let path = Path::new(input);
            if path.exists() {
                if !path.is_dir() {
                    return Err(format!(
                        "A file named '{}' already exists. Please choose a different name.",
                        input
                    ));
                }
                match std::fs::read_dir(path) {
                    Ok(entries) => {
                        if entries.count() > 0 {
                            return Err(format!(
                                "Directory '{}' already exists and is not empty. Please choose a different name.",
                                input
                            ));
                        }
                    }
                    Err(_) => {
                        return Err(format!(
                            "Cannot access directory '{}'. Please choose a different name.",
                            input
                        ));
                    }
                }
            }
            Ok(())
        })
        .interact_text()?;

    let templates = fetch_templates_list().await?;

    let mut template_choices: Vec<String> = templates.iter().map(|t| t.name.clone()).collect();
    template_choices.push("Empty project".to_string());
    template_choices.push("Custom GitHub repository".to_string());

    let template_selection = Select::with_theme(&theme)
        .with_prompt("Select template")
        .items(&template_choices)
        .default(0)
        .interact()?;

    let empty_index = templates.len();
    let custom_index = templates.len() + 1;

    let (template_type, server_lang, client_lang, github_repo, template_def) = if template_selection < templates.len() {
        let template = &templates[template_selection];
        let server_lang = match template.server_lang.as_str() {
            "rust" => ServerLanguage::Rust,
            "csharp" => ServerLanguage::Csharp,
            "typescript" => ServerLanguage::TypeScript,
            _ => ServerLanguage::Rust,
        };
        let client_lang = match template.client_lang.as_str() {
            "rust" => ClientLanguage::Rust,
            "csharp" => ClientLanguage::Csharp,
            "typescript" => ClientLanguage::TypeScript,
            "none" => ClientLanguage::None,
            _ => ClientLanguage::TypeScript,
        };
        (
            TemplateType::Builtin,
            server_lang,
            client_lang,
            None,
            Some(template.clone()),
        )
    } else if template_selection == empty_index {
        let server_lang_choices = vec!["Rust", "C#", "TypeScript", "None"];
        let server_lang_selection = Select::with_theme(&theme)
            .with_prompt("Select server language")
            .items(&server_lang_choices)
            .default(0)
            .interact()?;

        let server_lang = match server_lang_selection {
            0 => ServerLanguage::Rust,
            1 => ServerLanguage::Csharp,
            2 => ServerLanguage::TypeScript,
            _ => ServerLanguage::None,
        };

        let client_lang_choices = vec!["Rust", "C#", "TypeScript", "None"];
        let client_lang_selection = Select::with_theme(&theme)
            .with_prompt("Select client language")
            .items(&client_lang_choices)
            .default(0)
            .interact()?;

        let client_lang = match client_lang_selection {
            0 => ClientLanguage::Rust,
            1 => ClientLanguage::Csharp,
            2 => ClientLanguage::TypeScript,
            _ => ClientLanguage::None,
        };

        if server_lang == ServerLanguage::None && client_lang == ClientLanguage::None {
            anyhow::bail!("At least one of server or client language must be selected");
        }

        (TemplateType::Empty, server_lang, client_lang, None, None)
    } else {
        let repo: String = Input::with_theme(&theme)
            .with_prompt("GitHub repository (owner/repo or full URL)")
            .interact_text()?;
        (
            TemplateType::GitHub,
            ServerLanguage::Rust,
            ClientLanguage::None,
            Some(repo),
            None,
        )
    };

    Ok(TemplateConfig {
        project_name,
        template_type,
        server_lang,
        client_lang,
        github_repo,
        template_def,
        use_local: true,
    })
}

fn clone_git_subdirectory(repo_url: &str, subdir: &str, target: &Path) -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.path();

    eprintln!("  Cloning repository from {}...", repo_url);

    let mut builder = git2::build::RepoBuilder::new();

    let mut fetch_options = git2::FetchOptions::new();
    let mut callbacks = git2::RemoteCallbacks::new();

    callbacks.credentials(|url, username_from_url, allowed_types| {
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

fn clone_github_template(repo_input: &str, target: &Path) -> Result<()> {
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

    callbacks.credentials(|url, username_from_url, allowed_types| {
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

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
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

fn configure_rust_server(server_dir: &Path, project_name: &str) -> Result<()> {
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

fn create_root_package_json(root: &Path, project_name: &str, use_local: bool) -> Result<()> {
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

fn update_client_package_json(client_dir: &Path, project_name: &str) -> Result<()> {
    let package_path = client_dir.join("package.json");
    if !package_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&package_path)?;
    let mut package: serde_json::Value = serde_json::from_str(&content)?;

    package["name"] = json!(format!("{}-client", project_name));

    let updated_content = serde_json::to_string_pretty(&package)?;
    fs::write(package_path, updated_content)?;

    Ok(())
}

fn update_client_config(client_dir: &Path, module_name: &str, use_local: bool) -> Result<()> {
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

pub fn init_from_template(config: &TemplateConfig, project_path: &Path) -> Result<()> {
    eprintln!("{}", "Initializing project from template...".cyan());

    match config.template_type {
        TemplateType::Builtin => init_builtin(config, project_path)?,
        TemplateType::GitHub => init_github_template(config, project_path)?,
        TemplateType::Empty => init_empty(config, project_path)?,
    }

    eprintln!("{}", "Project initialized successfully!".green());
    print_next_steps(config, project_path)?;

    Ok(())
}

fn init_builtin(config: &TemplateConfig, project_path: &Path) -> Result<()> {
    let template_def = config
        .template_def
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Template definition missing"))?;

    eprintln!("Setting up client ({})...", config.client_lang.as_str());
    let client_source = &template_def.client_source;
    let (repo, subdir) = parse_repo_source(client_source);
    clone_git_subdirectory(&format!("https://github.com/{}", repo), subdir, project_path)?;

    eprintln!("Setting up server ({})...", config.server_lang.as_str());
    let server_dir = project_path.join("spacetimedb");
    let server_source = &template_def.server_source;
    let (repo, subdir) = parse_repo_source(server_source);
    clone_git_subdirectory(&format!("https://github.com/{}", repo), subdir, &server_dir)?;

    // TODO: figure out adjustments we may need to do for other client and server langs
    if config.server_lang == ServerLanguage::Rust {
        configure_rust_server(&server_dir, &config.project_name)?;
    }

    if config.client_lang == ClientLanguage::TypeScript {
        update_client_package_json(project_path, &config.project_name)?;
        update_client_config(project_path, &config.project_name, config.use_local)?;
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

fn init_github_template(config: &TemplateConfig, project_path: &Path) -> Result<()> {
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

fn init_empty(config: &TemplateConfig, project_path: &Path) -> Result<()> {
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

            clone_git_subdirectory(
                "https://github.com/clockworklabs/SpacetimeDB",
                "crates/bindings-typescript/examples/empty",
                &client_dir,
            )?;

            update_client_package_json(&client_dir, &config.project_name)?;

            if config.server_lang != ServerLanguage::None {
                // Create package.json with boilerplate for working with the server (like
                // `spacetime publish`
                // TODO: I don't like the name of this function, also it overrides whatever is in
                // the empty repo
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

fn init_empty_rust_server(server_dir: &Path, _project_name: &str) -> Result<()> {
    crate::subcommands::init::init_rust_project(server_dir)
}

fn init_empty_csharp_server(server_dir: &Path, _project_name: &str) -> Result<()> {
    crate::subcommands::init::init_csharp_project(server_dir)
}

fn init_empty_typescript_server(_server_dir: &Path, _project_name: &str) -> Result<()> {
    todo!()
}

fn print_next_steps(config: &TemplateConfig, project_path: &Path) -> Result<()> {
    eprintln!();
    eprintln!("{}", "Next steps:".bold());

    let rel_path = project_path
        .strip_prefix(std::env::current_dir()?)
        .unwrap_or(project_path);

    if rel_path != Path::new(".") && rel_path != Path::new("") {
        eprintln!("  cd {}", rel_path.display());
    }

    match (config.template_type, config.server_lang, config.client_lang) {
        (TemplateType::Builtin, _, ClientLanguage::TypeScript) => {
            eprintln!("  npm install");
            eprintln!("  npm run {}", if config.use_local { "local" } else { "deploy" });
            eprintln!("  npm run dev");
        }
        (TemplateType::Builtin, _, ClientLanguage::None) => {
            eprintln!("  cd spacetimedb");
            eprintln!("  spacetime build");
            eprintln!("  spacetime publish {}", config.project_name);
        }
        (TemplateType::GitHub, _, _) => {
            eprintln!("  # Follow the template's README for setup instructions");
        }
        (TemplateType::Empty, ServerLanguage::None, ClientLanguage::TypeScript) => {
            eprintln!("  npm install");
            eprintln!("  npm run dev");
        }
        (TemplateType::Empty, _, ClientLanguage::TypeScript) => {
            eprintln!("  npm install");
            eprintln!("  cd spacetimedb");
            eprintln!("  spacetime build");
            eprintln!("  spacetime publish {}", config.project_name);
            eprintln!("  cd ..");
            eprintln!("  npm run dev");
        }
        (TemplateType::Empty, _, ClientLanguage::None) => {
            eprintln!("  cd spacetimedb");
            eprintln!("  spacetime build");
            eprintln!("  spacetime publish {}", config.project_name);
        }
        (TemplateType::Builtin, _, ClientLanguage::Rust | ClientLanguage::Csharp) => {
            eprintln!("  # Follow the template's README for setup instructions");
        }
        (TemplateType::Empty, _, ClientLanguage::Rust | ClientLanguage::Csharp) => {
            eprintln!("  # Client setup not yet implemented");
            eprintln!("  cd spacetimedb");
            eprintln!("  spacetime build");
            eprintln!("  spacetime publish {}", config.project_name);
        }
    }

    eprintln!();
    eprintln!("Learn more: {}", "https://spacetimedb.com/docs".cyan());

    Ok(())
}
