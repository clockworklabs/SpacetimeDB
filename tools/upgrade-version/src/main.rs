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

    if matches.get_flag("rust-and-cli") {
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

    if matches.get_flag("typescript") {
        rewrite_json_version_inplace("crates/bindings-typescript/package.json", version)?;
    }

    if matches.get_flag("csharp") {
        // Compute various version forms
        let v = Version::parse(version).expect("Invalid semver provided to upgrade-version");
        let wildcard_patch = format!("{}.{}.*", v.major, v.minor);
        let assembly_version = format!("{}.{}.{}", v.major, v.minor, v.patch);

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
        rewrite_xml_tag_value(client_sdk, "Version", version)?;
        rewrite_xml_tag_value(client_sdk, "AssemblyVersion", &assembly_version)?;
        // Update SpacetimeDB.BSATN.Runtime dependency to major.minor.*
        rewrite_csproj_package_ref_version(client_sdk, "SpacetimeDB.BSATN.Runtime", &wildcard_patch)?;

        // Also bump the C# SDK package.json version (preserve formatting)
        rewrite_json_version_inplace("sdks/csharp/package.json", version)?;

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
            version,
        )?;
        rewrite_xml_tag_value("crates/bindings-csharp/Runtime/Runtime.csproj", "Version", version)?;
        rewrite_xml_tag_value(
            "crates/bindings-csharp/BSATN.Codegen/BSATN.Codegen.csproj",
            "Version",
            version,
        )?;
        rewrite_xml_tag_value("crates/bindings-csharp/Codegen/Codegen.csproj", "Version", version)?;

        // 4) Template StdbModule._csproj: SpacetimeDB.Runtime dependency -> major.minor.*
        rewrite_csproj_package_ref_version(
            "crates/cli/src/subcommands/project/csharp/StdbModule._csproj",
            "SpacetimeDB.Runtime",
            &wildcard_patch,
        )?;
    }

    Ok(())
}
