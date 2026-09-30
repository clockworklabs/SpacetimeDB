#![allow(clippy::disallowed_macros)]

//! Rejects newly added Bash and Python scripts in a Git repository.
//!
//! The check compares `HEAD` with the supplied base ref's merge base, asks Git
//! for added paths using NUL-delimited output. It checks known script extensions
//! directly and reads extensionless files to inspect their shebangs. Callers provide
//! `is_allowed` for repository-specific exceptions;
//! any remaining paths are reported together and cause the check to fail.

use anyhow::{bail, ensure, Context, Result};
use duct::cmd;
use std::path::{Path, PathBuf};

pub fn reject_new_scripts(repo_root: &Path, base_ref: &str, is_allowed: impl Fn(&Path) -> bool) -> Result<()> {
    let base_commit = fetch_base_ref(repo_root, base_ref)?;
    let merge_base = cmd!("git", "merge-base", &base_commit, "HEAD")
        .dir(repo_root)
        .read()
        .with_context(|| format!("failed to find merge base with {base_ref}"))?;
    let merge_base = merge_base.trim();
    let violations = find_violations(repo_root, merge_base, is_allowed)?;
    if violations.is_empty() {
        return Ok(());
    }
    eprintln!("The following new files are not allowed:");
    for path in &violations {
        eprintln!("  {}", path.display());
    }
    bail!(
        "Please implement new scripting in Rust. If an exception is needed, add the path or directory to the allowlist in the Rust check."
    )
}

fn fetch_base_ref(repo_root: &Path, base_ref: &str) -> Result<String> {
    let revision = if let Some(ref_name) = base_ref.strip_prefix("origin/") {
        cmd!("git", "fetch", "--no-tags", "origin", ref_name)
            .dir(repo_root)
            .run()
            .with_context(|| format!("failed to fetch base ref {base_ref}"))?;
        "FETCH_HEAD"
    } else {
        base_ref
    };
    cmd!("git", "rev-parse", revision)
        .dir(repo_root)
        .read()
        .with_context(|| format!("failed to resolve base ref {base_ref}"))
}

#[cfg(unix)]
fn path_from_git(bytes: &[u8]) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Ok(std::ffi::OsString::from_vec(bytes.to_vec()).into())
}

#[cfg(not(unix))]
fn path_from_git(bytes: &[u8]) -> Result<PathBuf> {
    Ok(String::from_utf8(bytes.to_vec())?.into())
}

#[cfg(unix)]
fn path_matches_git(path: &Path, bytes: &[u8]) -> bool {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes() == bytes
}

#[cfg(not(unix))]
fn path_matches_git(path: &Path, bytes: &[u8]) -> bool {
    path.to_string_lossy().as_bytes() == bytes
}

fn find_violations(repo_root: &Path, merge_base: &str, is_allowed: impl Fn(&Path) -> bool) -> Result<Vec<PathBuf>> {
    let added_paths = find_added_files(repo_root, merge_base)?;
    check_files_for_violations(added_paths, is_allowed, |path| read_blob(repo_root, "HEAD", path))
}

fn find_added_files(repo_root: &Path, merge_base: &str) -> Result<Vec<PathBuf>> {
    // NUL delimiters preserve unusual filenames.
    let output = cmd!(
        "git",
        "diff",
        "--name-only",
        "--diff-filter=A",
        "--find-renames",
        "-z",
        merge_base,
        "HEAD"
    )
    .dir(repo_root)
    .stdout_capture()
    .run()
    .context("failed to list added files")?
    .stdout;
    ensure!(
        output.is_empty() || output.last() == Some(&0),
        "truncated git diff output"
    );
    output
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(path_from_git)
        .collect()
}

fn check_files_for_violations(
    paths: Vec<PathBuf>,
    is_allowed: impl Fn(&Path) -> bool,
    read_contents: impl Fn(&Path) -> Result<Option<Vec<u8>>>,
) -> Result<Vec<PathBuf>> {
    let mut violations = Vec::new();
    for path in paths {
        if is_allowed(&path) {
            continue;
        }
        let is_script = if path.extension().is_some() {
            is_script_path(&path)
        } else {
            read_contents(&path)?.is_some_and(|contents| is_script_contents(&contents))
        };
        if is_script {
            violations.push(path);
        }
    }
    Ok(violations)
}

