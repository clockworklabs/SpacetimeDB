#![allow(clippy::disallowed_macros)]
use anyhow::{bail, ensure, Context, Result};
use clap::{Args, Subcommand};
use duct::cmd;
use spacetimedb_guard::ensure_binaries_built;
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::{env, fs};
use tempfile::TempDir;

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

    /// Use a SpacetimeAuth-issued login for remote-server tests.
    ///
    /// This is required for servers that reject direct server-issued logins for privileged operations.
    ///
    /// Optionally accepts an auth host to pass through to `spacetime login`,
    /// for example `--spacetime-login=https://spacetimedb.com`.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "")]
    spacetime_login: Option<String>,

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
        None => run_smoketest(args.server, args.dotnet, args.spacetime_login.as_deref(), args.args),
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

fn run_smoketest(
    server: Option<String>,
    dotnet: bool,
    spacetime_login_auth_host: Option<&str>,
    args: Vec<String>,
) -> Result<()> {
    // 1. Build binaries first (single process, no race)
    build_binaries()?;

    // 2. Build pre-compiled modules (this also warms the WASM dependency cache)
    build_precompiled_modules()?;

    let cli_path = ensure_binaries_built();
    let base_config_dir = prepare_base_config(&cli_path, server.as_deref(), spacetime_login_auth_host)?;
    let base_config_path = base_config_dir.path().join("config.toml");

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
        set_env(
            &mut cmd,
            server,
            dotnet,
            spacetime_login_auth_host.is_some(),
            &base_config_path,
        );
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
        set_env(
            &mut cmd,
            server,
            dotnet,
            spacetime_login_auth_host.is_some(),
            &base_config_path,
        );
        cmd.args(["test", "--release", "-p", "spacetimedb-smoketests"]);
        cmd
    };
    let status = cmd.args(&args).status()?;

    ensure!(status.success(), "Tests failed");
    let diff_status = cmd!("bash", "tools/check-diff.sh", "crates/smoketests").run()?;
    ensure!(
        diff_status.status.success(),
        "There is a diff in the smoketests directory."
    );
    Ok(())
}

fn prepare_base_config(
    cli_path: &Path,
    server: Option<&str>,
    spacetime_login_auth_host: Option<&str>,
) -> Result<TempDir> {
    if server.is_none() && spacetime_login_auth_host.is_some() {
        bail!("--spacetime-login requires --server");
    }

    let temp_dir = tempfile::tempdir()?;
    let config_path = temp_dir.path().join("config.toml");
    let config_path_str = config_path.to_str().context("invalid temp config path")?;

    // run an arbitrary command in order to initialize the config file
    let status = Command::new(cli_path)
        .args(["--config-path", config_path_str, "server", "set-default", "local"])
        .status()
        .context("failed to initialize smoketest server config")?;
    ensure!(status.success(), "spacetime server set-default failed");

    if let Some(server) = server {
        let status = Command::new(cli_path)
            .args([
                "--config-path",
                config_path_str,
                "server",
                "edit",
                "local",
                "--url",
                server,
                "--yes",
            ])
            .status()
            .context("failed to edit smoketest server config")?;
        ensure!(status.success(), "spacetime server edit failed");
    }

    if let Some(auth_host) = spacetime_login_auth_host {
        eprintln!("Logging in with SpacetimeAuth for remote smoketests...");
        let mut login = Command::new(cli_path);
        login.args(["--config-path", config_path_str, "login"]);
        if !auth_host.is_empty() {
            login.args(["--auth-host", auth_host]);
        }
        let status = login.status().context("failed to run spacetime login")?;
        ensure!(status.success(), "spacetime login failed");
    } else if server.is_some() {
        let status = Command::new(cli_path)
            .args([
                "--config-path",
                config_path_str,
                "login",
                "--server-issued-login",
                "local",
            ])
            .status()
            .context("failed to create server-issued smoketest identity")?;
        ensure!(status.success(), "spacetime login --server-issued-login failed");
    }

    ensure!(
        config_path.exists(),
        "smoketest config setup succeeded but did not create {}",
        config_path.display()
    );

    Ok(temp_dir)
}

fn set_env(cmd: &mut Command, server: Option<String>, dotnet: bool, spacetime_login: bool, base_config_path: &Path) {
    if let Some(ref server_url) = server {
        cmd.env("SPACETIME_REMOTE_SERVER", server_url);
    }
    cmd.env("SPACETIME_SMOKETEST_BASE_CONFIG_PATH", base_config_path);
    cmd.env(
        "SPACETIME_SMOKETEST_SPACETIME_LOGIN",
        if spacetime_login { "1" } else { "0" },
    );
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
        } else if ft.is_file()
            && path.extension() == Some(OsStr::new("rs"))
            && let Some(stem) = path.file_stem()
        {
            expected.insert(stem.to_string_lossy().to_string());
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
