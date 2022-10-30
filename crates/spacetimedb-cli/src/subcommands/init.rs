use crate::Config;
use clap::{Arg, ArgMatches};
use colored::{ColoredString, Colorize};
use std::path::Path;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("init")
        .about("Initializes a new spacetime project")
        .arg(
            Arg::new("project-path")
                .required(false)
                .default_value(".")
                .help("The path where we will create the spacetime project"),
        )
        .arg(
            Arg::new("lang")
                .required(true)
                .short('l')
                .long("lang")
                .takes_value(true)
                .help("The spacetime module language.")
                .possible_values(["rust"]),
        )
}

pub async fn exec(_: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let project_path_str = args.value_of("project-path").unwrap();
    let project_path = Path::new(project_path_str);
    let project_lang = args.value_of("lang").unwrap();

    // Create the project path, or make sure the target project path is empty.
    if project_path.exists() {
        if !project_path.is_dir() {
            return Err(anyhow::anyhow!(
                "Fatal Error: path {} exists but is not a directory.",
                project_path_str
            ));
        }

        if std::fs::read_dir(project_path_str).unwrap().count() > 0 {
            return Err(anyhow::anyhow!(
                "Fatal Error: Path {} is a directory, but is not empty",
                project_path_str
            ));
        }
    } else {
        if let Err(e) = create_directory(project_path_str) {
            return Err(e);
        }
    }

    let mut export_files = Vec::<(&str, &str)>::new();

    match project_lang {
        "rust" => {
            export_files.push((include_str!("project/Cargo._toml"), "Cargo.toml"));
            export_files.push((
                include_str!("../../../spacetimedb-core/protobuf/client_api.proto"),
                ".spacetime/client_api.proto",
            ));
            export_files.push((include_str!("project/lib._rs"), "src/lib.rs"));
            export_files.push((include_str!("project/rust_gitignore"), ".gitignore"));
        }
        _ => {
            panic!("Unsupported language!");
        }
    }

    for data_file in export_files {
        let value = project_path.join(data_file.1);
        let path_str = match value.to_str() {
            Some(s) => s,
            None => {
                // The developer created an invalid path
                panic!("Invalid path supplied: {}", data_file.1);
            }
        };

        let path = Path::new(path_str);
        if let Some(parent_path) = path.parent() {
            if let Some(parent_path) = parent_path.to_str() {
                if let Err(e) = create_directory(parent_path) {
                    return Err(e);
                }
            } else {
                return Err(anyhow::anyhow!("Failed to parse path: {}", path_str));
            }
        } else {
            return Err(anyhow::anyhow!("Failed to parse path: {}", path_str));
        }

        if let Err(e) = std::fs::write(path_str, data_file.0) {
            return Err(anyhow::anyhow!("{}", e));
        }
    }

    // Some courtesy checks for the user
    let mut has_protoc = false;
    let mut install_instructions: Option<ColoredString> = None;
    match std::env::consts::OS {
        "linux" | "freebsd" | "netbsd" | "openbsd" | "solaris" => {
            has_protoc = match find_executable("protoc") {
                None => false,
                Some(_) => true,
            };

            install_instructions = Some("You should install protoc from your package manager. Alternatively, follow the install instructions here:\n\n\thttp://google.github.io/proto-lens/installing-protoc.html".yellow());
        }
        "macos" => {
            has_protoc = match find_executable("protoc") {
                None => false,
                Some(_) => true,
            };
            install_instructions = Some("You can install protoc on macos from brew:\n\n\tbrew install protobuf\n\nAlternatively, follow the instructions here: http://google.github.io/proto-lens/installing-protoc.html".yellow());
        }
        "windows" => {
            has_protoc = match find_executable("protoc.exe") {
                None => false,
                Some(_) => true,
            };

            install_instructions = Some("To install protoc on Windows, follow the instructions here:\n\n\thttp://google.github.io/proto-lens/installing-protoc.html ".yellow());
        }
        unsupported_os => {
            println!("{}", format!("This OS may be unsupported: {}", unsupported_os).yellow());
        }
    }

    if !has_protoc {
        println!(
            "{}",
            format!("Warning: protoc not found in your PATH. If protoc is installed make sure it is").yellow()
        );
        println!("{}", format!("it is available in your PATH. \n").yellow());
        if let Some(colored_string) = install_instructions {
            println!("{}\n", colored_string);
        }
    }

    println!("Project successfully created at path: {}", project_path_str);

    Ok(())
}

fn create_directory(path: &str) -> Result<(), anyhow::Error> {
    if let Err(e) = std::fs::create_dir_all(path) {
        return Err(anyhow::anyhow!(
            "Fatal Error: Failed to create directory: {}",
            e.to_string()
        ));
    }
    Ok(())
}

fn find_executable<P>(exe_name: P) -> Option<std::path::PathBuf>
where
    P: AsRef<Path>,
{
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .filter_map(|dir| {
                let full_path = dir.join(&exe_name);
                if full_path.is_file() {
                    Some(full_path)
                } else {
                    None
                }
            })
            .next()
    })
}
