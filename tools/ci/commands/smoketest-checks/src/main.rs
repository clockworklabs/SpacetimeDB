#![allow(clippy::disallowed_macros)]

use std::ffi::OsStr;
use std::fs;
use std::io::{Error, Result};
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    if !Path::new("Cargo.toml").exists() {
        return Err(Error::other(
            "You must execute this command from the SpacetimeDB repository root",
        ));
    }
    check_smoketest_module_lists_complete()?;
    check_no_require_local_server_cluster_tests()?;
    eprintln!("smoketest module lists and suite constraints are up to date.");
    Ok(())
}

fn check_smoketest_module_lists_complete() -> Result<()> {
    let tests_dir = Path::new("crates/smoketests/tests");
    for suite in ["cluster", "standalone"] {
        let suite_dir = tests_dir.join(suite);
        let suite_root = tests_dir.join(format!("{suite}.rs"));
        for source in collect_rust_sources(&suite_dir)? {
            let source_dir = source
                .parent()
                .ok_or_else(|| Error::other("smoketest source has no parent"))?;
            let (module_dir, module_name) = if source.file_name() == Some(OsStr::new("mod.rs")) {
                (
                    source_dir
                        .parent()
                        .ok_or_else(|| Error::other("nested module has no parent"))?,
                    source_dir
                        .file_name()
                        .ok_or_else(|| Error::other("nested module has no name"))?
                        .to_string_lossy()
                        .into_owned(),
                )
            } else {
                (
                    source_dir,
                    source
                        .file_stem()
                        .ok_or_else(|| Error::other("smoketest source has no module name"))?
                        .to_string_lossy()
                        .into_owned(),
                )
            };
            let module_file = if module_dir == suite_dir {
                suite_root.clone()
            } else {
                module_dir.join("mod.rs")
            };
            let module_list = fs::read_to_string(&module_file)?;
            let private = format!("mod {module_name};");
            let public = format!("pub mod {module_name};");
            if !module_list
                .lines()
                .any(|line| matches!(line.trim(), value if value == private || value == public))
            {
                return Err(Error::other(format!(
                    "{} does not declare module {module_name} from {}",
                    module_file.display(),
                    source.display(),
                )));
            }
        }
    }
    Ok(())
}

fn check_no_require_local_server_cluster_tests() -> Result<()> {
    let mut misplaced_guards = Vec::new();
    let mut cluster_sources = collect_rust_sources(Path::new("crates/smoketests/tests/cluster"))?;
    cluster_sources.push(PathBuf::from("crates/smoketests/tests/cluster.rs"));
    for source in cluster_sources {
        if fs::read_to_string(&source)?.contains("require_local_server!") {
            misplaced_guards.push(source);
        }
    }

    if !misplaced_guards.is_empty() {
        return Err(Error::other(format!(
            "require_local_server!() may not be used in cluster smoketests:\n{}",
            misplaced_guards
                .iter()
                .map(|path| format!("- {}", path.display()))
                .collect::<Vec<_>>()
                .join("\n")
        )));
    }
    Ok(())
}

fn collect_rust_sources(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut sources = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            sources.extend(collect_rust_sources(&path)?);
        } else if path.extension() == Some(OsStr::new("rs")) {
            sources.push(path);
        }
    }
    Ok(sources)
}
