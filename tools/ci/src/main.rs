use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use duct::cmd;
use log::{debug, warn};
use serde_json;
use std::collections::HashSet;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use std::{env, fs};
use tempfile::TempDir;

static PRINT_LOCK: Mutex<()> = Mutex::new(());

fn with_print_lock<F: FnOnce() -> R, R>(f: F) -> R {
    let _guard = PRINT_LOCK.lock().expect("print lock poisoned");
    f()
}

const README_PATH: &str = "tools/ci/README.md";

mod ci_docs;

/// SpacetimeDB CI tasks
///
/// This tool provides several subcommands for automating CI workflows in SpacetimeDB.
///
/// It may be invoked via `cargo ci <subcommand>`, or simply `cargo ci` to run all subcommands in
/// sequence. It is mostly designed to be run in CI environments via the github workflows, but can
/// also be run locally
#[derive(Parser)]
#[command(name = "cargo ci", subcommand_required = false, arg_required_else_help = false)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<CiCmd>,

    /// Skip specified subcommands when running all
    ///
    /// When no subcommand is specified, all subcommands are run in sequence. This option allows
    /// specifying subcommands to skip when running all. For example, to skip the `unreal-tests`
    /// subcommand, use `--skip unreal-tests`.
    #[arg(long)]
    skip: Vec<String>,
}

#[derive(Subcommand)]
enum CiCmd {
    /// Runs tests
    ///
    /// Runs rust tests, codegens csharp sdk and runs csharp tests.
    /// This does not include Unreal tests.
    /// This expects to run in a clean git state.
    Test,
    /// Lints the codebase
    ///
    /// Runs rustfmt, clippy, csharpier and generates rust docs to ensure there are no warnings.
    Lint,
    /// Tests Wasm bindings
    ///
    /// Runs tests for the codegen crate and builds a test module with the wasm bindings.
    WasmBindings,
    /// Runs smoketests
    ///
    /// Executes the smoketests suite with some default exclusions.
    Smoketests {
        #[arg(
            long = "python",
            value_name = "PYTHON_PATH",
            long_help = "Python interpreter to use for smoketests"
        )]
        python: Option<String>,

        /// List the tests that would be run, but don't run them
        #[arg(
            long = "list",
            num_args(0..=1),
            default_missing_value = "text",
            value_parser = ["text", "json"]
        )]
        list: Option<String>,

        // Args that influence test selection
        #[arg(
            long = "docker",
            value_name = "COMPOSE_FILE",
            num_args(0..=1),
            default_missing_value = "docker-compose.yml",
            long_help = "Use docker for smoketests, specifying a docker compose file. If no value is provided, docker-compose.yml is used by default. This cannot be combined with --start-server."
        )]
        docker: Option<String>,
        /// Ignore tests which require dotnet
        #[arg(long = "skip-dotnet", default_value_t = false)]
        skip_dotnet: bool,
        /// Only run tests which match the given substring (can be specified multiple times)
        #[arg(short = 'k', action = clap::ArgAction::Append)]
        test_name_patterns: Vec<String>,
        /// Exclude tests matching these names/patterns
        #[arg(short = 'x', num_args(0..))]
        exclude: Vec<String>,
        /// Run against a remote server
        #[arg(long = "remote-server")]
        remote_server: Option<String>,
        /// Only run tests that require a local server
        #[arg(long = "local-only", default_value_t = false)]
        local_only: bool,
        /// Use `spacetime login` for these tests (and disable tests that don't work with that)
        #[arg(long = "spacetime-login", default_value_t = false)]
        spacetime_login: bool,
        /// Tests to run (positional); if omitted, run all
        #[arg(value_name = "TEST")]
        test: Vec<String>,

        // Args that only influence test running
        /// Show all stdout/stderr from the tests as they're running
        #[arg(long = "show-all-output", default_value_t = false)]
        show_all_output: bool,
        /// Don't cargo build the CLI in the Python runner
        #[arg(long = "no-build-cli", default_value_t = false)]
        no_build_cli: bool,
        /// Do not stream docker logs alongside test output
        #[arg(long = "no-docker-logs", default_value_t = false)]
        no_docker_logs: bool,
        #[arg(
            long = "start-server",
            default_value_t = true,
            long_help = "Whether to start a local SpacetimeDB server before running smoketests"
        )]
        start_server: bool,
        #[arg(
            long = "parallel",
            default_value_t = false,
            long_help = "Run smoketests in parallel batches grouped by test suite"
        )]
        parallel: bool,
    },
    /// Tests the update flow
    ///
    /// Tests the self-update flow by building the spacetimedb-update binary for the specified
    /// target, by default the current target, and performing a self-install into a temporary
    /// directory.
    UpdateFlow {
        #[arg(
            long,
            long_help = "Target triple to build for, by default the current target. Used by github workflows to check the update flow on multiple platforms."
        )]
        target: Option<String>,
        #[arg(
            long,
            default_value = "false",
            long_help = "Whether to enable github token authentication feature when building the update binary. By default this is disabled."
        )]
        github_token_auth: bool,
    },
    /// Generates CLI documentation and checks for changes
    CliDocs {
        #[arg(
            long,
            long_help = "specify a custom path to the SpacetimeDB repository root (where the main Cargo.toml is located)"
        )]
        spacetime_path: Option<String>,
    },
    SelfDocs {
        #[arg(
            long,
            default_value_t = false,
            long_help = "Only check for changes, do not generate the docs"
        )]
        check: bool,
    },
}

