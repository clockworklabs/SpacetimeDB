use anyhow::{ensure, Result};
use clap::{Parser, Subcommand};
use std::env;
use std::fs;
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

fn warmup_wasm_cache() -> Result<()> {
    eprintln!("Warming WASM dependency cache...");

    let workspace_root = env::current_dir()?;
    let target_dir = workspace_root.join("target/smoketest-modules");
    fs::create_dir_all(&target_dir)?;

    let temp_dir = tempfile::tempdir()?;

    // Write minimal Cargo.toml that depends on spacetimedb bindings
    let bindings_path = workspace_root.join("crates/bindings");
    let cargo_toml = format!(
        r#"[package]
name = "warmup"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
spacetimedb = {{ path = "{}" }}
"#,
        bindings_path.display().to_string().replace('\\', "/")
    );
    fs::write(temp_dir.path().join("Cargo.toml"), cargo_toml)?;

    // Copy rust-toolchain.toml if it exists
    let toolchain_src = workspace_root.join("rust-toolchain.toml");
    if toolchain_src.exists() {
        fs::copy(&toolchain_src, temp_dir.path().join("rust-toolchain.toml"))?;
    }

    // Write minimal lib.rs
    fs::create_dir_all(temp_dir.path().join("src"))?;
    fs::write(temp_dir.path().join("src/lib.rs"), "")?;

    // Build to warm the cache
    let status = Command::new("cargo")
        .args([
            "build",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ])
        .env("CARGO_TARGET_DIR", &target_dir)
        .current_dir(temp_dir.path())
        .status()?;

    ensure!(status.success(), "Failed to warm WASM cache");
    eprintln!("WASM cache warmed.\n");
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
/// Limited to avoid cargo build lock contention when many tests run simultaneously.
const DEFAULT_PARALLELISM: &str = "8";

fn run_smoketest(args: Vec<String>) -> Result<()> {
    // 1. Build binaries first (single process, no race)
    build_binaries()?;

    // 2. Warm the WASM dependency cache (single process, no race)
    warmup_wasm_cache()?;

    // 3. Build pre-compiled modules (if available)
    build_precompiled_modules()?;

    // 4. Detect whether to use nextest or cargo test
    let use_nextest = Command::new("cargo")
        .args(["nextest", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    // 5. Run tests with appropriate runner
    let status = if use_nextest {
        eprintln!("Running smoketests with cargo nextest...\n");
        let mut cmd = Command::new("cargo");
        cmd.args(["nextest", "run", "-p", "spacetimedb-smoketests", "--no-fail-fast"]);

        // Set default parallelism if user didn't specify -j
        if !args.iter().any(|a| a == "-j" || a.starts_with("-j") || a.starts_with("--jobs")) {
            cmd.args(["-j", DEFAULT_PARALLELISM]);
        }

        cmd.args(&args).status()?
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
