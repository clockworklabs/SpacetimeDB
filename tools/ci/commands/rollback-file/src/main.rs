#![allow(clippy::disallowed_macros)]

use anyhow::{Context, Result};
use clap::Parser;
use rollback_coordination::{rollback_point_for_repo, write_or_check_rollback_point, Gh, Release};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(about = "Generates or checks the repository's earliest allowed rollback point.")]
struct Args {
    #[arg(long, default_value = ".")]
    repo: PathBuf,
    #[arg(long)]
    check: bool,
}

fn target_release(repo: &Path) -> Result<Release> {
    let manifest_path = repo.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?
        .parse::<toml::Table>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let version = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .context("workspace.package.version is missing from Cargo.toml")?;
    Release::from_tag(&format!("v{version}"))?.context("workspace package version is not a compatible release")
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let args = Args::parse();
    let target = target_release(&args.repo)?;
    let point = rollback_point_for_repo(&Gh, &args.repo, &[&args.repo], &target, false, &[])?;
    write_or_check_rollback_point(&args.repo, &point, args.check)?;
    println!("Earliest allowed rollback point: {point}");
    Ok(())
}
