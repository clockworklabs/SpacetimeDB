#![allow(clippy::disallowed_macros)]
use anyhow::{bail, ensure, Result};
use clap::{Args, Subcommand};
use duct::cmd;
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::{env, fs};

use crate::util;

#[derive(Args)]
/// This command first builds the spacetimedb-cli and spacetimedb-standalone binaries,
/// then runs the smoketests. This prevents race conditions when running tests in parallel
/// with nextest, where multiple test processes might try to build the same binaries
/// simultaneously.
pub struct SmoketestsArgs {
    #[command(subcommand)]
    cmd: Option<SmoketestCmd>,

    /// Run tests against a remote server instead of spawning local servers.
    ///
    /// When specified, tests will connect to the given URL instead of starting
    /// local server instances. Tests that require local server control (like
    /// restart tests) will be skipped.
    #[arg(long)]
    server: Option<String>,

    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    dotnet: bool,

    /// Additional arguments to pass to the test runner
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

#[derive(Subcommand)]
enum SmoketestCmd {
    /// Only build binaries without running tests
    ///
    /// Use this before running `cargo test --all` to ensure binaries are built.
    Prepare,
    CheckModList,
}

pub fn run(args: SmoketestsArgs) -> Result<()> {
    match args.cmd {
        Some(SmoketestCmd::Prepare) => {
            build_binaries()?;
            eprintln!("Binaries ready. You can now run `cargo test --all`.");
            Ok(())
        }
        Some(SmoketestCmd::CheckModList) => {
            check_smoketests_mod_rs_complete()?;
            eprintln!("smoketests/mod.rs is up to date.");
            Ok(())
        }
        None => run_smoketest(args.server, args.dotnet, args.args),
    }
}

fn build_binaries() -> Result<()> {
    eprintln!("Building spacetimedb-cli and spacetimedb-standalone (release)...");

    let mut cmd = Command::new("cargo");
    cmd.args([
        "build",
        "--release",
        "-p",
        "spacetimedb-cli",
        "-p",
        "spacetimedb-standalone",
        "--features",
        "spacetimedb-standalone/allow_loopback_http_for_tests",
    ]);

    // Remove cargo/rust env vars that could cause fingerprint mismatches
    // when the test later runs cargo build from a different environment
    for (key, _) in env::vars() {
        let should_remove = (key.starts_with("CARGO") && key != "CARGO_HOME" && key != "CARGO_TARGET_DIR")
            || key.starts_with("RUST")
            // > The environment variable `__CARGO_FIX_YOLO` is an undocumented, internal-use-only feature
            // > for the Rust cargo fix command (and cargo clippy --fix) that forces the application of all
            // > available suggestions, including those that are marked as potentially incorrect or dangerous.
            || key == "__CARGO_FIX_YOLO";
        if should_remove {
            cmd.env_remove(&key);
        }
    }

    let status = cmd.status()?;
    ensure!(status.success(), "Failed to build binaries");
    eprintln!("Binaries built successfully.\n");
    Ok(())
}

fn build_precompiled_modules() -> Result<()> {
    let workspace_root = env::current_dir()?;
    let modules_dir = workspace_root.join("crates/smoketests/modules");

    // Check if the modules workspace exists
    if !modules_dir.join("Cargo.toml").exists() {
        eprintln!("Skipping pre-compiled modules (workspace not found).\n");
        return Ok(());
    }

    eprintln!("Building pre-compiled smoketest modules...");

    let status = Command::new("cargo")
        .args([
            "build",
            "--workspace",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .current_dir(&modules_dir)
        .status()?;

    ensure!(status.success(), "Failed to build pre-compiled modules");
    eprintln!("Pre-compiled modules built.\n");
    Ok(())
}

/// Default parallelism for smoketests.
/// 16 was found to be optimal - higher values cause OS scheduler overhead.
const DEFAULT_PARALLELISM: &str = "16";

fn run_smoketest(server: Option<String>, dotnet: bool, args: Vec<String>) -> Result<()> {
    // 1. Build binaries first (single process, no race)
    build_binaries()?;

    // 2. Build pre-compiled modules (this also warms the WASM dependency cache)
    build_precompiled_modules()?;

    // 4. Detect whether to use nextest or cargo test
    let use_nextest = Command::new("cargo")
        .args(["nextest", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if let Some(ref server_url) = server {
        eprintln!("Running smoketests against remote server {server_url}...\n");
    }

    // 5. Run tests with appropriate runner (release mode for faster execution)
    let mut cmd = if use_nextest {
        eprintln!("Running smoketests with cargo nextest...\n");
        let mut cmd = Command::new("cargo");
        set_env(&mut cmd, server, dotnet);
        cmd.args([
            "nextest",
            "run",
            "--release",
            "-p",
            "spacetimedb-smoketests",
            "--no-fail-fast",
        ]);

        // Set default parallelism if user didn't specify -j
        if !args
            .iter()
            .any(|a| a.starts_with("-j") || a.starts_with("--jobs") || a.starts_with("--test-threads"))
        {
            cmd.args(["-j", DEFAULT_PARALLELISM]);
        }

        cmd
    } else {
        eprintln!("Running smoketests with cargo test...\n");
        let mut cmd = Command::new("cargo");
        set_env(&mut cmd, server, dotnet);
        cmd.args(["test", "--release", "-p", "spacetimedb-smoketests"]);
        cmd
    };
    let status = cmd.args(&args).status()?;

    ensure!(status.success(), "Tests failed");
    Ok(())
}

fn set_env(cmd: &mut Command, server: Option<String>, dotnet: bool) {
    if let Some(ref server_url) = server {
        cmd.env("SPACETIME_REMOTE_SERVER", server_url);
    }
    cmd.env("SMOKETESTS_DOTNET", if dotnet { "1" } else { "0" });
}

fn check_smoketests_mod_rs_complete() -> Result<()> {
    util::ensure_repo_root()?;

    let expected_dir = Path::new("crates/smoketests/tests/smoketests");
    let mut expected = std::collections::BTreeSet::<String>::new();
    for entry in fs::read_dir(expected_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "mod.rs" {
            continue;
        }
        if name.starts_with('.') {
            continue;
        }

        let ft = entry.file_type()?;
        if ft.is_dir() {
            expected.insert(name.to_string());
        } else if ft.is_file() && path.extension() == Some(OsStr::new("rs")) {
            if let Some(stem) = path.file_stem() {
                expected.insert(stem.to_string_lossy().to_string());
            }
        }
    }

    let out = cmd!("cargo", "test", "-p", "spacetimedb-smoketests", "--", "--list",).read()?;

    let mut present = std::collections::BTreeSet::<String>::new();
    for line in out.lines() {
        let line = line.trim();
        let parts: Vec<&str> = line.split("::").collect();
        if parts.len() < 2 {
            continue;
        }
        if parts[0] != "smoketests" {
            continue;
        }
        present.insert(parts[1].to_string());
    }

    let missing = expected
        .into_iter()
        .filter(|m| !present.contains(m))
        .collect::<Vec<_>>();

    if !missing.is_empty() {
        bail!(
            "crates/smoketests/tests/smoketests/mod.rs appears incomplete; missing modules (not present in `cargo test -- --list`):\n{}",
            missing
                .iter()
                .map(|m| format!("- mod {m};"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    Ok(())
}