fn run_all_clap_subcommands(skips: &[String]) -> Result<()> {
    let subcmds = Cli::command()
        .get_subcommands()
        .map(|sc| sc.get_name().to_string())
        .collect::<Vec<_>>();

    for subcmd in subcmds {
        if skips.contains(&subcmd) {
            log::info!("skipping {subcmd} as requested");
            continue;
        }
        log::info!("executing cargo ci {subcmd}");
        cmd!("cargo", "ci", &subcmd).run()?;
    }
    Ok(())
}
#[derive(Debug, Clone)]
pub enum StartServer {
    No,
    Yes,
    Docker { compose_file: PathBuf },
}

fn find_free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("failed to bind to an ephemeral port")?;
    let port = listener
        .local_addr()
        .context("failed to read local address for ephemeral port")?
        .port();
    drop(listener);
    Ok(port)
}

fn wait_until_http_ready(timeout: Duration, server_url: &str) -> Result<()> {
    println!("Waiting for server to start: {server_url}..");
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        // Use duct::cmd directly so we can suppress output from the ping command.
        let status = cmd(
            "cargo",
            &["run", "-p", "spacetimedb-cli", "--", "server", "ping", server_url],
        )
        .stdout_null()
        .stderr_null()
        .unchecked()
        .run();

        if let Ok(status) = status {
            if status.status.success() {
                debug!("Server started: {server_url}");
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    anyhow::bail!("Timed out waiting for {server_url}");
}

pub enum ServerState {
    None,
    Yes {
        handle: thread::JoinHandle<()>,
        data_dir: TempDir,
    },
    Docker {
        handle: thread::JoinHandle<()>,
        compose_file: PathBuf,
        project: String,
    },
}

impl ServerState {
    fn start(start_mode: StartServer, args: &mut Vec<String>) -> Result<Self> {
        Self::start_with_output(start_mode, args, None)
    }

    fn start_with_output(start_mode: StartServer, args: &mut Vec<String>, output: Option<&mut String>) -> Result<Self> {
        // TODO: Currently the server output leaks. We should be capturing it and only printing if the test fails.

        match start_mode {
            StartServer::No => Ok(Self::None),
            StartServer::Docker { compose_file } => {
                if let Some(buf) = output {
                    buf.push_str("Starting server..\n");
                } else {
                    println!("Starting server..");
                }
                let server_port = find_free_port()?;
                let pg_port = find_free_port()?;
                let tracy_port = find_free_port()?;
                let project = format!("spacetimedb-smoketests-{server_port}");
                args.push("--remote-server".into());
                let server_url = format!("http://localhost:{server_port}");
                args.push(server_url.clone());
                let compose_str = compose_file.to_string_lossy().to_string();

                // TODO: We don't capture the output from this, which pollutes the logs.
                let handle = thread::spawn({
                    let project = project.clone();
                    move || {
                        let _ = cmd!(
                            "docker",
                            "compose",
                            "-f",
                            &compose_str,
                            "--project-name",
                            &project,
                            "up",
                            "--abort-on-container-exit",
                        )
                        .env("STDB_PORT", server_port.to_string())
                        .env("STDB_PG_PORT", pg_port.to_string())
                        .env("STDB_TRACY_PORT", tracy_port.to_string())
                        .run();
                    }
                });
                wait_until_http_ready(Duration::from_secs(900), &server_url)?;
                Ok(ServerState::Docker {
                    handle,
                    compose_file,
                    project,
                })
            }
            StartServer::Yes => {
                // TODO: Make sure that this isn't brittle / multiple parallel batches don't grab the same port

                // Create a temporary data directory for this server instance.
                let data_dir = TempDir::new()?;

                let server_port = find_free_port()?;
                let pg_port = find_free_port()?;
                args.push("--remote-server".into());
                let server_url = format!("http://localhost:{server_port}");
                args.push(server_url.clone());
                if let Some(buf) = output {
                    buf.push_str("Starting server..\n");
                } else {
                    println!("Starting server..");
                }
                let data_dir_str = data_dir.path().to_string_lossy().to_string();
                let handle = thread::spawn(move || {
                    let _ = cmd!(
                        "cargo",
                        "run",
                        "-p",
                        "spacetimedb-cli",
                        "--",
                        "start",
                        "--listen-addr",
                        &format!("0.0.0.0:{server_port}"),
                        "--pg-port",
                        pg_port.to_string(),
                        "--data-dir",
                        data_dir_str,
                    )
                    .read();
                });
                wait_until_http_ready(Duration::from_secs(1200), &server_url)?;
                Ok(ServerState::Yes { handle, data_dir })
            }
        }
    }
}

impl Drop for ServerState {
    fn drop(&mut self) {
        // TODO: Consider doing a dance to have the server thread die, instead of just dying with this process.
        match self {
            ServerState::None => {}
            ServerState::Docker {
                handle: _,
                compose_file,
                project,
            } => {
                with_print_lock(|| {
                    println!("Shutting down server..");
                });
                let compose_str = compose_file.to_string_lossy().to_string();
                let _ = cmd!(
                    "docker",
                    "compose",
                    "-f",
                    &compose_str,
                    "--project-name",
                    &project,
                    "down",
                )
                .run();
            }
            ServerState::Yes { handle: _, data_dir } => {
                with_print_lock(|| {
                    println!("Shutting down server (temp data-dir will be dropped)..");
                });
                let _ = data_dir;
            }
        }
    }
}

fn run_smoketests_batch(server_mode: StartServer, args: &[String], python: &str) -> Result<()> {
    let mut args: Vec<_> = args.iter().cloned().collect();

    let _server = ServerState::start(server_mode, &mut args)?;

    println!("Running smoketests: {}", args.join(" "));
    cmd(
        python,
        ["-m", "smoketests"].into_iter().map(|s| s.to_string()).chain(args),
    )
    .run()?;
    Ok(())
}

// TODO: Fold this into `run_smoketests_batch`.
fn run_smoketests_batch_captured(server_mode: StartServer, args: &[String], python: &str) -> (String, Result<()>) {
    let mut args: Vec<_> = args.iter().cloned().collect();
    let mut output = String::new();

    let server = ServerState::start_with_output(server_mode, &mut args, Some(&mut output));
    let _server = match server {
        Ok(server) => server,
        Err(e) => return (output, Err(e)),
    };

    output.push_str(&format!("Running smoketests: {}\n", args.join(" ")));

    let res = cmd(
        python,
        ["-m", "smoketests"].into_iter().map(|s| s.to_string()).chain(args),
    )
    .stdout_capture()
    .stderr_capture()
    .unchecked()
    .run();

    let res = match res {
        Ok(res) => res,
        Err(e) => return (output, Err(e.into())),
    };

    let stdout = String::from_utf8_lossy(&res.stdout).to_string();
    let stderr = String::from_utf8_lossy(&res.stderr).to_string();
    if !stdout.is_empty() {
        output.push_str(&stdout);
        if !stdout.ends_with('\n') {
            output.push('\n');
        }
    }
    if !stderr.is_empty() {
        output.push_str(&stderr);
        if !stderr.ends_with('\n') {
            output.push('\n');
        }
    }

    if !res.status.success() {
        return (
            output,
            Err(anyhow::anyhow!("smoketests exited with status: {}", res.status)),
        );
    }

    (output, Ok(()))
}

fn server_start_config(start_server: bool, docker: Option<String>) -> StartServer {
    match (start_server, docker.as_ref()) {
        (start_server, Some(compose_file)) => {
            if !start_server {
                warn!("--docker implies --start-server=true");
            }
            StartServer::Docker {
                compose_file: compose_file.into(),
            }
        }
        (true, None) => StartServer::Yes,
        (false, None) => StartServer::No,
    }
}

fn common_args(
    docker: Option<String>,
    skip_dotnet: bool,
    test_name_patterns: Vec<String>,
    exclude: Vec<String>,
    local_only: bool,
    spacetime_login: bool,
    show_all_output: bool,
    no_build_cli: bool,
    no_docker_logs: bool,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    if no_docker_logs {
        args.push("--no-docker-logs".to_string());
    }
    if skip_dotnet {
        args.push("--skip-dotnet".to_string());
    }
    if show_all_output {
        args.push("--show-all-output".to_string());
    }
    for pat in test_name_patterns {
        args.push("-k".to_string());
        args.push(pat);
    }
    if !exclude.is_empty() {
        args.push("-x".to_string());
        args.push(exclude.join(" "));
    }
    if no_build_cli {
        args.push("--no-build-cli".to_string());
    }
    if spacetime_login {
        args.push("--spacetime-login".to_string());
    }
    if local_only {
        args.push("--local-only".to_string());
    }

    if let Some(compose_file) = docker.as_ref() {
        args.push("--docker".to_string());
        args.push("--compose-file".to_string());
        args.push(compose_file.to_string());
    }

    args
}

fn infer_python() -> String {
    let py3_available = cmd!("python3", "--version").run().is_ok();
    if py3_available {
        "python3".to_string()
    } else {
        "python".to_string()
    }
}

fn run_smoketests_serial(
    python: String,
    list: Option<String>,
    docker: Option<String>,
    skip_dotnet: bool,
    test_name_patterns: Vec<String>,
    exclude: Vec<String>,
    remote_server: Option<String>,
    local_only: bool,
    spacetime_login: bool,
    test: Vec<String>,
    show_all_output: bool,
    no_build_cli: bool,
    no_docker_logs: bool,
    start_server: StartServer,
) -> Result<()> {
    let mut args = Vec::new();
    if let Some(list_mode) = list {
        args.push(format!("--list={list_mode}").to_string());
    }
    if let Some(remote) = remote_server {
        args.push("--remote-server".to_string());
        args.push(remote);
    }
    for test in test {
        args.push(test.clone());
    }
    // The python smoketests take -x X Y Z, which can be ambiguous with passing test names as args to run.
    // So, we make sure the anonymous test name arg has been added _before_ the exclude args which are a part of common_args.
    args.extend(common_args(
        docker,
        skip_dotnet,
        test_name_patterns,
        exclude,
        local_only,
        spacetime_login,
        show_all_output,
        no_build_cli,
        no_docker_logs,
    ));
    run_smoketests_batch(start_server, &args, &python)?;
    Ok(())
}

fn run_smoketests_parallel(
    python: String,
    list: Option<String>,
    docker: Option<String>,
    skip_dotnet: bool,
    test_name_patterns: Vec<String>,
    exclude: Vec<String>,
    remote_server: Option<String>,
    local_only: bool,
    spacetime_login: bool,
    test: Vec<String>,
    show_all_output: bool,
    no_build_cli: bool,
    no_docker_logs: bool,
    start_server: StartServer,
) -> Result<()> {
    let args = common_args(
        docker,
        skip_dotnet,
        test_name_patterns,
        exclude,
        local_only,
        spacetime_login,
        show_all_output,
        no_build_cli,
        no_docker_logs,
    );

    if list.is_some() {
        anyhow::bail!("--list does not make sense with --parallel");
    }
    if remote_server.is_some() {
        // This is just because we manually provide --remote-server later, so it requires some refactoring.
        anyhow::bail!("--remote-server is not supported in parallel mode");
    }

    // TODO: Handle --local-only tests separately, since we are passing --remote-server in all of our batches.

    println!("Listing smoketests for parallel execution..");

    let tests = {
        let mut list_args: Vec<String> = args.clone();
        list_args.push("--list=json".to_string());
        // TODO: Are users able to list specific tests here, or just top-level test filenames?
        // If they can list individual tests, then this won't work as expected (because we should past those restrictions later
        // when we run each batch as well).
        for test in test {
            list_args.push(test.clone());
        }

        let output = cmd(
            python.clone(),
            ["-m", "smoketests"].into_iter().map(|s| s.to_string()).chain(list_args),
        )
        .stderr_to_stdout()
        .read()
        .expect("Failed to list smoketests");

        let parsed: serde_json::Value = serde_json::from_str(&output)?;
        let tests = parsed.get("tests").and_then(|v| v.as_array()).cloned().unwrap();
        let errors = parsed
            .get("errors")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        if !errors.is_empty() {
            println!("Errors while constructing smoketests:");
            for err in &errors {
                let test_id = err.get("test_id").and_then(|v| v.as_str()).unwrap();
                let msg = err.get("error").and_then(|v| v.as_str()).unwrap();
                println!("{test_id}");
                println!("{msg}");
            }
            // If there were errors constructing tests, treat this as a failure
            // and do not run any batches.
            anyhow::bail!("Errors encountered while constructing smoketests; aborting parallel run");
        }

        tests
    };

    let batches: HashSet<String> = tests
        .into_iter()
        .map(|t| {
            let name = t.as_str().unwrap();
            let parts = name.split('.').collect::<Vec<&str>>();
            parts[2].to_string()
        })
        .collect();

    // Run each batch in parallel threads.
    let mut handles = Vec::new();
    for batch in batches {
        let start_server_clone = start_server.clone();
        let python = python.clone();
        let mut batch_args: Vec<String> = Vec::new();
        batch_args.push(batch.clone());
        batch_args.extend(args.iter().cloned());

        handles.push((
            batch.clone(),
            std::thread::spawn(move || {
                let (captured, result) = run_smoketests_batch_captured(start_server_clone, &batch_args, &python);

                with_print_lock(|| {
                    println!("===== smoketests batch: {batch} =====");
                    print!("{captured}");
                    if let Err(e) = &result {
                        println!("(batch failed) {e:?}");
                    }
                    println!("===== end smoketests batch: {batch} =====");
                });

                result
            }),
        ));
    }

    let mut failed_batches = vec![];
    for (batch, handle) in handles {
        // If the thread panicked or the batch failed, treat it as a failure.
        let result = handle
            .join()
            .unwrap_or_else(|_| Err(anyhow::anyhow!("smoketest batch thread panicked",)));
        if let Err(e) = result {
            println!("Smoketest batch {batch} failed: {e:?}");
            failed_batches.push(batch);
        }
    }

    if !failed_batches.is_empty() {
        anyhow::bail!("Smoketest batch(es) failed: {}", failed_batches.join(", "));
    }

    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    // Remove all Cargo-provided env vars from the subcommand
    for (key, _) in std::env::vars() {
        if key.starts_with("CARGO_") && key != "CARGO_TARGET_DIR" {
            std::env::remove_var(key);
        }
    }

    match cli.cmd {
        Some(CiCmd::Test) => {
            // TODO: This doesn't work on at least user Linux machines, because something here apparently uses `sudo`?

            cmd!("cargo", "test", "--all", "--", "--skip", "unreal").run()?;
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
        }

        Some(CiCmd::Lint) => {
            cmd!("cargo", "fmt", "--all", "--", "--check").run()?;
            cmd!(
                "cargo",
                "clippy",
                "--all",
                "--tests",
                "--benches",
                "--",
                "-D",
                "warnings",
            )
            .run()?;
            cmd!("dotnet", "tool", "restore").dir("crates/bindings-csharp").run()?;
            cmd!("dotnet", "csharpier", "--check", ".")
                .dir("crates/bindings-csharp")
                .run()?;
            // `bindings` is the only crate we care strongly about documenting,
            // since we link to its docs.rs from our website.
            // We won't pass `--no-deps`, though,
            // since we want everything reachable through it to also work.
            // This includes `sats` and `lib`.
            cmd!("cargo", "doc")
                .dir("crates/bindings")
                // Make `cargo doc` exit with error on warnings, most notably broken links
                .env("RUSTDOCFLAGS", "--deny warnings")
                .run()?;
        }

        Some(CiCmd::WasmBindings) => {
            cmd!("cargo", "test", "-p", "spacetimedb-codegen").run()?;
            // Make sure the `Cargo.lock` file reflects the latest available versions.
            // This is what users would end up with on a fresh module, so we want to
            // catch any compile errors arising from a different transitive closure
            // of dependencies than what is in the workspace lock file.
            //
            // For context see also: https://github.com/clockworklabs/SpacetimeDB/pull/2714
            cmd!("cargo", "update").run()?;
            cmd!(
                "cargo",
                "run",
                "-p",
                "spacetimedb-cli",
                "--",
                "build",
                "--project-path",
                "modules/module-test",
            )
            .run()?;
        }

        Some(CiCmd::Smoketests {
            start_server,
            docker,
            test,
            no_docker_logs,
            skip_dotnet,
            show_all_output,
            test_name_patterns,
            exclude,
            mut no_build_cli,
            list,
            remote_server,
            spacetime_login,
            local_only,
            parallel,
            python,
        }) => {
            let start_server = server_start_config(start_server, docker.clone());
            // Do initial server build
            match start_server.clone() {
                StartServer::No => {}
                StartServer::Yes { .. } => {
                    println!("Building SpacetimeDB..");

                    // Pre-build so that `cargo run -p spacetimedb-cli` will immediately start. Otherwise we risk timing out waiting for the server to come up.
                    cmd!(
                        "cargo",
                        "build",
                        "-p",
                        "spacetimedb-cli",
                        "-p",
                        "spacetimedb-standalone",
                        "-p",
                        "spacetimedb-update",
                    )
                    .run()?;
                    no_build_cli = true;
                }
                StartServer::Docker { compose_file } => {
                    println!("Building docker container..");
                    let compose_str = compose_file.to_string_lossy().to_string();
                    let _ = cmd!("docker", "compose", "-f", &compose_str, "build",).run()?;
                }
            }

            let python = python.unwrap_or(infer_python());

            // These are split into two separate functions, so that we can ensure all the args are considered in both cases.
            if parallel {
                run_smoketests_parallel(
                    python,
                    list,
                    docker,
                    skip_dotnet,
                    test_name_patterns,
                    exclude,
                    remote_server,
                    local_only,
                    spacetime_login,
                    test,
                    show_all_output,
                    no_build_cli,
                    no_docker_logs,
                    start_server,
                )?;
            } else {
                run_smoketests_serial(
                    python,
                    list,
                    docker,
                    skip_dotnet,
                    test_name_patterns,
                    exclude,
                    remote_server,
                    local_only,
                    spacetime_login,
                    test,
                    show_all_output,
                    no_build_cli,
                    no_docker_logs,
                    start_server,
                )?;
            }
        }

        Some(CiCmd::UpdateFlow {
            target,
            github_token_auth,
        }) => {
            let mut common_args = vec![];
            if let Some(target) = target.as_ref() {
                common_args.push("--target");
                common_args.push(target);
                log::info!("checking update flow for target: {target}");
            } else {
                log::info!("checking update flow");
            }
            if github_token_auth {
                common_args.push("--features");
                common_args.push("github-token-auth");
            }

            cmd(
                "cargo",
                ["build", "-p", "spacetimedb-update"]
                    .into_iter()
                    .chain(common_args.clone()),
            )
            .run()?;
            // NOTE(bfops): We need the `github-token-auth` feature because we otherwise tend to get ratelimited when we try to fetch `/releases/latest`.
            // My best guess is that, on the GitHub runners, the "anonymous" ratelimit is shared by *all* users of that runner (I think this because it
            // happens very frequently on the `macos-runner`, but we haven't seen it on any others).
            let root_dir = tempfile::tempdir()?;
            let root_dir_string = root_dir.path().to_string_lossy().to_string();
            let root_arg = format!("--root-dir={}", root_dir_string);
            cmd(
                "cargo",
                ["run", "-p", "spacetimedb-update"]
                    .into_iter()
                    .chain(common_args.clone())
                    .chain(["--", "self-install", &root_arg, "--yes"].into_iter()),
            )
            .run()?;
            cmd!(format!("{}/spacetime", root_dir_string), &root_arg, "help",).run()?;
        }

        Some(CiCmd::CliDocs { spacetime_path }) => {
            if let Some(path) = spacetime_path {
                env::set_current_dir(path).ok();
            }
            let current_dir = env::current_dir().expect("No current directory!");
            let dir_name = current_dir.file_name().expect("No current directory!");
            if dir_name != "SpacetimeDB" && dir_name != "public" {
                anyhow::bail!(
                    "You must execute this binary from inside of the SpacetimeDB directory, or use --spacetime-path"
                );
            }

            cmd!("pnpm", "install", "--recursive").run()?;
            cmd!("pnpm", "generate-cli-docs").run()?;
            let out = cmd!("git", "status", "--porcelain").read()?;
            if out == "" {
                log::info!("No docs changes detected");
            } else {
                anyhow::bail!("CLI docs are out of date");
            }
        }

        Some(CiCmd::SelfDocs { check }) => {
            let readme_content = ci_docs::generate_cli_docs();
            let path = Path::new(README_PATH);

            if check {
                let existing = fs::read_to_string(path).unwrap_or_default();
                if existing != readme_content {
                    bail!("README.md is out of date. Please run `cargo ci self-docs` to update it.");
                } else {
                    log::info!("README.md is up to date.");
                }
            } else {
                fs::write(path, readme_content)?;
                log::info!("Wrote CLI docs to {}", path.display());
            }
        }

        None => run_all_clap_subcommands(&cli.skip)?,
    }

    Ok(())
}
