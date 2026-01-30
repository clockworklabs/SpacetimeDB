#![allow(clippy::disallowed_macros)]
use anyhow::{ensure, Result};
use clap::{Parser, Subcommand};
use std::env;
use std::process::{Command, Stdio};

/// SpacetimeDB development tasks
#[derive(Parser)]
#[command(name = "cargo xtask")]
struct Cli {
    #[command(subcommand)]
    cmd: XtaskCmd,
}

#[derive(Subcommand)]
enum XtaskCmd {
    /// Run smoketests with pre-built binaries
    ///
    /// This command first builds the spacetimedb-cli and spacetimedb-standalone binaries,
    /// then runs the smoketests. This prevents race conditions when running tests in parallel
    /// with nextest, where multiple test processes might try to build the same binaries
    /// simultaneously.
    Smoketest {
        #[command(subcommand)]
        cmd: Option<SmoketestCmd>,

        /// Run tests against a remote server instead of spawning local servers.
        ///
        /// When specified, tests will connect to the given URL instead of starting
        /// local server instances. Tests that require local server control (like
        /// restart tests) will be skipped.
        #[arg(long)]
        server: Option<String>,

        /// Additional arguments to pass to the test runner
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum SmoketestCmd {
    /// Only build binaries without running tests
    ///
    /// Use this before running `cargo test --all` to ensure binaries are built.
    Prepare,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        XtaskCmd::Smoketest {
            cmd: Some(SmoketestCmd::Prepare),
            ..
        } => {
            build_binaries()?;
            eprintln!("Binaries ready. You can now run `cargo test --all`.");
            Ok(())
        }
        XtaskCmd::Smoketest {
            cmd: None,
            server,
            args,
        } => run_smoketest(server, args),
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

fn run_smoketest(server: Option<String>, args: Vec<String>) -> Result<()> {
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

    // Set remote server environment variable if specified
    if let Some(ref server_url) = server {
        cmd.env("SPACETIME_REMOTE_SERVER", server_url);
        eprintln!("Running smoketests against remote server {server_url}...\n");
    }

    // 5. Run tests with appropriate runner (release mode for faster execution)
    let status = if use_nextest {
        eprintln!("Running smoketests with cargo nextest...\n");
        let mut cmd = Command::new("cargo");
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
            .any(|a| a == "-j" || a.starts_with("-j") || a.starts_with("--jobs"))
        {
            cmd.args(["-j", DEFAULT_PARALLELISM]);
        }

        cmd.args(&args).status()?
    } else {
        eprintln!("Running smoketests with cargo test...\n");
        let mut cmd = Command::new("cargo");
        cmd.args(["test", "--release", "-p", "spacetimedb-smoketests"]);

        cmd.args(&args).status()?
    };

    ensure!(status.success(), "Tests failed");
    Ok(())
}
