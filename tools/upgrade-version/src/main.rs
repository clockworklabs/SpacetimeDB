#![allow(clippy::disallowed_macros)]

use anyhow::Context;
use chrono::{Datelike, Local};
use clap::{Arg, ArgGroup, Command};
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

pub fn rewrite_package_json_dependency_version_inplace(
    path: impl AsRef<Path>,
    new_version: &str,
) -> anyhow::Result<()> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

    // This regex matches:
    // "spacetimedb": "1.5.*" → capturing leading whitespace and quotes, then replacing only the version.
    let re = Regex::new(r#"(?m)^(\s*"spacetimedb"\s*:\s*")([^"]*)(")"#).expect("Invalid regex");

    let mut replaced = false;
    let updated = re.replacen(&contents, 1, |caps: &regex::Captures| {
        replaced = true;
        format!("{}{}{}", &caps[1], new_version, &caps[3])
    });

    if !replaced {
        anyhow::bail!(
            "Could not find \"spacetimedb\" dependency to update in {}",
            path.display()
        );
    }

    fs::write(path, updated.as_ref()).with_context(|| format!("Failed to write updated file to {}", path.display()))?;

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
                .action(clap::ArgAction::SetTrue)
                .help("Also bump the version of the TypeScript SDK (crates/bindings-typescript/package.json)"),
        )
        .arg(
            Arg::new("rust-and-cli")
                .long("rust-and-cli")
                .action(clap::ArgAction::SetTrue)
                .help("Whether to update Rust workspace TOMLs, CLI template, and license files (default: true)"),
        )
        .arg(
            Arg::new("csharp")
                .long("csharp")
                .action(clap::ArgAction::SetTrue)
                .help("Also bump versions in C# SDK and templates"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .action(clap::ArgAction::SetTrue)
                .help("Update all targets (equivalent to --typescript --rust-and-cli --csharp)")
                .conflicts_with_all(["typescript", "rust-and-cli", "csharp"]),
        )
        .group(
            ArgGroup::new("update-targets")
                .args(["all", "typescript", "rust-and-cli", "csharp"])
                .required(true)
                .multiple(true),
        )
        .get_matches();

    let unparsed_version_arg = matches.get_one::<String>("upgrade_version").unwrap();
    let semver = Version::parse(unparsed_version_arg).expect("Invalid semver provided to upgrade-version");
    let full_version = format!("{}.{}.{}", semver.major, semver.minor, semver.patch);
    let wildcard_patch = format!("{}.{}.*", semver.major, semver.minor);

    if let Some(path) = matches.get_one::<PathBuf>("spacetime-path") {
        env::set_current_dir(path).ok();
    }

    let current_dir = env::current_dir().expect("No current directory!");
    let dir_name = current_dir.file_name().expect("No current directory!");
    if dir_name != "SpacetimeDB" && dir_name != "public" {
        anyhow::bail!("You must execute this binary from inside of the SpacetimeDB directory, or use --spacetime-path");
    }

    if matches.get_flag("rust-and-cli") || matches.get_flag("all") {
        // Use `=` for dependency versions, to avoid issues where Cargo automatically rolls forward to later minor versions.
        // See https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#default-requirements.
        let dep_version = format!("={full_version}");

        // root Cargo.toml
        edit_toml("Cargo.toml", |doc| {
            doc["workspace"]["package"]["version"] = toml_edit::value(full_version.clone());
            for (key, dep) in doc["workspace"]["dependencies"]
                .as_table_like_mut()
                .expect("workspace.dependencies is not a table")
                .iter_mut()
            {
                if key.get().starts_with("spacetime") {
                    dep["version"] = toml_edit::value(dep_version.clone())
                }
            }
        })?;

        edit_toml("crates/cli/templates/basic-rust/server/Cargo.toml", |doc| {
            // Only set major.minor.* for the spacetimedb dependency.
            // See https://github.com/clockworklabs/SpacetimeDB/issues/2724.
            //
            // Note: This is meaningfully different than setting just major.minor.
            // See https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#default-requirements.
            doc["dependencies"]["spacetimedb"] = toml_edit::value(wildcard_patch.clone());
        })?;

        edit_toml("crates/cli/templates/basic-rust/client/Cargo.toml", |doc| {
            doc["dependencies"]["spacetimedb-sdk"] = toml_edit::value(wildcard_patch.clone());
        })?;

        process_license_file("LICENSE.txt", &full_version);
        process_license_file("licenses/BSL.txt", &full_version);
        cmd!("cargo", "check").run().expect("Cargo check failed!");
    }

    if matches.get_flag("typescript") || matches.get_flag("all") {
        rewrite_json_version_inplace("crates/bindings-typescript/package.json", &full_version)?;

        rewrite_package_json_dependency_version_inplace(
            "crates/cli/src/subcommands/project/typescript/package._json",
            &wildcard_patch,
        )?;
    }

    if matches.get_flag("csharp") || matches.get_flag("all") {
        // Helpers for XML edits
        fn rewrite_xml_tag_value(path: &str, tag: &str, new_value: &str) -> anyhow::Result<()> {
            let contents = fs::read_to_string(path)?;
            let re = Regex::new(&format!(r"(?ms)(<\s*{tag}\s*>\s*)([^<]*?)(\s*<\s*/\s*{tag}\s*>)")).unwrap();
            let mut replaced = false;
            let updated = re.replacen(&contents, 1, |caps: &regex::Captures| {
                replaced = true;
                format!("{}{}{}", &caps[1], new_value, &caps[3])
            });
            if replaced {
                fs::write(path, updated.as_ref())?;
            }
            Ok(())
        }

        fn rewrite_csproj_package_ref_version(path: &str, package: &str, new_version: &str) -> anyhow::Result<()> {
            let contents = fs::read_to_string(path)?;
            let mut changed = false;
            // Version as attribute
            let re_attr = Regex::new(&format!(
                r#"(?ms)(<PackageReference[^>]*?Include="{}"[^>]*?\sVersion=")(.*?)(")"#,
                regex::escape(package)
            ))
            .unwrap();
            let updated_attr = re_attr.replace_all(&contents, |caps: &regex::Captures| {
                changed = true;
                format!("{}{}{}", &caps[1], new_version, &caps[3])
            });

            // Version as child element
            let re_child = Regex::new(&format!(
                r#"(?ms)(<PackageReference[^>]*?Include="{}"[^>]*?>.*?<Version>)([^<]*?)(</Version>)"#,
                regex::escape(package)
            ))
            .unwrap();
            let updated = re_child.replace_all(updated_attr.as_ref(), |caps: &regex::Captures| {
                changed = true;
                format!("{}{}{}", &caps[1], new_version, &caps[3])
            });

            if changed {
                fs::write(path, updated.as_ref())?;
            }
            Ok(())
        }

        // 1) Client SDK csproj
        let client_sdk = "sdks/csharp/SpacetimeDB.ClientSDK.csproj";
        rewrite_xml_tag_value(client_sdk, "Version", &full_version)?;
        rewrite_xml_tag_value(client_sdk, "AssemblyVersion", &full_version)?;
        // Update SpacetimeDB.BSATN.Runtime dependency to major.minor.*
        rewrite_csproj_package_ref_version(client_sdk, "SpacetimeDB.BSATN.Runtime", &wildcard_patch)?;

        // Also bump the C# SDK package.json version (preserve formatting)
        rewrite_json_version_inplace("sdks/csharp/package.json", &full_version)?;

        // 2) StdbModule.csproj files: SpacetimeDB.Runtime dependency -> major.minor
        let stdb_modules: &[&str] = &[
            "demo/Blackholio/server-csharp/StdbModule.csproj",
            "sdks/csharp/examples~/quickstart-chat/server/StdbModule.csproj",
            "sdks/csharp/examples~/regression-tests/server/StdbModule.csproj",
        ];
        for path in stdb_modules {
            rewrite_csproj_package_ref_version(path, "SpacetimeDB.Runtime", &wildcard_patch)?;
        }

        // 3) Version in BSATN.Runtime.csproj, Runtime.csproj, BSATN.Codegen.csproj, Codegen.csproj
        rewrite_xml_tag_value(
            "crates/bindings-csharp/BSATN.Runtime/BSATN.Runtime.csproj",
            "Version",
            &full_version,
        )?;
        rewrite_xml_tag_value(
            "crates/bindings-csharp/Runtime/Runtime.csproj",
            "Version",
            &full_version,
        )?;
        rewrite_xml_tag_value(
            "crates/bindings-csharp/BSATN.Codegen/BSATN.Codegen.csproj",
            "Version",
            &full_version,
        )?;
        rewrite_xml_tag_value(
            "crates/bindings-csharp/Codegen/Codegen.csproj",
            "Version",
            &full_version,
        )?;

        // 4) Template StdbModule.csproj: SpacetimeDB.Runtime dependency -> major.minor.*
        rewrite_csproj_package_ref_version(
            "crates/cli/templates/basic-c-sharp/server/StdbModule.csproj",
            "SpacetimeDB.Runtime",
            &wildcard_patch,
        )?;
    }

    Ok(())
}
