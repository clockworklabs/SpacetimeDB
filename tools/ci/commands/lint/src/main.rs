#![allow(clippy::disallowed_macros)]
#![allow(dead_code)]

use anyhow::{bail, Context, Result};
use ci_common::pnpm;
use duct::cmd;
use serde_json::Value;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

fn check_global_json_policy() -> Result<()> {
    ensure_repo_root()?;

    let root_json = Path::new("global.json");
    let root_contents = fs::read_to_string(root_json)?;

    let globals = git_tracked_files(":(glob)**/global.json")?;

    let mut ok = true;
    for p in globals {
        let meta = fs::symlink_metadata(&p)?;
        let is_symlink = meta.file_type().is_symlink();
        let is_template_global_json = p.strip_prefix(".").unwrap_or(&p).starts_with(Path::new("templates"));
        if is_template_global_json && is_symlink {
            eprintln!(
                "Error: {} is a symlink. Template files must not be symlinks; they are copied literally and this will break if the CLI is built under Windows where symlinks are not supported.",
                p.display()
            );
            ok = false;
        }

        let contents = fs::read_to_string(&p)?;
        if contents != root_contents {
            eprintln!("Error: {} does not match the root global.json contents", p.display());
            ok = false;
        } else if !is_template_global_json || !is_symlink {
            println!("OK: {}", p.display());
        }
    }

    if !ok {
        bail!("global.json policy check failed");
    }

    Ok(())
}

fn package_json_pnpm_version(package_manager: &str) -> Option<&str> {
    package_manager.strip_prefix("pnpm@")
}

fn git_tracked_files(pathspec: &str) -> Result<Vec<PathBuf>> {
    let output = cmd!("git", "ls-files", pathspec).read()?;
    Ok(output.lines().map(PathBuf::from).collect())
}

fn package_json_string_value(package_json: &Value, key: &str) -> Option<String> {
    package_json.get(key)?.as_str().map(str::to_owned)
}

fn package_json_engines_pnpm(package_json: &Value) -> Option<String> {
    package_json.get("engines")?.get("pnpm")?.as_str().map(str::to_owned)
}

fn read_package_json(path: &Path) -> Result<Value> {
    let contents = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&contents)?)
}

fn is_npm_package_json(package_json: &Value) -> bool {
    [
        "bin",
        "dependencies",
        "devDependencies",
        "exports",
        "main",
        "optionalDependencies",
        "packageManager",
        "peerDependencies",
        "scripts",
        "type",
    ]
    .iter()
    .any(|key| package_json.get(key).is_some())
}

fn is_template_path(path: &Path) -> bool {
    path.starts_with("templates")
}

fn minimum_release_age(path: &Path) -> Result<u64> {
    let workspace = fs::read_to_string(path)?;
    workspace
        .lines()
        .find_map(|line| {
            let line = line.trim();
            let value = line.strip_prefix("minimumReleaseAge:")?.trim();
            value.parse::<u64>().ok()
        })
        .ok_or_else(|| anyhow::anyhow!("{} is missing minimumReleaseAge", path.display()))
}

fn npmrc_minimum_release_age(path: &Path, expected_minimum_release_age: u64) -> Result<u64> {
    let contents = fs::read_to_string(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!(
                "{} is tracked but missing from the working tree. Restore it with:\nminimum-release-age={}",
                path.display(),
                expected_minimum_release_age
            )
        } else {
            anyhow::anyhow!(
                "failed to read {} while checking pnpm minimum package age: {err}",
                path.display()
            )
        }
    })?;
    contents
        .lines()
        .find_map(|line| {
            let line = line.trim();
            let value = line.strip_prefix("minimum-release-age=")?.trim();
            value.parse::<u64>().ok()
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{} must contain `minimum-release-age={}` to match root pnpm-workspace.yaml",
                path.display(),
                expected_minimum_release_age
            )
        })
}

