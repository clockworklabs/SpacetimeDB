#![allow(clippy::disallowed_macros)]
use anyhow::{Context, Result};
use ci_common::{ensure_repo_root, pnpm};
use clap::Parser;
use duct::cmd;
use std::ffi::OsString;
use std::path::PathBuf;

/// Formats the codebase
///
/// Runs rustfmt, csharpier, and the TypeScript/JS formatter (`pnpm format`) in
/// write mode, so a single `cargo fmt` fixes formatting everywhere in the repo.
/// This mirrors `cargo ci lint`'s checks, but writes fixes instead of only
/// checking for them.
#[derive(Parser)]
struct Cli {}

// NOTE: duplicated from `ci-lint`'s `tracked_rs_files_under`. `cargo fmt --all`
// only checks files that Cargo discovers through workspace/package targets,
// but we also keep Rust sources in locations that are tracked but not part of
// our workspace, so we enumerate tracked files directly instead, exactly like
// `ci-lint` does for its `--check` pass. If this feels worth deduplicating,
// it could move to `ci-common`.
fn tracked_rs_files_under(path: &str) -> Result<Vec<PathBuf>> {
    let output = cmd!("git", "ls-files", "--", path)
        .read()
        .with_context(|| format!("failed to list tracked files under {path}"))?;
    Ok(output
        .lines()
        .filter(|line| line.ends_with(".rs"))
        .map(PathBuf::from)
        .collect())
}

fn main() -> Result<()> {
    Cli::parse();
    ensure_repo_root()?;

    // Format Rust files.
    let files = tracked_rs_files_under(".")?;
    const RUSTFMT_BATCH_SIZE: usize = 200;
    for batch in files.chunks(RUSTFMT_BATCH_SIZE) {
        let mut args = Vec::<OsString>::with_capacity(batch.len());
        args.extend(batch.iter().map(|path| path.as_os_str().to_os_string()));
        cmd("rustfmt", args)
            .run()
            .context("failed to run rustfmt")?;
    }

    // Format C# files.
    cmd!("dotnet", "tool", "restore")
        .dir("crates/bindings-csharp")
        .run()
        .context("failed to run `dotnet tool restore` in crates/bindings-csharp")?;
    cmd!("dotnet", "csharpier", ".")
        .dir("crates/bindings-csharp")
        .run()
        .context("failed to run `dotnet csharpier .` in crates/bindings-csharp")?;

    // Format TypeScript/JS files. This script already exists in package.json
    // and is what `pnpm format` runs today.
    pnpm(["format"])
        .run()
        .context("failed to run `pnpm format`")?;

    Ok(())
}
