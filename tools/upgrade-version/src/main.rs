#![allow(clippy::disallowed_macros)]

use clap::{Arg, Command};
use duct::cmd;
use regex::Regex;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

fn process_license_file(upgrade_version: &str) {
    let path = "LICENSE.txt";
    let file = fs::read_to_string(path).unwrap();
    let re = Regex::new(r"(?m)^(Licensed Work:\s+SpacetimeDB )([\d\.]+)$").unwrap();
    let file = re.replace_all(&file, |caps: &regex::Captures| {
        format!("{}{}", &caps[1], upgrade_version)
    });
    fs::write(path, &*file).unwrap();
}

fn edit_toml(path: impl AsRef<Path>, f: impl FnOnce(&mut toml_edit::DocumentMut)) -> anyhow::Result<()> {
    let path = path.as_ref();
    let mut doc = fs::read_to_string(path)?.parse::<toml_edit::DocumentMut>()?;
    f(&mut doc);
    fs::write(path, doc.to_string())?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
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
    if dir_name != "SpacetimeDB" && dir_name != "public" {
        anyhow::bail!("You must execute this binary from inside of the SpacetimeDB directory, or use --spacetime-path");
    }

    // root Cargo.toml
    edit_toml("Cargo.toml", |doc| {
        doc["workspace"]["package"]["version"] = toml_edit::value(version);
        for (key, dep) in doc["workspace"]["dependencies"]
            .as_table_like_mut()
            .expect("workspace.dependencies is not a table")
            .iter_mut()
        {
            if key.get().starts_with("spacetime") {
                dep["version"] = toml_edit::value(version)
            }
        }
    })?;

    edit_toml("crates/cli/src/subcommands/project/rust/Cargo._toml", |doc| {
        doc["dependencies"]["spacetimedb"] = toml_edit::value(version);
    })?;

    process_license_file(version);
    cmd!("cargo", "check").run().expect("Cargo check failed!");

    Ok(())
}
