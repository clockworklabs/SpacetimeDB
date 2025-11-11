#![allow(clippy::disallowed_macros)]
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use replace_spacetimedb::{replace_in_tree, ReplaceOptions};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Parser, Debug)]
#[command(version, about = "Build + generate TS bindings + replace + prettier")]
struct Cli {
    /// Output directory for generated code
    #[arg(long, default_value = "src/module_bindings")]
    out_dir: String,

    /// Project path passed to spacetimedb-cli
    #[arg(long, default_value = "server")]
    project_path: String,

    /// Replacement for 'spacetimedb' (relative string used in imports)
    #[arg(long)]
    replacement: Option<String>,

    #[arg(long)]
    index_replacement: Option<String>,
}

fn run_inherit(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("Failed to start {cmd}"))?;
    if !status.success() {
        return Err(anyhow!("Command failed: {cmd} {args:?} (exit {status})"));
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Cli::parse();

    // 1) Build prerequisite
    run_inherit("cargo", &["build", "-p", "spacetimedb-standalone"])?;

    // 2) Ensure output directory exists
    if !Path::new(&args.out_dir).exists() {
        fs::create_dir_all(&args.out_dir).context("create output directory")?;
    }

    // 3) Generate TS client from project
    run_inherit(
        "cargo",
        &[
            "run",
            "-p",
            "spacetimedb-cli",
            "generate",
            "--lang",
            "typescript",
            "--out-dir",
            &args.out_dir,
            "--project-path",
            &args.project_path,
        ],
    )?;

    if let Some(other_replacement) = &args.replacement {
        let index_replacement = if let Some(index_replacement) = &args.index_replacement {
            index_replacement
        } else {
            other_replacement 
        };

        // 5) Replace "spacetimedb" references
        let opts = ReplaceOptions {
            dry_run: false,
            only_exts: Some(vec![
                "ts".into(),
                "tsx".into(),
                "js".into(),
                "jsx".into(),
                "mts".into(),
                "cts".into(),
                "json".into(),
                "d.ts".into(),
            ]),
            follow_symlinks: false,
            include_hidden: false,
            ignore_globs: vec![
                "**/node_modules/**".into(),
                "**/dist/**".into(),
                "**/target/**".into(),
            ],
        };

        let stats = replace_in_tree(&args.out_dir, index_replacement, other_replacement, &opts)?;
        println!(
            "Replaced 'spacetimedb' → '{}' in {} files ({} occurrences).",
            other_replacement, stats.files_changed, stats.occurrences
        );
    }
    Ok(())
}
