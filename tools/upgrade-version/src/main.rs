#![allow(clippy::disallowed_macros)]

extern crate clap;
extern crate walkdir;

use clap::{Arg, Command};
use duct::cmd;
use regex::Regex;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use walkdir::WalkDir;

static IGNORE_FILES: [&str; 5] = [
    "crates/sdk/tests/connect_disconnect_client/Cargo.toml",
    "crates/sdk/tests/test-client/Cargo.toml",
    "crates/sdk/tests/test-counter/Cargo.toml",
    "crates/sqltest/Cargo.toml",
    "crates/testing/Cargo.toml",
];

fn find_files(start_dir: &str, name: &str) -> Vec<String> {
    let mut files = Vec::new();
    for entry in WalkDir::new(start_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.path().file_name() == Some(OsStr::new(name)) {
            if IGNORE_FILES.contains(&entry.path().to_string_lossy().as_ref()) {
                continue;
            }
            files.push(entry.path().to_string_lossy().to_string());
        }
    }
    files
}

enum FileProcessState {
    Package,
    Dependencies,
}

fn process_crate_toml(path: &PathBuf, upgrade_version: &str, upgrade_package_version: bool) {
    println!("Processing file: {}", path.to_string_lossy());

    let file = File::open(path).unwrap_or_else(|_| panic!("File not found: {}", path.to_string_lossy()));
    let reader = BufReader::new(file);
    let mut temp_file = NamedTempFile::new().expect("Failed to create temporary file!");
    let mut state = FileProcessState::Package;
    let re = Regex::new(r#"(version = ")([^"]+)"#).unwrap();

    for line_result in reader.lines() {
        match line_result {
            Ok(line) => {
                let new_line = match state {
                    FileProcessState::Package => {
                        if line.contains("version = ") && upgrade_package_version {
                            re.replace(&line, format!("version = \"{}", upgrade_version).as_str())
                                .into()
                        } else if line.contains("[dependencies]") {
                            state = FileProcessState::Dependencies;
                            line
                        } else {
                            line
                        }
                    }
                    FileProcessState::Dependencies => {
                        if line.starts_with("spacetimedb") {
                            if !line.contains('{') {
                                format!("spacetimedb = \"{}\"", upgrade_version)
                            } else {
                                // Match the version number and capture it
                                re.replace(&line, format!("version = \"{}", upgrade_version).as_str())
                                    .into()
                            }
                        } else {
                            line
                        }
                    }
                };

                writeln!(temp_file, "{}", new_line).unwrap();
            }
            Err(e) => eprintln!("Error reading line: {}", e),
        }
    }

    // Rename the temporary file to replace the original file
    fs::rename(temp_file.path(), path).expect("Failed to overwrite source file.");
}

fn process_license_file(upgrade_version: &str) {
    let path = "LICENSE.txt";
    let file = File::open(path).unwrap_or_else(|_| panic!("File not found: {}", path));
    let reader = BufReader::new(file);
    let mut temp_file = NamedTempFile::new().expect("Failed to create temporary file!");
    let re = Regex::new(r"(^Licensed Work:\s+SpacetimeDB )([\d\.]+)").unwrap();

    for line_result in reader.lines() {
        match line_result {
            Ok(line) => {
                let new_line = if line.starts_with("Licensed Work") {
                    re.replace(
                        &line,
                        format!("{}{}", &re.captures(&line).unwrap()[1], upgrade_version).as_str(),
                    )
                    .into()
                } else {
                    line
                };
                writeln!(temp_file, "{}", new_line).unwrap();
            }
            Err(e) => eprintln!("Error reading line: {}", e),
        }
    }

    // Rename the temporary file to replace the original file
    fs::rename(temp_file.path(), path).expect("Failed to overwrite source file.");
}

fn main() {
    let matches = Command::new("upgrade-version")
        .version("1.0")
        .about("Upgrades the version of the SpacetimeDB repository")
        .arg(
            Arg::new("upgrade_version")
                .required(true)
                .help("The version to upgrade to"),
        )
        .arg(
            Arg::new("spacetime-path")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value("../..")
                .long("spacetime-path")
                .help("The path to SpacetimeDB"),
        )
        .get_matches();

    let version = matches.get_one::<String>("upgrade_version").unwrap();
    env::set_current_dir(matches.get_one::<PathBuf>("spacetime-path").unwrap()).ok();

    let current_dir = env::current_dir().expect("No current directory!");
    let dir_name = current_dir.file_name().expect("No current directory!");
    if dir_name != "SpacetimeDB" {
        println!("You must execute this binary from inside of the SpacetimeDB directory, or use --spacetime-path");
        return;
    }

    for file in find_files("crates", "Cargo.toml") {
        process_crate_toml(&PathBuf::from(file), version, true);
    }
    for file in find_files("modules", "Cargo.toml") {
        process_crate_toml(&PathBuf::from(file), version, false);
    }
    for file in find_files("crates", "Cargo._toml") {
        process_crate_toml(&PathBuf::from(file), version, false);
    }

    process_crate_toml(&PathBuf::from("crates/testing/Cargo.toml"), version, false);
    process_license_file(version);
    cmd!("cargo", "check").run().expect("Cargo check failed!");
}
