#![allow(clippy::disallowed_macros)]
use anyhow::{ensure, Context, Result};
use ci_common::pnpm;
use clap::{Parser, Subcommand};
use duct::{cmd, Expression};
use std::fs;
use std::path::{Path, PathBuf};

const SDK_TEST_MODE_ENV: &str = "SPACETIME_SDK_TEST_MODE";
const SDK_TEST_ARTIFACT_DIR_ENV: &str = "SPACETIME_SDK_TEST_ARTIFACT_DIR";

/// Runs the public test suite locally or prepares its CI nextest archives.
///
/// With no subcommand, this preserves the all-in-one local test workflow.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// CI build job: run unsharded checks and archive the core Rust test binaries.
    ArchiveCore {
        #[arg(long)]
        archive_file: PathBuf,
        #[arg(long)]
        unstable_archive_file: PathBuf,
    },
    /// CI build job: prepare SDK modules/clients and archive native and browser tests.
    ArchiveSdk {
        #[arg(long)]
        native_archive_file: PathBuf,
        #[arg(long)]
        browser_archive_file: PathBuf,
        #[arg(long)]
        support_archive_file: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        None => run_all_locally(),
        Some(Command::ArchiveCore {
            archive_file,
            unstable_archive_file,
        }) => archive_core(&archive_file, &unstable_archive_file),
        Some(Command::ArchiveSdk {
            native_archive_file,
            browser_archive_file,
            support_archive_file,
        }) => archive_sdk(&native_archive_file, &browser_archive_file, &support_archive_file),
    }
}

fn run(expression: Expression, failure: &str) -> Result<()> {
    let output = expression.unchecked().run()?;
    ensure!(output.status.success(), "{failure}");
    Ok(())
}

fn run_typescript_bindings_build() -> Result<()> {
    pnpm(["build"]).dir("crates/bindings-typescript").run()?;
    Ok(())
}

fn run_core_cargo_tests() -> Result<()> {
    run(
        cmd!(
            "cargo",
            "test",
            "--workspace",
            "--exclude",
            "spacetimedb-smoketests",
            "--exclude",
            "spacetimedb-sdk",
            "--exclude",
            "spacetimedb",
            "--",
            "--test-threads=2",
            "--skip",
            "unreal",
        ),
        "workspace tests failed",
    )?;
    run(
        cmd!(
            "cargo",
            "test",
            "-p",
            "spacetimedb",
            "--features",
            "unstable",
            "--",
            "--test-threads=2",
        ),
        "unstable spacetimedb tests failed",
    )
}

fn run_sdk_tests_locally() -> Result<()> {
    run(
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
        ),
        "SDK support binary build failed",
    )?;
    run_sdk_test_command(false, None, false)?;
    run_sdk_test_command(true, None, false)
}

fn run_sdk_test_command(browser: bool, artifact_dir: Option<&Path>, prepare: bool) -> Result<()> {
    let features = if browser {
        "allow_loopback_http_for_tests,browser"
    } else {
        "allow_loopback_http_for_tests"
    };
    let mut args = vec!["test", "-p", "spacetimedb-sdk", "--features", features];
    if prepare {
        args.extend(["--test", "test"]);
    }
    args.extend([
        "--",
        if prepare {
            "--test-threads=1"
        } else {
            "--test-threads=2"
        },
        "--skip",
        "unreal",
    ]);
    if prepare {
        // These failure-path tests share their module and client artifacts with ordinary
        // tests. Do not execute them during preparation: libtest would otherwise reject
        // their intentionally successful prepare-only path because they are `should_panic`.
        args.extend(["--skip", "should_fail", "--skip", "subscribe_all_select_star"]);
    }
    let mut command = cmd("cargo", args);
    if let Some(artifact_dir) = artifact_dir {
        command = command
            .env(SDK_TEST_MODE_ENV, "prepare")
            .env(SDK_TEST_ARTIFACT_DIR_ENV, artifact_dir);
    }
    run(command, "SDK tests failed")
}

fn run_durability_fallocate_tests() -> Result<()> {
    run(
        cmd!(
            "cargo",
            "test",
            "-p",
            "spacetimedb-durability",
            "--features",
            "fallocate",
            "--",
            "--test-threads=1",
        ),
        "durability fallocate tests failed",
    )
}