fn check_pnpm_release_age_policy() -> Result<()> {
    ensure_repo_root()?;

    let root_package_json_path = Path::new("package.json");
    let root_package_json = read_package_json(root_package_json_path)?;
    let package_manager = package_json_string_value(&root_package_json, "packageManager")
        .ok_or_else(|| anyhow::anyhow!("package.json is missing packageManager"))?;
    let package_manager_version = package_json_pnpm_version(&package_manager)
        .ok_or_else(|| anyhow::anyhow!("packageManager must be pnpm@<version>, found {package_manager:?}"))?;

    let expected_engine_pnpm = format!(">={package_manager_version}");
    let engine_pnpm = package_json_engines_pnpm(&root_package_json)
        .ok_or_else(|| anyhow::anyhow!("package.json engines is missing pnpm"))?;
    if engine_pnpm != expected_engine_pnpm {
        bail!("package.json engines.pnpm must be {expected_engine_pnpm:?}, found {engine_pnpm:?}");
    }

    for package_json_path in git_tracked_files(":(glob)**/package.json")? {
        let package_json = read_package_json(&package_json_path)?;
        let Some(found_package_manager) = package_json_string_value(&package_json, "packageManager") else {
            continue;
        };
        if found_package_manager != package_manager {
            bail!(
                "{} packageManager must match root package.json: expected {:?}, found {:?}",
                package_json_path.display(),
                package_manager,
                found_package_manager
            );
        }
    }

    let root_workspace_path = Path::new("pnpm-workspace.yaml");
    let root_minimum_release_age = minimum_release_age(root_workspace_path)?;
    for workspace_path in git_tracked_files(":(glob)**/pnpm-workspace.yaml")? {
        let found_minimum_release_age = minimum_release_age(&workspace_path)?;
        if found_minimum_release_age != root_minimum_release_age {
            bail!(
                "{} minimumReleaseAge must match root pnpm-workspace.yaml: expected {}, found {}",
                workspace_path.display(),
                root_minimum_release_age,
                found_minimum_release_age
            );
        }
    }

    for npmrc_path in git_tracked_files(":(glob)**/.npmrc")? {
        // Template package roots are copied into projects created by `spacetime init`.
        // They must not embed this repo's package-age policy; smoketests enforce it
        // at the pnpm process boundary instead.
        if is_template_path(&npmrc_path) {
            continue;
        }
        let found_minimum_release_age = npmrc_minimum_release_age(&npmrc_path, root_minimum_release_age)?;
        if found_minimum_release_age != root_minimum_release_age {
            bail!(
                "{} minimum-release-age must match root pnpm-workspace.yaml: expected {}, found {}",
                npmrc_path.display(),
                root_minimum_release_age,
                found_minimum_release_age
            );
        }
    }

    for package_json_path in git_tracked_files(":(glob)**/package.json")? {
        // Template package roots are copied into projects created by `spacetime init`.
        // They must not require adjacent .npmrc files for this repo's package-age
        // policy; smoketests enforce it at the pnpm process boundary instead.
        if is_template_path(&package_json_path) {
            continue;
        }
        let package_json = read_package_json(&package_json_path)?;
        if !is_npm_package_json(&package_json) {
            continue;
        }
        let package_dir = package_json_path
            .parent()
            .expect("git-tracked package.json path should have a parent");
        let npmrc_path = package_dir.join(".npmrc");
        if !npmrc_path.is_file() {
            bail!(
                "{} is required because {} is an npm/pnpm package manifest.\nAdd {} containing:\nminimum-release-age={}",
                npmrc_path.display(),
                package_json_path.display(),
                npmrc_path.display(),
                root_minimum_release_age
            );
        }
        let found_minimum_release_age = npmrc_minimum_release_age(&npmrc_path, root_minimum_release_age)?;
        if found_minimum_release_age != root_minimum_release_age {
            bail!(
                "{} minimum-release-age must match root pnpm-workspace.yaml: expected {}, found {}",
                npmrc_path.display(),
                root_minimum_release_age,
                found_minimum_release_age
            );
        }
    }

    for workflow_path in git_tracked_files(".github/workflows/*")? {
        let contents = fs::read_to_string(&workflow_path)?;
        if contents.contains("pnpm/action-setup@v4") {
            bail!(
                "{} must use ./.github/actions/setup-pnpm instead of pnpm/action-setup@v4",
                workflow_path.display()
            );
        }
    }

    Ok(())
}

