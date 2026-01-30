use crate::context::constants::docs_dir;
use crate::context::hashing::gather_docs_files;
use crate::context::{rustdoc_crate_root, rustdoc_readme_path};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

/// Pinned nightly version for rustdoc JSON generation.
/// This ensures consistent output across CI runs regardless of when they execute.
/// Update this when intentionally upgrading to a newer nightly.
const PINNED_NIGHTLY: &str = "nightly-2026-01-15";

pub fn resolve_mode_paths(mode: &str) -> Result<Vec<PathBuf>> {
    match mode {
        "docs" => gather_docs_files(),
        "llms.md" => Ok(vec![docs_dir().join("llms.md")]),
        "cursor_rules" => Ok(vec![docs_dir().join(".cursor/rules/spacetimedb.md")]),
        "rustdoc_json" => resolve_rustdoc_json_paths_always(),
        other => bail!("unknown mode `{other}` (expected: docs | llms.md | cursor_rules | rustdoc_json)"),
    }
}

// --- hashing resolver stays as you wrote it ---
pub fn resolve_mode_paths_hashing(mode: &str) -> Result<Vec<PathBuf>> {
    match mode {
        "docs" => gather_docs_files(),
        "llms.md" => Ok(vec![docs_dir().join("llms.md")]),
        "cursor_rules" => Ok(vec![docs_dir().join(".cursor/rules/spacetimedb.md")]),
        "rustdoc_json" => {
            if let Some(p) = rustdoc_readme_path() {
                Ok(vec![p])
            } else {
                bail!("README not found under {}", rustdoc_crate_root().display())
            }
        }
        other => bail!("unknown mode `{other}` (expected: docs | llms.md | cursor_rules | rustdoc_json)"),
    }
}

fn resolve_rustdoc_json_paths_always() -> Result<Vec<PathBuf>> {
    // Always rebuild JSON
    generate_rustdoc_json()?;

    // Then read from target/doc
    if let Some(p) = find_target_doc_json("spacetimedb") {
        return Ok(vec![p]);
    }
    bail!("rustdoc_json: missing target/doc/spacetimedb.json after generation")
}

fn workspace_target_dir() -> Result<(PathBuf, PathBuf)> {
    // -> (target_dir, workspace_root)
    let out = Command::new("cargo")
        .args(["metadata", "--format-version=1", "--no-deps"])
        .output()
        .context("running `cargo metadata`")?;
    if !out.status.success() {
        bail!("cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let v: Value = serde_json::from_slice(&out.stdout)?;
    let target = v["target_directory"]
        .as_str()
        .ok_or_else(|| anyhow!("missing target_directory"))?;
    let root = v["workspace_root"]
        .as_str()
        .ok_or_else(|| anyhow!("missing workspace_root"))?;
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(target));
    Ok((target_dir, PathBuf::from(root)))
}

fn find_target_doc_json(crate_name: &str) -> Option<PathBuf> {
    let (target_dir, _) = workspace_target_dir().ok()?;
    let file = format!("{}.json", crate_name.replace('-', "_"));
    let cand = target_dir.join("doc").join(file);
    if cand.is_file() {
        return Some(cand);
    }
    // Tiny fallback: newest *.json in target/doc
    fs::read_dir(target_dir.join("doc"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
}

fn generate_rustdoc_json() -> Result<()> {
    // Install the pinned nightly toolchain if not present
    let _ = Command::new("rustup")
        .args(["toolchain", "install", PINNED_NIGHTLY])
        .status();

    let (_target_dir, workspace_root) = workspace_target_dir()?;

    // Run from the *workspace root* so output lands in the shared target/
    let toolchain_arg = format!("+{}", PINNED_NIGHTLY);
    let status = Command::new("cargo")
        .current_dir(&workspace_root)
        .args([&toolchain_arg, "rustdoc", "-p", "spacetimedb", "--"])
        .args(["-Z", "unstable-options", "--output-format", "json"])
        .status()
        .with_context(|| {
            format!(
                "running cargo {} rustdoc -p spacetimedb -- -Z unstable-options --output-format json",
                toolchain_arg
            )
        })?;

    if !status.success() {
        bail!("cargo rustdoc failed with status {:?}", status.code());
    }
    Ok(())
}
