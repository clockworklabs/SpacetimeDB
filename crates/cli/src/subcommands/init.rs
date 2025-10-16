mod template;

use crate::util::ModuleLanguage;
use crate::Config;
use crate::{detect::find_executable, util::UNSTABLE_WARNING};
use anyhow::Context;
use clap::{Arg, ArgMatches};
use colored::Colorize;
use std::path::{Path, PathBuf};

use template as init_template;

pub fn cli() -> clap::Command {
    clap::Command::new("init")
        .about(format!("Initializes a new spacetime project. {UNSTABLE_WARNING}"))
        .arg(
            Arg::new("project-path")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(".")
                .help("The path where we will create the spacetime project"),
        )
        .arg(
            Arg::new("name")
                .short('n')
                .long("name")
                .value_name("NAME")
                .help("Project name (defaults to directory name if not provided)"),
        )
        .arg(
            Arg::new("lang")
                .short('l')
                .long("lang")
                .help("The spacetime module language.")
                .value_parser(clap::value_parser!(ModuleLanguage)),
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

    let project_path = args.get_one::<PathBuf>("project-path");
    let template = args.get_one::<String>("template");
    let non_interactive = args.get_flag("non-interactive");
    let lang = args.get_one::<ModuleLanguage>("lang");
    let server_lang = args.get_one::<String>("server-lang");
    let client_lang = args.get_one::<String>("client-lang");

    // Determine if we should run in non-interactive mode
    let is_non_interactive =
        non_interactive || template.is_some() || lang.is_some() || server_lang.is_some() || client_lang.is_some();

    if is_non_interactive {
        return init_template::exec_non_interactive_init(&mut config, args).await;
    }

    // Interactive mode
    let path = project_path.cloned().unwrap_or_else(|| PathBuf::from("."));
    init_template::exec_interactive_init(&mut config, &path).await
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
