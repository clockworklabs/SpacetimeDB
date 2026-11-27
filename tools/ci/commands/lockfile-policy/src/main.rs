#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use ci_common::ensure_repo_root;
use clap::Parser;
use duct::cmd;

/// Verify that no npm lockfiles are committed. JavaScript packages in this repository are
/// installed with pnpm, so a `package-lock.json` is never read and only goes stale, while
/// Dependabot still reports every vulnerability it pins.
#[derive(Parser)]
struct Cli {}

/// Returns the tracked paths that are npm lockfiles.
fn npm_lockfiles(tracked: &str) -> Vec<&str> {
    tracked
        .lines()
        .filter(|path| path.rsplit('/').next() == Some("package-lock.json"))
        .collect()
}

fn main() -> Result<()> {
    Cli::parse();

    ensure_repo_root()?;

    let tracked = cmd!("git", "ls-files").read()?;
    let lockfiles = npm_lockfiles(&tracked);
    if lockfiles.is_empty() {
        println!("OK: no package-lock.json files are committed");
        return Ok(());
    }

    for path in &lockfiles {
        eprintln!("Error: {path} is an npm lockfile");
    }
    eprintln!(
        "This repository uses pnpm. Delete these files, and if the package is not already in the root \
         workspace, add it to pnpm-workspace.yaml (or give it its own pnpm-workspace.yaml) and run `pnpm install`."
    );
    bail!("lockfile policy check failed");
}

#[cfg(test)]
mod tests {
    use super::npm_lockfiles;

    #[test]
    fn finds_npm_lockfiles_at_any_depth() {
        let tracked = "package-lock.json\ntemplates/a/package-lock.json\npnpm-lock.yaml\ntemplates/a/package.json\n";
        assert_eq!(
            npm_lockfiles(tracked),
            ["package-lock.json", "templates/a/package-lock.json"]
        );
    }

    #[test]
    fn ignores_names_that_only_contain_package_lock() {
        let tracked = "docs/package-lock.json.md\nold-package-lock.json\nscripts/package-lock.json/README\n";
        assert!(npm_lockfiles(tracked).is_empty());
    }
}
