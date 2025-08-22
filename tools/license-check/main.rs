use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn main() -> Result<()> {
    let repo_root = find_repo_root().context("Could not locate repo root (looked for `.git/` or `licenses/`)")?;

    check_license_symlinks(&repo_root)?;
    println!("All LICENSE files are valid symlinks into `licenses/`.");
    Ok(())
}

fn find_repo_root() -> Option<PathBuf> {
    let mut dir = env::current_dir().ok()?.to_path_buf();
    loop {
        if dir.join(".git").is_dir() || dir.join("licenses").is_dir() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn relative_to(path: &Path, root: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) => rel.display().to_string(),
        Err(_) => path.display().to_string(), // fallback if not under repo_root
    }
}

fn check_license_symlinks(repo_root: &Path) -> Result<()> {
    let licenses_dir = repo_root.join("licenses");
    if !licenses_dir.is_dir() {
        bail!(
            "Required directory 'licenses/' not found at the repo root: {}",
            licenses_dir.display()
        );
    }

    let licenses_dir_canon = fs::canonicalize(&licenses_dir)
        .with_context(|| format!("Could not canonicalize licenses dir: {}", licenses_dir.display()))?;

    let ignore_list = ["LICENSE.txt", "crates/sqltest/standards/LICENSE"];
    let mut errors: Vec<String> = Vec::new();

    for entry in WalkDir::new(repo_root).into_iter().filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy();
        if name != "LICENSE" && name != "LICENSE.txt" {
            continue;
        }

        let path = entry.into_path();
        let rel_str = relative_to(&path, repo_root);
        if ignore_list.contains(&rel_str.as_str()) {
            continue;
        }

        if let Err(e) = validate_one_license(path, repo_root, &licenses_dir_canon) {
            // include the relative path to make the report easy to scan
            errors.push(e.to_string());
        }
    }

    if !errors.is_empty() {
        bail!("Found invalid LICENSE symlinks:\n{}", errors.join("\n"));
    }

    Ok(())
}

fn validate_one_license(path: PathBuf, repo_root: &Path, licenses_dir_canon: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(&path)
        .with_context(|| format!("Could not stat file {}", relative_to(&path, repo_root)))?;

    if !meta.file_type().is_symlink() {
        bail!(
            "{}: Must be a symlink pointing into 'licenses/'.",
            relative_to(&path, repo_root)
        );
    }

    let raw_target = fs::read_link(&path)
        .with_context(|| format!("Could not read symlink target {}", relative_to(&path, repo_root)))?;

    let resolved =
        fs::canonicalize(resolve_relative_target(&raw_target, path.parent().unwrap())).with_context(|| {
            format!(
                "{}: Broken symlink (target {}).",
                relative_to(&path, repo_root),
                raw_target.display()
            )
        })?;

    if !is_within(&resolved, licenses_dir_canon) {
        bail!(
            "{}: Symlink target must be inside 'licenses/' (got {}).",
            relative_to(&path, repo_root),
            relative_to(&resolved, repo_root)
        );
    }

    Ok(())
}

/// Is `child` inside `base`?
fn is_within(child: &Path, base: &Path) -> bool {
    child.ancestors().any(|a| a == base)
}

/// Resolve a relative symlink target against its parent directory.
fn resolve_relative_target(target: &Path, link_parent: &Path) -> PathBuf {
    if target.is_absolute() {
        target.to_path_buf()
    } else {
        link_parent.join(target)
    }
}