/// Codex plugin ships a copy of `skills/`, because plugin installers do not follow symlinks,
/// this checks if the copy is in sync
fn check_codex_plugin_skills_sync() -> Result<()> {
    let source = Path::new("skills");
    let copy = Path::new("codex-plugin/plugins/spacetimedb/skills");

    if fs::symlink_metadata(copy)
        .with_context(|| format!("reading {}", copy.display()))?
        .file_type()
        .is_symlink()
    {
        bail!(
            "{} must be a real directory, not a symlink, plugin installers do not follow symlinks",
            copy.display()
        );
    }

    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                walk(root, &entry.path(), out)?;
            } else {
                let rel = entry
                    .path()
                    .strip_prefix(root)
                    .expect("walked path should be under its root")
                    .to_path_buf();
                out.insert(rel);
            }
        }
        Ok(())
    }

    let mut source_files = BTreeSet::new();
    walk(source, source, &mut source_files)?;
    let mut copy_files = BTreeSet::new();
    walk(copy, copy, &mut copy_files)?;

    let mut drift = Vec::new();
    for rel in &source_files {
        if !copy_files.contains(rel) {
            drift.push(format!("missing from copy: {}", rel.display()));
        } else if fs::read(source.join(rel))? != fs::read(copy.join(rel))? {
            drift.push(format!("differs: {}", rel.display()));
        }
    }
    for rel in &copy_files {
        if !source_files.contains(rel) {
            drift.push(format!("extraneous in copy: {}", rel.display()));
        }
    }

    if !drift.is_empty() {
        bail!(
            "codex-plugin skills copy is out of sync with skills/:\n  {}\nRun: node codex-plugin/scripts/check-skills-sync.ts --fix",
            drift.join("\n  ")
        );
    }
    Ok(())
}

fn ensure_repo_root() -> Result<()> {
    if !Path::new("Cargo.toml").exists() {
        bail!("You must execute this command from the SpacetimeDB repository root (where Cargo.toml is located)");
    }
    Ok(())
}
fn tracked_rs_files_under(path: &str) -> Result<Vec<PathBuf>> {
    let output = cmd!("git", "ls-files", "--", path).read()?;
    Ok(output
        .lines()
        .filter(|line| line.ends_with(".rs"))
        .map(PathBuf::from)
        .collect())
}

fn main() -> Result<()> {
    env_logger::init();
    ensure_repo_root()?;
    check_pnpm_release_age_policy()?;
    check_codex_plugin_skills_sync()?;
    // `cargo fmt --all` only checks files that Cargo discovers through workspace/package targets.
    // However, we also keep Rust sources in a locations that are tracked but not part of our workspace,
    // so this approach properly catches all the files, where `cargo fmt` does not.
    let mut files = Vec::new();
    files.extend(tracked_rs_files_under(".")?);
    const RUSTFMT_BATCH_SIZE: usize = 200;
    for batch in files.chunks(RUSTFMT_BATCH_SIZE) {
        let mut args = Vec::<OsString>::with_capacity(batch.len() + 1);
        args.push("--check".into());
        args.extend(batch.iter().map(|path| path.as_os_str().to_os_string()));
        cmd("rustfmt", args).run()?;
    }
    cmd!(
        "cargo",
        "clippy",
        "--timings",
        "--all",
        "--tests",
        "--benches",
        "--",
        "-D",
        "warnings",
    )
    .run()?;
    cmd!(
        "cargo",
        "clippy",
        "--timings",
        "--no-default-features",
        "--features=browser",
        "-pspacetimedb-sdk",
        "--tests",
        "--benches",
        "--",
        "-D",
        "warnings",
    )
    .run()?;
    cmd!("dotnet", "tool", "restore").dir("crates/bindings-csharp").run()?;
    cmd!("dotnet", "csharpier", "--check", ".")
        .dir("crates/bindings-csharp")
        .run()?;
    pnpm(["lint"]).run()?;
    cmd!("cargo", "test", "--doc", "--target", "wasm32-unknown-unknown")
        .dir("crates/bindings")
        .run()?;
    cmd!("cargo", "test", "--doc").dir("crates/bindings").run()?;
    // `bindings` is the only crate we care strongly about documenting,
    // since we link to its docs.rs from our website.
    // We won't pass `--no-deps`, though,
    // since we want everything reachable through it to also work.
    // This includes `sats` and `lib`.
    cmd!("cargo", "doc")
        .dir("crates/bindings")
        // Make `cargo doc` exit with error on warnings, most notably broken links
        .env("RUSTDOCFLAGS", "--deny warnings")
        .run()?;

    Ok(())
}
