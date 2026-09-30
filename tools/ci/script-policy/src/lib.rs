#![allow(clippy::disallowed_macros)]

use anyhow::{bail, ensure, Context, Result};
use duct::cmd;
use std::path::{Path, PathBuf};

struct ChangedFile {
    old_path: Option<PathBuf>,
    new_path: Option<PathBuf>,
}

pub fn reject_new_scripts(repo_root: &Path, base_ref: &str, is_allowed: impl Fn(&Path) -> bool) -> Result<()> {
    let base_commit = fetch_base_ref(repo_root, base_ref)?;
    let merge_base = cmd!("git", "merge-base", &base_commit, "HEAD")
        .dir(repo_root)
        .read()
        .with_context(|| format!("failed to find merge base with {base_ref}"))?;
    let merge_base = merge_base.trim();
    let changes = changed_files(repo_root, merge_base)?;
    let violations = find_violations(repo_root, &changes, merge_base, is_allowed)?;
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

fn changed_files(repo_root: &Path, merge_base: &str) -> Result<Vec<ChangedFile>> {
    let output = cmd!(
        "git",
        "diff",
        "--name-status",
        "-z",
        "--find-renames",
        merge_base,
        "HEAD"
    )
    .dir(repo_root)
    .stdout_capture()
    .run()
    .context("failed to list changed files")?
    .stdout;
    parse_changed_files(&output)
}

fn parse_changed_files(output: &[u8]) -> Result<Vec<ChangedFile>> {
    ensure!(
        output.is_empty() || output.last() == Some(&0),
        "truncated git diff output"
    );
    let mut fields = output.split(|byte| *byte == 0);
    let mut changes = Vec::new();
    while let Some(status) = fields.next() {
        if status.is_empty() {
            break;
        }
        let path_field = fields.next().context("missing path in git diff output")?;
        ensure!(!path_field.is_empty(), "empty path in git diff output");
        let path = path_from_git(path_field)?;
        let change = match status.first() {
            Some(b'A') => ChangedFile {
                old_path: None,
                new_path: Some(path),
            },
            Some(b'D') => ChangedFile {
                old_path: Some(path),
                new_path: None,
            },
            Some(b'R' | b'C') => ChangedFile {
                old_path: Some(path),
                new_path: Some({
                    let destination = fields.next().context("missing destination in git diff output")?;
                    ensure!(!destination.is_empty(), "empty destination in git diff output");
                    path_from_git(destination)?
                }),
            },
            Some(b'M' | b'T') => ChangedFile {
                old_path: Some(path.clone()),
                new_path: Some(path),
            },
            _ => bail!("unexpected git diff status {}", String::from_utf8_lossy(status)),
        };
        changes.push(change);
    }
    Ok(changes)
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

fn find_violations(
    repo_root: &Path,
    changes: &[ChangedFile],
    merge_base: &str,
    is_allowed: impl Fn(&Path) -> bool,
) -> Result<Vec<PathBuf>> {
    let mut violations = Vec::new();
    for change in changes {
        let Some(new_path) = &change.new_path else {
            continue;
        };
        let Some(new_contents) = read_blob(repo_root, "HEAD", new_path)? else {
            continue;
        };
        if !is_script(new_path, &new_contents) || is_allowed(new_path) {
            continue;
        }
        let old_contents = change
            .old_path
            .as_ref()
            .map(|path| read_blob(repo_root, merge_base, path))
            .transpose()?
            .flatten();
        if is_new_script(
            change.old_path.as_deref().zip(old_contents.as_deref()),
            new_path,
            &new_contents,
        ) {
            violations.push(new_path.clone());
        }
    }
    Ok(violations)
}

fn is_new_script(old: Option<(&Path, &[u8])>, new_path: &Path, new_contents: &[u8]) -> bool {
    is_script(new_path, new_contents) && !old.is_some_and(|(path, contents)| is_script(path, contents))
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

fn is_script(path: &Path, contents: &[u8]) -> bool {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "sh" | "bash" | "py" | "pyw"))
    {
        return true;
    }

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

    #[test]
    fn repository_scan_uses_caller_allowlist() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        for args in [
            vec!["init", "-q", "-b", "base"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.com"],
        ] {
            cmd("git", args).dir(root).run().unwrap();
        }
        std::fs::write(root.join("notes"), "plain text\n").unwrap();
        cmd!("git", "add", ".").dir(root).run().unwrap();
        cmd!("git", "commit", "-qm", "base").dir(root).run().unwrap();
        cmd!("git", "switch", "-qc", "change").dir(root).run().unwrap();
        std::fs::write(root.join("new.py"), "print(1)\n").unwrap();
        cmd!("git", "add", ".").dir(root).run().unwrap();
        cmd!("git", "commit", "-qm", "add script").dir(root).run().unwrap();
        let base = cmd!("git", "rev-parse", "base").dir(root).read().unwrap();
        let gitlink = format!("160000,{},public", base.trim());
        cmd!("git", "update-index", "--add", "--cacheinfo", &gitlink)
            .dir(root)
            .run()
            .unwrap();
        cmd!("git", "commit", "-qm", "add submodule entry")
            .dir(root)
            .run()
            .unwrap();

        assert!(reject_new_scripts(root, "base", |_| false).is_err());
        assert!(reject_new_scripts(root, "base", |path| path == Path::new("new.py")).is_ok());

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let non_utf8 = PathBuf::from(std::ffi::OsString::from_vec(b"bad-\xff.py".to_vec()));
            std::fs::write(root.join(&non_utf8), "print(2)\n").unwrap();
            cmd!("git", "add", "--", &non_utf8).dir(root).run().unwrap();
            cmd!("git", "commit", "-qm", "add non-UTF-8 script")
                .dir(root)
                .run()
                .unwrap();
            assert!(reject_new_scripts(root, "base", |path| path == Path::new("new.py")).is_err());
        }
    }

    #[test]
    fn detects_extensions_and_shebangs() {
        for path in ["tool.sh", "tool.bash", "tool.py", "tool.pyw"] {
            assert!(is_script(Path::new(path), b""));
        }
        for shebang in [
            b"#!/bin/bash\n".as_slice(),
            b"#!/usr/bin/env bash\n",
            b"#!/usr/bin/env -S python3 -u\n",
            b"#!/usr/bin/python3.12\n",
        ] {
            assert!(is_script(Path::new("tool"), shebang));
        }
        assert!(!is_script(Path::new("tool"), b"#!/usr/bin/env node\n"));
        assert!(!is_script(Path::new("tool.rs"), b"fn main() {}"));
    }

    #[test]
    fn parses_additions_edits_deletions_and_moves() {
        let changes = parse_changed_files(b"A\0new.py\0M\0old.sh\0D\0gone.py\0R100\0before.sh\0after.sh\0").unwrap();
        assert_eq!(changes.len(), 4);
        assert!(changes[0].old_path.is_none());
        assert_eq!(changes[0].new_path.as_deref(), Some(Path::new("new.py")));
        assert_eq!(changes[1].old_path.as_deref(), Some(Path::new("old.sh")));
        assert!(changes[2].new_path.is_none());
        assert_eq!(changes[3].old_path.as_deref(), Some(Path::new("before.sh")));
        assert_eq!(changes[3].new_path.as_deref(), Some(Path::new("after.sh")));
    }

    #[test]
    fn detects_new_scripts_but_grandfathers_existing_ones() {
        assert!(is_new_script(None, Path::new("new.py"), b""));
        assert!(is_new_script(
            Some((Path::new("tool"), b"plain text")),
            Path::new("tool"),
            b"#!/bin/bash\n"
        ));
        assert!(is_new_script(
            Some((Path::new("before.txt"), b"plain text")),
            Path::new("after.py"),
            b""
        ));
        assert!(!is_new_script(
            Some((Path::new("before.sh"), b"")),
            Path::new("after.sh"),
            b""
        ));
        assert!(!is_new_script(
            Some((Path::new("tool.py"), b"")),
            Path::new("tool.py"),
            b"updated"
        ));
        assert!(!is_new_script(None, Path::new("tool.rs"), b"fn main() {}"));
    }
}
