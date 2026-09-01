#![allow(clippy::disallowed_macros)]

use anyhow::{ensure, Context, Result};
use ci_common::pnpm;
use clap::{Parser, Subcommand, ValueEnum};
use duct::cmd;
use spacetimedb_testing::sdk::{
    build_precompiled_modules, PRECOMPILED_MODULE_DIR_ENV_VAR, PREPARED_CLIENT_DIR_ENV_VAR, PREPARE_CLIENT_DIR_ENV_VAR,
    TEST_WORKSPACE_ROOT_ENV_VAR,
};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(about = "Builds and runs the Rust SDK test suite")]
struct Args {
    #[command(subcommand)]
    command: Option<SdkTestCommand>,
}

#[derive(Clone, Copy, ValueEnum)]
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
    /// Generate, compile, and export one artifact for each shared SDK test client.
    PrepareClients {
        #[arg(long)]
        mode: Mode,
        #[arg(long)]
        module_dir: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
    },
    /// Compile the SDK test binary into a nextest archive.
    Archive {
        #[arg(long)]
        mode: Mode,
        #[arg(long)]
        archive_file: PathBuf,
    },
    /// Run a partition from an existing nextest archive.
    RunArchive {
        #[arg(long)]
        archive_file: PathBuf,
        #[arg(long)]
        module_dir: PathBuf,
        #[arg(long)]
        client_dir: PathBuf,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

fn main() -> Result<()> {
    ci_common::ensure_repo_root()?;
    match Args::parse().command {
        Some(SdkTestCommand::PrepareModules { output_dir }) => {
            let count = build_precompiled_modules(&output_dir)?;
            ensure!(count > 0, "No SDK test modules were found");
            eprintln!("Built {count} precompiled SDK test modules.");
            Ok(())
        }
        Some(SdkTestCommand::Archive { mode, archive_file }) => archive(mode, &archive_file),
        Some(SdkTestCommand::PrepareClients {
            mode,
            module_dir,
            output_dir,
        }) => prepare_clients(mode, &module_dir, &output_dir),
        Some(SdkTestCommand::RunArchive {
            archive_file,
            module_dir,
            client_dir,
            args,
        }) => run_archive(&archive_file, &module_dir, &client_dir, args),
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
    pnpm(["build"]).dir("crates/bindings-typescript").run()?;
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
    }
    Ok(())
}

fn archive(mode: Mode, archive_file: &Path) -> Result<()> {
    let status = Command::new("cargo")
        .args([
            "nextest",
            "archive",
            "--timings",
            "-p",
            "spacetimedb-sdk",
            "--features",
            mode.features(),
            "--archive-file",
        ])
        .arg(archive_file)
        .status()?;
    ensure!(status.success(), "Failed to archive SDK tests");
    Ok(())
}

fn prepare_clients(mode: Mode, module_dir: &Path, output_dir: &Path) -> Result<()> {
    let workspace_root = env::current_dir()?;
    let module_dir = absolute_from_workspace(module_dir)?;
    let output_dir = absolute_from_workspace(output_dir)?;
    ensure!(
        module_dir.is_dir(),
        "SDK module directory does not exist: {}",
        module_dir.display()
    );
    std::fs::create_dir_all(&output_dir)?;

    let status = Command::new("cargo")
        .args([
            "test",
            "--timings",
            "-p",
            "spacetimedb-sdk",
            "--features",
            mode.features(),
            "--test",
            "test",
            "prepare_clients",
            "--",
            "--ignored",
            "--exact",
            "--test-threads=1",
        ])
        .env(PRECOMPILED_MODULE_DIR_ENV_VAR, module_dir)
        .env(PREPARE_CLIENT_DIR_ENV_VAR, output_dir)
        .env(TEST_WORKSPACE_ROOT_ENV_VAR, workspace_root)
        .status()?;
    ensure!(status.success(), "Failed to prepare SDK test clients");
    Ok(())
}

fn absolute_from_workspace(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn run_archive(archive_file: &Path, module_dir: &Path, client_dir: &Path, args: Vec<String>) -> Result<()> {
    ci_common::require_runtime()?;
    let workspace_root = env::current_dir()?;
    let archive_file = absolute_from_workspace(archive_file)?;
    let module_dir = absolute_from_workspace(module_dir)?;
    let client_dir = absolute_from_workspace(client_dir)?;
    ensure!(
        module_dir.is_dir(),
        "SDK module directory does not exist: {}",
        module_dir.display()
    );
    ensure!(
        client_dir.is_dir(),
        "Prepared SDK client directory does not exist: {}",
        client_dir.display()
    );

    let status = Command::new("cargo")
        .args(["nextest", "run", "--archive-file"])
        .arg(archive_file)
        .arg("--workspace-remap")
        .arg(&workspace_root)
        .args(["--no-fail-fast", "--no-tests", "pass", "-j", "1"])
        .args(args)
        .env(PRECOMPILED_MODULE_DIR_ENV_VAR, module_dir)
        .env(PREPARED_CLIENT_DIR_ENV_VAR, client_dir)
        .env(TEST_WORKSPACE_ROOT_ENV_VAR, workspace_root)
        .status()
        .context("Failed to start cargo nextest")?;
    ensure!(status.success(), "SDK tests failed");

    cmd!("bash", "tools/check-diff.sh").run()?;
    Ok(())
}
