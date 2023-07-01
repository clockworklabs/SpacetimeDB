use crate::Config;
use anyhow::Context;
use clap::{Arg, ArgMatches};
use colored::Colorize;
use std::path::{Path, PathBuf};

pub fn cli() -> clap::Command {
    clap::Command::new("init")
        .about("Initializes a new spacetime project")
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
                .value_parser(clap::value_parser!(ProjectLang)),
        )
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum ProjectLang {
    Rust,
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
            println!("{}", "Warning: You have created a rust project, but you are missing cargo. Visit the rust-lang official website for the latest instructions on install cargo on Windows:\n\n\tYou have created a rust project, but you are missing cargo.\n".yellow());
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

pub async fn exec(_: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let project_path = args.get_one::<PathBuf>("project-path").unwrap();
    let project_lang = *args.get_one::<ProjectLang>("lang").unwrap();

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
        ProjectLang::Rust => exec_init_rust(args).await,
    }
}

pub async fn exec_init_rust(args: &ArgMatches) -> Result<(), anyhow::Error> {
    let project_path = args.get_one::<PathBuf>("project-path").unwrap();

    let export_files = vec![
        (include_str!("project/Cargo._toml"), "Cargo.toml"),
        (include_str!("project/lib._rs"), "src/lib.rs"),
        (include_str!("project/rust_gitignore"), ".gitignore"),
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

fn create_directory(path: &Path) -> Result<(), anyhow::Error> {
    std::fs::create_dir_all(path).context("Failed to create directory")
}

fn find_executable(exe_name: impl AsRef<Path>) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .filter_map(|dir| {
                let full_path = dir.join(&exe_name);
                full_path.is_file().then_some(full_path)
            })
            .next()
    })
}