fn read_blob(repo_root: &Path, revision: &str, path: &Path) -> Result<Option<Vec<u8>>> {
    let entries = cmd!("git", "ls-tree", "-z", revision, "--", path)
        .dir(repo_root)
        .stdout_capture()
        .run()
        .with_context(|| format!("failed to inspect {} at {revision}", path.display()))?
        .stdout;
    for entry in entries.split(|byte| *byte == 0) {
        let Some(tab) = entry.iter().position(|byte| *byte == b'\t') else {
            continue;
        };
        if !path_matches_git(path, &entry[tab + 1..]) {
            continue;
        }
        let mut metadata = entry[..tab].split(|byte| *byte == b' ');
        let _mode = metadata.next();
        if metadata.next() != Some(b"blob".as_slice()) {
            return Ok(None);
        }
        let hash = metadata.next().context("missing blob hash in git ls-tree output")?;
        let hash = std::str::from_utf8(hash).context("invalid blob hash in git ls-tree output")?;
        return Ok(Some(
            cmd!("git", "cat-file", "blob", hash)
                .dir(repo_root)
                .stdout_capture()
                .run()
                .with_context(|| format!("failed to read blob for {} at {revision}", path.display()))?
                .stdout,
        ));
    }
    Ok(None)
}

fn is_script_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "sh" | "bash" | "py" | "pyw"))
}

fn is_script_contents(contents: &[u8]) -> bool {
    let first_line = contents.split(|byte| *byte == b'\n').next().unwrap_or_default();
    let Some(shebang) = first_line.strip_prefix(b"#!") else {
        return false;
    };
    let Ok(shebang) = std::str::from_utf8(shebang) else {
        return false;
    };
    let mut words = shebang.split_whitespace();
    let Some(interpreter) = words.next() else {
        return false;
    };
    let interpreter = if interpreter.ends_with("/env") {
        words.find(|word| !word.starts_with('-'))
    } else {
        Some(interpreter)
    };
    interpreter
        .and_then(|word| word.rsplit('/').next())
        .is_some_and(|name| {
            name == "bash"
                || name == "python"
                || name
                    .strip_prefix("python")
                    .and_then(|suffix| suffix.chars().next())
                    .is_some_and(|character| character.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn finds_added_files_without_renames() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        cmd!("git", "init", "-q", "-b", "base").dir(root).run().unwrap();
        cmd!("git", "config", "user.name", "Test").dir(root).run().unwrap();
        cmd!("git", "config", "user.email", "test@example.com")
            .dir(root)
            .run()
            .unwrap();
        std::fs::write(root.join("existing.sh"), "#!/bin/bash\n").unwrap();
        cmd!("git", "add", ".").dir(root).run().unwrap();
        cmd!("git", "commit", "-qm", "base").dir(root).run().unwrap();
        let base = cmd!("git", "rev-parse", "HEAD").dir(root).read().unwrap();
        cmd!("git", "mv", "existing.sh", "moved.sh").dir(root).run().unwrap();
        std::fs::write(root.join("new.py"), "print(1)\n").unwrap();
        cmd!("git", "add", ".").dir(root).run().unwrap();
        cmd!("git", "commit", "-qm", "change").dir(root).run().unwrap();

        assert_eq!(
            find_added_files(root, base.trim()).unwrap(),
            vec![PathBuf::from("new.py")]
        );
    }

    #[test]
    fn checks_file_list_with_caller_allowlist() {
        let paths = ["new.py", "allowed.sh", "notes.txt", "script", "plain", "gitlink"]
            .map(PathBuf::from)
            .to_vec();
        let read_paths = RefCell::new(Vec::new());
        let violations = check_files_for_violations(
            paths,
            |path| path == Path::new("allowed.sh"),
            |path| {
                read_paths.borrow_mut().push(path.to_path_buf());
                Ok(match path.to_str().unwrap() {
                    "script" => Some(b"#!/usr/bin/env python3\n".to_vec()),
                    "plain" => Some(b"plain text\n".to_vec()),
                    "gitlink" => None,
                    _ => panic!("unexpected blob read: {}", path.display()),
                })
            },
        )
        .unwrap();

        assert_eq!(violations, vec![PathBuf::from("new.py"), PathBuf::from("script")]);
        assert_eq!(
            read_paths.into_inner(),
            vec![
                PathBuf::from("script"),
                PathBuf::from("plain"),
                PathBuf::from("gitlink")
            ]
        );
    }

    #[test]
    fn detects_extensions_and_shebangs() {
        for path in ["tool.sh", "tool.bash", "tool.py", "tool.pyw"] {
            assert!(is_script_path(Path::new(path)));
        }
        assert!(!is_script_path(Path::new("tool.txt")));
        for shebang in [
            b"#!/bin/bash\n".as_slice(),
            b"#!/usr/bin/env bash\n",
            b"#!/usr/bin/env -S python3 -u\n",
            b"#!/usr/bin/python3.12\n",
        ] {
            assert!(is_script_contents(shebang));
        }
        assert!(!is_script_contents(b"#!/usr/bin/env node\n"));
        assert!(!is_script_contents(b"fn main() {}"));
    }
}
