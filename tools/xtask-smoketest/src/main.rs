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
        XtaskCmd::Smoketest { cmd: None, args } => run_smoketest(args),
    }
}

fn build_binaries() -> Result<()> {
    eprintln!("Building spacetimedb-cli and spacetimedb-standalone...");

    let mut cmd = Command::new("cargo");
    cmd.args(["build", "-p", "spacetimedb-cli", "-p", "spacetimedb-standalone"]);

    // Remove cargo/rust env vars that could cause fingerprint mismatches
    // when the test later runs cargo build from a different environment
    for (key, _) in env::vars() {
        let should_remove = (key.starts_with("CARGO") && key != "CARGO_HOME" && key != "CARGO_TARGET_DIR")
            || key.starts_with("RUST")
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

fn run_smoketest(args: Vec<String>) -> Result<()> {
    // 1. Build binaries first (single process, no race)
    build_binaries()?;

    // 2. Detect whether to use nextest or cargo test
    let use_nextest = Command::new("cargo")
        .args(["nextest", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    // 3. Run tests with appropriate runner
    let status = if use_nextest {
        eprintln!("Running smoketests with cargo nextest...\n");
        Command::new("cargo")
            .args(["nextest", "run", "-p", "spacetimedb-smoketests"])
            .args(&args)
            .status()?
    } else {
        eprintln!("Running smoketests with cargo test...\n");
        Command::new("cargo")
            .args(["test", "-p", "spacetimedb-smoketests"])
            .args(&args)
            .status()?
    };

    ensure!(status.success(), "Tests failed");
    Ok(())
}
