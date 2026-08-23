#![allow(clippy::disallowed_macros)]
use anyhow::{ensure, Result};
use ci_common::pnpm;
use clap::Parser;
use duct::cmd;
use std::{env, path::PathBuf};

/// Runs tests
///
/// Runs rust tests, codegens csharp sdk and runs csharp tests.
/// This does not include Unreal tests.
/// This expects to run in a clean git state.
#[derive(Parser)]
struct Cli {
    /// Use release CLI and standalone binaries already present in the Cargo target directory.
    #[arg(long)]
    prebuilt_runtime: bool,
}

fn runtime_binary_path(binary_name: &str) -> PathBuf {
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| ci_common::repo_root().join("target"));
    target_dir
        .join("release")
        .join(binary_name)
        .with_extension(env::consts::EXE_EXTENSION)
}

fn verify_prebuilt_runtime() -> Result<()> {
    for binary_name in ["spacetimedb-cli", "spacetimedb-standalone"] {
        let binary_path = runtime_binary_path(binary_name);
        ensure!(
            binary_path.is_file(),
            "--prebuilt-runtime requires {binary_name} at {}",
            binary_path.display()
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.prebuilt_runtime {
        verify_prebuilt_runtime()?;
    }

    pnpm(["build"]).dir("crates/bindings-typescript").run()?;

    // TODO: This doesn't work on at least user Linux machines, because something here apparently uses `sudo`?

    // Exclude smoketests from `cargo test --all` since they require pre-built binaries.
    // Smoketests have their own dedicated command: `cargo ci smoketests`
    cmd!(
        "cargo",
        "test",
        "--all",
        "--exclude",
        "spacetimedb-smoketests",
        "--exclude",
        "spacetimedb-sdk",
        "--exclude",
        "spacetimedb",
        "--",
        "--test-threads=2",
        "--skip",
        "unreal"
    )
    .run()?;
    // Bindings snapshot tests rely on the unstable feature,
    // as they compile and test APIs which are gated behind that feature,
    // e.g. procedures, HTTP handlers.
    cmd!(
        "cargo",
        "test",
        "-p",
        "spacetimedb",
        "--features",
        "unstable",
        "--",
        "--test-threads=2",
    )
    .run()?;
    // The SDK test harness uses the same child-process server guard as smoketests,
    // which expects release CLI/standalone binaries to already exist.
    if !cli.prebuilt_runtime {
        cmd!(
            "cargo",
            "build",
            "--release",
            "-p",
            "spacetimedb-cli",
            "-p",
            "spacetimedb-standalone",
            "--features",
            "spacetimedb-standalone/allow_loopback_http_for_tests",
        )
        .run()?;
    }
    // SDK procedure tests intentionally make localhost HTTP requests.
    cmd!(
        "cargo",
        "test",
        "-p",
        "spacetimedb-sdk",
        "--features",
        "allow_loopback_http_for_tests",
        "--",
        "--test-threads=2",
        "--skip",
        "unreal"
    )
    .run()?;
    // Run the same SDK suite against wasm/browser test clients.
    cmd!(
        "cargo",
        "test",
        "-p",
        "spacetimedb-sdk",
        "--features",
        "allow_loopback_http_for_tests,browser",
        "--",
        "--test-threads=2",
        "--skip",
        "unreal"
    )
    .run()?;
    // TODO: This should check for a diff at the start. If there is one, we should alert the user
    // that we're disabling diff checks because they have a dirty git repo, and to re-run in a clean one
    // if they want those checks.

    // The fallocate tests have been flakely when running in parallel
    cmd!(
        "cargo",
        "test",
        "-p",
        "spacetimedb-durability",
        "--features",
        "fallocate",
        "--",
        "--test-threads=1",
    )
    .run()?;
    cmd!("bash", "tools/check-diff.sh").run()?;
    cmd!(
        "cargo",
        "run",
        "-p",
        "spacetimedb-codegen",
        "--example",
        "regen-csharp-moduledef",
    )
    .run()?;
    cmd!("bash", "tools/check-diff.sh", "crates/bindings-csharp").run()?;
    cmd!("dotnet", "test", "-warnaserror")
        .dir("crates/bindings-csharp")
        .run()?;

    Ok(())
}
