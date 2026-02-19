use crate::context::constants::docs_dir;
use crate::context::hashing::gather_docs_files;
use crate::context::{rustdoc_crate_root, rustdoc_readme_path};
use crate::eval::lang::Lang;
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
        "cursor_rules" => gather_cursor_rules_files(docs_dir().join(".cursor/rules"), None),
        "rustdoc_json" => resolve_rustdoc_json_paths_always(),
        "none" => Ok(Vec::new()),
        other => bail!("unknown mode `{other}` (expected: docs | llms.md | cursor_rules | rustdoc_json | none)"),
    }
}

/// Cursor rules under docs: include general rules + rules for the given language.
/// General = filename (lowercase) does not contain "typescript", "rust", or "csharp".
/// Lang-specific = filename contains lang (e.g. "typescript" for TypeScript).
pub fn gather_cursor_rules_files(rules_dir: PathBuf, lang: Option<Lang>) -> Result<Vec<PathBuf>> {
    if !rules_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out: Vec<PathBuf> = fs::read_dir(&rules_dir)
        .with_context(|| format!("read cursor rules dir {}", rules_dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e == "md" || e == "mdc")
                .unwrap_or(false)
        })
        .collect();
    if let Some(l) = lang {
        let tag = l.as_str();
        out.retain(|p| {
            let name = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            let is_general = !name.contains("typescript") && !name.contains("rust") && !name.contains("csharp");
            let is_lang = name.contains(tag);
            is_general || is_lang
        });
    }
    out.sort();
    Ok(out)
}

// --- hashing resolver stays as you wrote it ---
pub fn resolve_mode_paths_hashing(mode: &str) -> Result<Vec<PathBuf>> {
    match mode {
        "docs" => gather_docs_files(),
        "llms.md" => Ok(vec![docs_dir().join("llms.md")]),
        "cursor_rules" => gather_cursor_rules_files(docs_dir().join(".cursor/rules"), None),
        "none" => Ok(Vec::new()),
        "rustdoc_json" => {
            if let Some(p) = rustdoc_readme_path() {
                Ok(vec![p])
            } else {
                bail!("README not found under {}", rustdoc_crate_root().display())
            }
        }
        other => bail!("unknown mode `{other}` (expected: docs | llms.md | cursor_rules | rustdoc_json | none)"),
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