fn run_csharp_bindings_checks() -> Result<()> {
    run(
        cmd!("bash", "tools/check-diff.sh"),
        "repository has changes before C# regeneration",
    )?;
    run(
        cmd!(
            "cargo",
            "run",
            "-p",
            "spacetimedb-codegen",
            "--example",
            "regen-csharp-moduledef",
        ),
        "C# module definition regeneration failed",
    )?;
    run(
        cmd!("bash", "tools/check-diff.sh", "crates/bindings-csharp"),
        "generated C# bindings are out of date",
    )?;
    run(
        cmd!("dotnet", "test", "-warnaserror").dir("crates/bindings-csharp"),
        "C# bindings tests failed",
    )
}

fn run_all_locally() -> Result<()> {
    run_typescript_bindings_build()?;
    // TODO: This doesn't work on at least user Linux machines, because something here apparently uses `sudo`?
    run_core_cargo_tests()?;
    run_sdk_tests_locally()?;
    run_durability_fallocate_tests()?;
    run_csharp_bindings_checks()
}

fn archive_core(archive_file: &Path, unstable_archive_file: &Path) -> Result<()> {
    run_typescript_bindings_build()?;
    run(
        cmd!(
            "cargo",
            "nextest",
            "archive",
            "--timings",
            "--workspace",
            "--exclude",
            "spacetimedb-smoketests",
            "--exclude",
            "spacetimedb-sdk",
            "--exclude",
            "spacetimedb",
            "--archive-file",
            archive_file,
        ),
        "failed to archive workspace tests",
    )?;
    run(
        cmd!(
            "cargo",
            "nextest",
            "archive",
            "--timings",
            "-p",
            "spacetimedb",
            "--features",
            "unstable",
            "--archive-file",
            unstable_archive_file,
        ),
        "failed to archive unstable spacetimedb tests",
    )?;

    // Nextest does not support doctests, so preserve the current doctest coverage here.
    run(
        cmd!(
            "cargo",
            "test",
            "--workspace",
            "--exclude",
            "spacetimedb-smoketests",
            "--exclude",
            "spacetimedb-sdk",
            "--exclude",
            "spacetimedb",
            "--doc",
            "--",
            "--test-threads=2",
            "--skip",
            "unreal",
        ),
        "workspace doctests failed",
    )?;
    run(
        cmd!(
            "cargo",
            "test",
            "-p",
            "spacetimedb",
            "--features",
            "unstable",
            "--doc",
            "--",
            "--test-threads=2",
        ),
        "unstable spacetimedb doctests failed",
    )?;
    run_durability_fallocate_tests()?;
    run_csharp_bindings_checks()
}

fn archive_sdk_tests(browser: bool, archive_file: &Path) -> Result<()> {
    let features = if browser {
        "allow_loopback_http_for_tests,browser"
    } else {
        "allow_loopback_http_for_tests"
    };
    run(
        cmd!(
            "cargo",
            "nextest",
            "archive",
            "--timings",
            "-p",
            "spacetimedb-sdk",
            "--features",
            features,
            "--archive-file",
            archive_file,
        ),
        "failed to archive SDK tests",
    )
}

fn run_sdk_doctests(browser: bool) -> Result<()> {
    let features = if browser {
        "allow_loopback_http_for_tests,browser"
    } else {
        "allow_loopback_http_for_tests"
    };
    run(
        cmd!(
            "cargo",
            "test",
            "-p",
            "spacetimedb-sdk",
            "--features",
            features,
            "--doc",
        ),
        "SDK doctests failed",
    )
}

fn archive_sdk(native_archive_file: &Path, browser_archive_file: &Path, support_archive_file: &Path) -> Result<()> {
    let artifact_dir = PathBuf::from("target/sdk-test-support");
    if artifact_dir.exists() {
        fs::remove_dir_all(&artifact_dir).context("failed to clear old SDK support artifacts")?;
    }

    // Libtest runs preparation serially so the harness's in-process memoization
    // builds each unique module and client once per feature mode.
    run_sdk_test_command(false, Some(&artifact_dir), true)?;
    run_sdk_test_command(true, Some(&artifact_dir), true)?;
    archive_sdk_tests(false, native_archive_file)?;
    archive_sdk_tests(true, browser_archive_file)?;
    run_sdk_doctests(false)?;
    run_sdk_doctests(true)?;
    run(
        cmd!("bash", "tools/check-diff.sh"),
        "SDK binding generation changed tracked files",
    )?;

    let artifact_parent = artifact_dir.parent().context("SDK support directory has no parent")?;
    let artifact_name = artifact_dir
        .file_name()
        .context("SDK support directory has no filename")?;
    run(
        cmd!(
            "tar",
            "-czf",
            support_archive_file,
            "-C",
            artifact_parent,
            artifact_name
        ),
        "failed to package SDK support artifacts",
    )
}
