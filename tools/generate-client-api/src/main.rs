#![allow(clippy::disallowed_macros)]
use anyhow::{anyhow, Context, Result};
use replace_spacetimedb::{replace_in_tree, ReplaceOptions};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::NamedTempFile;

/// Run a command inheriting stdio; error if it fails.
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

/// Run a command and return captured stdout as UTF-8 string.
fn run_capture(cmd: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("Failed to start {cmd}"))?;
    if !out.status.success() {
        return Err(anyhow!(
            "Command failed: {cmd} {args:?} (exit {})",
            out.status
        ));
    }
    Ok(String::from_utf8(out.stdout)?)
}

fn main() -> Result<()> {
    let out_dir = "src/sdk/client_api";
    let index_replacement = "../../index";
    let other_replacement = "../../lib/type_builders";

    // 1) Build prerequisite
    run_inherit("cargo", &["build", "-p", "spacetimedb-standalone"])?;

    // 2) Get schema to a temp file (auto-cleaned)
    let mut tmp_schema = NamedTempFile::new().context("create temp schema file")?;
    let schema_json = run_capture(
        "cargo",
        &[
            "run",
            "-p",
            "spacetimedb-client-api-messages",
            "--example",
            "get_ws_schema",
        ],
    )?;
    use std::io::Write;
    tmp_schema.write_all(schema_json.as_bytes())?;
    let schema_path = tmp_schema.path();

    // 3) Ensure output directory exists
    if !Path::new(out_dir).exists() {
        fs::create_dir_all(out_dir).context("create output directory")?;
    }

    // 4) Generate TS client
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
            out_dir,
            "--module-def",
            schema_path.to_str().unwrap(),
        ],
    )?;

    // 5) Replace "spacetimedb" references under out_dir
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
        ignore_globs: vec!["**/node_modules/**".into(), "**/dist/**".into()],
    };
    let stats = replace_in_tree(out_dir, index_replacement, other_replacement, &opts)?;
    println!(
        "Replaced {} occurrences across {} files.",
        stats.occurrences, stats.files_changed
    );

    Ok(())
}
