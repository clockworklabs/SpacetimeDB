#![allow(clippy::disallowed_macros)]

use anyhow::{ensure, Result};
use ci_common::pnpm;
use clap::{Parser, Subcommand};
use duct::cmd;
use spacetimedb_testing::sdk::build_precompiled_modules;
use std::env;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(about = "Builds and runs the Rust SDK test suite")]
struct Args {
    #[command(subcommand)]
    command: Option<SdkTestCommand>,
}

#[derive(Clone, Copy)]
enum Mode {
    Native,
    Browser,
}

impl Mode {
    fn features(self) -> &'static str {
        match self {
            Self::Native => "allow_loopback_http_for_tests",
            Self::Browser => "allow_loopback_http_for_tests,browser",
        }
    }
}

#[derive(Subcommand)]
enum SdkTestCommand {
    /// Compile all sdk-test modules without running tests.
    PrepareModules {
        #[arg(long)]
        output_dir: PathBuf,
    },
}

fn main() -> Result<()> {
    ci_common::ensure_repo_root()?;
    match Args::parse().command {
        Some(SdkTestCommand::PrepareModules { output_dir }) => {
            build_typescript_sdk()?;
            let count = build_precompiled_modules(&output_dir)?;
            ensure!(count > 0, "No SDK test modules were found");
            eprintln!("Built {count} precompiled SDK test modules.");
            Ok(())
        }
        None => run_local(),
    }
}

fn ensure_runtime() -> Result<()> {
    if env::var_os("SPACETIME_BIN").is_some() {
        return ci_common::require_runtime();
    }

    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "-p",
            "spacetimedb-cli",
            "-p",
            "spacetimedb-standalone",
            "--features",
            "spacetimedb-standalone/allow_loopback_http_for_tests",
        ])
        .status()?;
    ensure!(status.success(), "Failed to build the SDK test runtime");
    Ok(())
}

fn run_local() -> Result<()> {
    ensure_runtime()?;
    build_typescript_sdk()?;
    for mode in [Mode::Native, Mode::Browser] {
        let status = Command::new("cargo")
            .args([
                "test",
                "-p",
                "spacetimedb-sdk",
                "--features",
                mode.features(),
                "--",
                "--test-threads=2",
                "--skip",
                "unreal",
            ])
            .status()?;
        ensure!(status.success(), "SDK tests failed");
        cmd!("bash", "tools/check-diff.sh").run()?;
    }
    Ok(())
}

fn build_typescript_sdk() -> Result<()> {
    pnpm(["build"]).dir("crates/bindings-typescript").run()?;
    Ok(())
}
