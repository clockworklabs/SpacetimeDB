#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use ci_args::global_json_policy::Args;
use ci_common::ensure_repo_root;
use clap::Parser;
use duct::cmd;
use std::fs;
use std::path::{Path, PathBuf};

/// Verify that any non-root global.json files are symlinks to the root global.json.
#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    args: Args,
}

fn git_tracked_files(pathspec: &str) -> Result<Vec<PathBuf>> {
    let output = cmd!("git", "ls-files", pathspec).read()?;
    Ok(output.lines().map(PathBuf::from).collect())
}

fn main() -> Result<()> {
    Cli::parse();

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
