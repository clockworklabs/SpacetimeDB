#![allow(clippy::disallowed_macros)]
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use replace_spacetimedb::{replace_in_tree, ReplaceOptions};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Parser, Debug)]
#[command(version, about = "Build + generate TS bindings + replace + prettier")]
struct Cli {
    /// Output directory for generated code
    #[arg(long, default_value = "src/module_bindings")]
    out_dir: String,

    /// Module path passed to spacetimedb-cli
    #[arg(long, alias = "project-path", default_value = "server")]
    module_path: String,

    /// Replacement for 'spacetimedb' (relative string used in imports)
    #[arg(long)]
    replacement: Option<String>,

    #[arg(long)]
    index_replacement: Option<String>,
}

fn run_inherit(cmd: impl AsRef<OsStr>, args: &[&str], cwd: Option<&Path>) -> Result<()> {
    let cmd = cmd.as_ref();
    let mut c = Command::new(cmd);
    if let Some(cwd) = cwd {
        c.current_dir(cwd);
    }
    let status = c
        .args(args)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("Failed to start {cmd:?}"))?;
    if !status.success() {
        return Err(anyhow!("Command failed: {cmd:?} {args:?} (exit {status})"));
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Cli::parse();

    let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    // 1) Build prerequisite
    run_inherit("cargo", &["build"], Some(workspace_dir))?;

    // 2) Ensure output directory exists
    if !Path::new(&args.out_dir).exists() {
        fs::create_dir_all(&args.out_dir).context("create output directory")?;
    }

    // 3) Generate TS client from project
    run_inherit(
        workspace_dir
            .join("target/debug/spacetimedb-cli")
            .with_extension(std::env::consts::EXE_EXTENSION),
        &[
            "generate",
            "-y",
            "--lang",
            "typescript",
            "--out-dir",
            &args.out_dir,
            "--module-path",
            &args.module_path,
        ],
        None,
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
            ignore_globs: vec!["**/node_modules/**".into(), "**/dist/**".into(), "**/target/**".into()],
        };

        let stats = replace_in_tree(&args.out_dir, index_replacement, other_replacement, &opts)?;
        println!(
            "Replaced 'spacetimedb' → '{}' in {} files ({} occurrences).",
            other_replacement, stats.files_changed, stats.occurrences
        );
    }
    Ok(())
}
