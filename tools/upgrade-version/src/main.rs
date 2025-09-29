#![allow(clippy::disallowed_macros)]

use chrono::{Datelike, Local};
use clap::{Arg, Command};
use duct::cmd;
use regex::Regex;
use semver::Version;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

fn process_license_file(path: &str, version: &str) {
    let file = fs::read_to_string(path).unwrap();

    let version_re = Regex::new(r"(?m)^(Licensed Work:\s+SpacetimeDB )([\d\.]+)\r?$").unwrap();
    let file = version_re.replace_all(&file, |caps: &regex::Captures| format!("{}{}", &caps[1], version));

    let date_re = Regex::new(r"(?m)^Change Date:\s+\d{4}-\d{2}-\d{2}\r?$").unwrap();
    let new_date = Local::now()
        .with_year(Local::now().year() + 5)
        .unwrap()
        .format("Change Date:          %Y-%m-%d")
        .to_string();

    let file = date_re.replace_all(&file, new_date.as_str());

    fs::write(path, &*file).unwrap();
}

fn edit_toml(path: impl AsRef<Path>, f: impl FnOnce(&mut toml_edit::DocumentMut)) -> anyhow::Result<()> {
    let path = path.as_ref();
    let mut doc = fs::read_to_string(path)?.parse::<toml_edit::DocumentMut>()?;
    f(&mut doc);
    fs::write(path, doc.to_string())?;
    Ok(())
}

// Update only the first occurrence of the top-level "version" field in a JSON file,
// preserving original formatting and key order.
fn rewrite_json_version_inplace(path: impl AsRef<Path>, new_version: &str) -> anyhow::Result<()> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)?;
    let re = Regex::new(r#"(?m)^(\s*\"version\"\s*:\s*\")(.*?)(\")"#).unwrap();
    let mut replaced = false;
    let updated = re.replacen(&contents, 1, |caps: &regex::Captures| {
        replaced = true;
        format!("{}{}{}", &caps[1], new_version, &caps[3])
    });
    if !replaced {
        anyhow::bail!(
            "Could not find top-level \"version\" field to update in {}",
            path.display()
        );
    }
    fs::write(path, updated.as_ref())?;
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
                .long("spacetime-path")
                .help("The path to SpacetimeDB. If not provided, uses the current directory."),
        )
        .arg(
            Arg::new("typescript")
                .long("typescript")
                .value_parser(clap::value_parser!(bool))
                .help("Also bump the version of the TypeScript SDK (crates/bindings-typescript/package.json)"),
        )
        .arg(
            Arg::new("rust-and-cli")
                .long("rust-and-cli")
                .value_parser(clap::value_parser!(bool))
                .default_value("true")
                .help("Whether to update Rust workspace TOMLs, CLI template, and license files (default: true)"),
        )
        .get_matches();

    let version = matches.get_one::<String>("upgrade_version").unwrap();
    if let Some(path) = matches.get_one::<PathBuf>("spacetime-path") {
        env::set_current_dir(path).ok();
    }

    let current_dir = env::current_dir().expect("No current directory!");
    let dir_name = current_dir.file_name().expect("No current directory!");
    if dir_name != "SpacetimeDB" && dir_name != "public" {
        anyhow::bail!("You must execute this binary from inside of the SpacetimeDB directory, or use --spacetime-path");
    }

    if matches.get_one::<bool>("rust-and-cli").unwrap() {
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
            // Only set major.minor for the spacetimedb dependency, drop the patch component.
            // See https://github.com/clockworklabs/SpacetimeDB/issues/2724.
            let v = Version::parse(version).expect("Invalid semver provided to upgrade-version");
            let major_minor = format!("{}.{}", v.major, v.minor);
            doc["dependencies"]["spacetimedb"] = toml_edit::value(major_minor);
        })?;

        process_license_file("LICENSE.txt", version);
        process_license_file("licenses/BSL.txt", version);
        cmd!("cargo", "check").run().expect("Cargo check failed!");
    }

    if matches.get_one::<bool>("typescript").unwrap() {
        // Update the TypeScript SDK version field without reformatting the file.
        // If the repository layout changes, update this path accordingly.
        rewrite_json_version_inplace("crates/bindings-typescript/package.json", version)?;
    }

    Ok(())
}
