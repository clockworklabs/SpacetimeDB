use crate::util::ModuleLanguage;
use crate::Config;
use crate::{detect::find_executable, util::UNSTABLE_WARNING};
use anyhow::Context;
use clap::{Arg, ArgMatches};
use colored::Colorize;
use std::path::{Path, PathBuf};

pub fn cli() -> clap::Command {
    clap::Command::new("init")
        .about(format!("Initializes a new spacetime project. {}", UNSTABLE_WARNING))
        .arg(
            Arg::new("project-path")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(".")
                .help("The path where we will create the spacetime project"),
        )
        .arg(
            Arg::new("lang")
                .required(true)
                .short('l')
                .long("lang")
                .help("The spacetime module language.")
                .value_parser(clap::value_parser!(ModuleLanguage)),
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
            println!("{}", format!("This OS may be unsupported: {}", unsupported_os).yellow());
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

fn check_for_go() -> bool {
    match std::env::consts::OS {
        "linux" | "freebsd" | "netbsd" | "openbsd" | "solaris" | "macos" => {
            if find_executable("go").is_some() {
                return true;
            }
            println!("{}", "Warning: You have created a Go project, but you are missing the Go toolchain. You should install Go from:\n\n\thttps://golang.org/dl/\n".yellow());
        }
        "windows" => {
            if find_executable("go.exe").is_some() {
                return true;
            }
            println!("{}", "Warning: You have created a Go project, but you are missing the Go toolchain. Visit https://golang.org/dl/ for installation instructions.\n".yellow());
        }
        unsupported_os => {
            println!("{}", format!("This OS may be unsupported: {}", unsupported_os).yellow());
        }
    }
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
            println!("{}", format!("This OS may be unsupported: {}", unsupported_os).yellow());
        }
    }
    false
}

pub async fn exec(_config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    eprintln!("{}\n", UNSTABLE_WARNING);

    let project_path = args.get_one::<PathBuf>("project-path").unwrap();
    let project_lang = *args.get_one::<ModuleLanguage>("lang").unwrap();

    // Create the project path, or make sure the target project path is empty.
    if project_path.exists() {
        if !project_path.is_dir() {
            return Err(anyhow::anyhow!(
                "Path {} exists but is not a directory. A new SpacetimeDB project must be initialized in an empty directory.",
                project_path.display()
            ));
        }

        if std::fs::read_dir(project_path).unwrap().count() > 0 {
            return Err(anyhow::anyhow!(
                "Cannot create new SpacetimeDB project in non-empty directory: {}",
                project_path.display()
            ));
        }
    } else {
        create_directory(project_path)?;
    }

    match project_lang {
        ModuleLanguage::Rust => exec_init_rust(args).await,
        ModuleLanguage::Csharp => exec_init_csharp(args).await,
        ModuleLanguage::Go => exec_init_go(args).await,
    }
}

pub async fn exec_init_rust(args: &ArgMatches) -> Result<(), anyhow::Error> {
    let project_path = args.get_one::<PathBuf>("project-path").unwrap();

    let export_files = vec![
        (include_str!("project/rust/Cargo._toml"), "Cargo.toml"),
        (include_str!("project/rust/lib._rs"), "src/lib.rs"),
        (include_str!("project/rust/_gitignore"), ".gitignore"),
        (include_str!("project/rust/config._toml"), ".cargo/config.toml"),
    ];

    for data_file in export_files {
        let path = project_path.join(data_file.1);

        create_directory(path.parent().unwrap())?;

        std::fs::write(path, data_file.0)?;
    }

    // Check all dependencies
    check_for_cargo();
    check_for_git();

    println!(
        "{}",
        format!("Project successfully created at path: {}", project_path.display()).green()
    );

    Ok(())
}

pub async fn exec_init_csharp(args: &ArgMatches) -> anyhow::Result<()> {
    let project_path = args.get_one::<PathBuf>("project-path").unwrap();

    let export_files = vec![
        (include_str!("project/csharp/StdbModule._csproj"), "StdbModule.csproj"),
        (include_str!("project/csharp/Lib._cs"), "Lib.cs"),
        (include_str!("project/csharp/_gitignore"), ".gitignore"),
        (include_str!("project/csharp/global._json"), "global.json"),
    ];

    // Check all dependencies
    check_for_dotnet();
    check_for_git();

    for data_file in export_files {
        let path = project_path.join(data_file.1);

        create_directory(path.parent().unwrap())?;

        std::fs::write(path, data_file.0)?;
    }

    println!(
        "{}",
        format!("Project successfully created at path: {}", project_path.display()).green()
    );

    Ok(())
}

pub async fn exec_init_go(args: &ArgMatches) -> anyhow::Result<()> {
    let project_path = args.get_one::<PathBuf>("project-path").unwrap();

    let export_files = vec![
        (include_str!("project/go/go._mod"), "go.mod"),
        (include_str!("project/go/main._go"), "main.go"),
        (include_str!("project/go/_gitignore"), ".gitignore"),
        (include_str!("project/go/Makefile"), "Makefile"),
    ];

    // Check all dependencies
    check_for_go();
    check_for_git();

    for data_file in export_files {
        let path = project_path.join(data_file.1);

        create_directory(path.parent().unwrap())?;

        std::fs::write(path, data_file.0)?;
    }

    println!(
        "{}",
        format!("Project successfully created at path: {}", project_path.display()).green()
    );

    Ok(())
}

fn create_directory(path: &Path) -> Result<(), anyhow::Error> {
    std::fs::create_dir_all(path).context("Failed to create directory")
}
