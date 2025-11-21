use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use duct::cmd;
use log::warn;
use std::collections::HashMap;
use std::net::TcpListener;
use std::path::Path;
use std::thread;
use std::{env, fs};

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
            long = "start-server",
            default_value_t = true,
            long_help = "Whether to start a local SpacetimeDB server before running smoketests"
        )]
        start_server: bool,
        #[arg(
            long = "docker",
            value_name = "COMPOSE_FILE",
            num_args(0..=1),
            default_missing_value = "docker-compose.yml",
            long_help = "Use docker for smoketests, specifying a docker compose file. If no value is provided, docker-compose.yml is used by default. This cannot be combined with --start-server."
        )]
        docker: Option<String>,
        #[arg(
            long = "parallel",
            default_value_t = false,
            long_help = "Run smoketest suites in parallel, one process per top-level suite"
        )]
        parallel: bool,
        #[arg(
            trailing_var_arg = true,
            long_help = "Additional arguments to pass to the smoketests runner. These are usually set by the CI environment, such as `-- --docker`"
        )]
        args: Vec<String>,
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

macro_rules! bash {
    ($cmdline:expr) => {
        run_bash($cmdline, &Vec::new())
    };
    ($cmdline:expr, $envs:expr) => {
        run_bash($cmdline, $envs)
    };
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
        bash!(&format!("cargo ci {subcmd}"))?;
    }

    Ok(())
}

fn prebuild_bare_server() -> Result<()> {
    // Pre-build so that `cargo run -p spacetimedb-cli` will immediately start. Otherwise we risk starting the tests
    // before the server is up.
    bash!("cargo build -p spacetimedb-cli -p spacetimedb-standalone")
}

fn run_smoketests_inner(
    python: &str,
    suite_name: Option<&str>,
    start_server: bool,
    docker: &Option<String>,
    per_suite: bool,
    args: &[String],
) -> (String, bool) {
    let mut output = String::new();
    let mut ok = true;

    // Server setup
    let mut pid: Option<i32> = None;
    let mut port: Option<u16> = None;
    if let Some(compose_file) = docker.as_ref() {
        output.push_str("Starting server..\n");
        // This means we ignore `.dockerignore`, beacuse it omits `target`, which our CI Dockerfile needs.
        let cmdline = format!("docker compose -f {} up -d", compose_file);
        let res = cmd!("bash", "-lc", &cmdline).stderr_to_stdout().run();
        if let Err(e) = res {
            output.push_str(&format!("Failed to start docker: {}\n", e));
            return (output, false);
        }
    } else if start_server {
        // Bare server: shared (port 5432) in serial mode, or per-suite port + --remote-server in parallel mode.
        if !per_suite {
            if let Err(e) = prebuild_bare_server() {
                output.push_str(&format!("Failed to prebuild server: {}\n", e));
                return (output, false);
            }
            output.push_str("Starting server..\n");
            let cmdline = "nohup cargo run -p spacetimedb-cli -- start --pg-port 5432 >/dev/null 2>&1 & echo $!";
            let pid_str = cmd!("bash", "-lc", cmdline).read().unwrap_or_default();
            match pid_str.trim().parse::<i32>() {
                Ok(p) => pid = Some(p),
                Err(e) => {
                    output.push_str(&format!("Failed to parse server PID from '{}': {}\n", pid_str, e));
                    return (output, false);
                }
            }
        } else {
            let free_port = match find_free_port() {
                Ok(p) => p,
                Err(e) => {
                    output.push_str(&format!("Failed to find free port: {}\n", e));
                    return (output, false);
                }
            };
            port = Some(free_port);
            output.push_str(&format!("Starting local server on port {}..\n", free_port));
            let cmdline = format!(
                "nohup cargo run -p spacetimedb-cli -- start --pg-port {} >/dev/null 2>&1 & echo $!",
                free_port
            );
            let pid_str = cmd!("bash", "-lc", &cmdline).read().unwrap_or_default();
            match pid_str.trim().parse::<i32>() {
                Ok(p) => pid = Some(p),
                Err(e) => {
                    output.push_str(&format!("Failed to parse server PID from '{}': {}\n", pid_str, e));
                    return (output, false);
                }
            }
        }
    }

    // Build smoketests args
    let mut smoketests_args = Vec::new();
    if let Some(name) = suite_name {
        smoketests_args.push(name.to_string());
    }
    smoketests_args.extend(args.iter().cloned());

    if let Some(compose_file) = docker.as_ref() {
        // Note that we do not assume that the user wants to pass --docker to the tests. We leave them the power to
        // run the server in docker while still retaining full control over what tests they want.
        smoketests_args.push("--compose-file".to_string());
        smoketests_args.push(compose_file.to_string());
    }

    if start_server && docker.is_none() && per_suite {
        if let Some(p) = port {
            smoketests_args.push("--remote-server".to_string());
            smoketests_args.push(format!("http://127.0.0.1:{}", p));
        }
    }

    output.push_str("Running smoketests..\n");
    let cmdline = format!("{} -m smoketests {}", python, smoketests_args.join(" "));
    let res = cmd!("bash", "-lc", &cmdline).stderr_to_stdout().read();
    match res {
        Ok(out) => {
            output.push_str(&out);
        }
        Err(e) => {
            output.push_str(&format!("smoketests failed: {}\n", e));
            ok = false;
        }
    }

    // Shutdown
    if let Some(compose_file) = docker.as_ref() {
        output.push_str("Shutting down server..\n");
        let down_cmd = format!("docker compose -f {} down", compose_file);
        let _ = cmd!("bash", "-lc", &down_cmd).run();
    }

    if let Some(p) = pid {
        output.push_str("Shutting down server..\n");
        let kill_cmd = format!("kill {}", p);
        let _ = cmd!("bash", "-lc", &kill_cmd).run();
    }

    (output, ok)
}

fn run_smoketests_serial(python: &str, start_server: bool, docker: &Option<String>, args: &[String]) -> Result<()> {
    let (output, ok) = run_smoketests_inner(python, None, start_server, docker, false, args);
    print!("{}", output);
    if !ok {
        bail!("smoketests failed");
    }
    Ok(())
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

fn list_smoketest_suites() -> Result<Vec<String>> {
    let mut suites = Vec::new();
    for entry in fs::read_dir("smoketests/tests").context("failed to read smoketests/tests directory")? {
        let entry = entry?;
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name == "__init__.py" {
                continue;
            }
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "py" {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        suites.push(stem.to_string());
                    }
                }
            }
        }
    }
    suites.sort();
    Ok(suites)
}

fn run_bash(cmdline: &str, additional_env: &[(&str, &str)]) -> Result<()> {
    let mut env = env::vars().collect::<HashMap<_, _>>();
    env.extend(additional_env.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    log::debug!("$ {cmdline}");
    let status = cmd!("bash", "-lc", cmdline).full_env(env).run()?;
    if !status.status.success() {
        let e = anyhow::anyhow!("command failed: {cmdline}");
        log::error!("{e}");
        return Err(e);
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Some(CiCmd::Test) => {
            bash!("cargo test --all -- --skip unreal")?;
            // The fallocate tests have been flakely when running in parallel
            bash!("cargo test -p spacetimedb-durability --features fallocate -- --test-threads=1")?;
            bash!("bash tools/check-diff.sh")?;
            bash!("cargo run -p spacetimedb-codegen --example regen-csharp-moduledef && bash tools/check-diff.sh crates/bindings-csharp")?;
            bash!("(cd crates/bindings-csharp && dotnet test -warnaserror)")?;
        }

        Some(CiCmd::Lint) => {
            bash!("cargo fmt --all -- --check")?;
            bash!("cargo clippy --all --tests --benches -- -D warnings")?;
            bash!("(cd crates/bindings-csharp && dotnet tool restore && dotnet csharpier --check .)")?;
            // `bindings` is the only crate we care strongly about documenting,
            // since we link to its docs.rs from our website.
            // We won't pass `--no-deps`, though,
            // since we want everything reachable through it to also work.
            // This includes `sats` and `lib`.
            bash!(
                "cd crates/bindings && cargo doc",
                // Make `cargo doc` exit with error on warnings, most notably broken links
                &[("RUSTDOCFLAGS", "--deny warnings")]
            )?;
        }

        Some(CiCmd::WasmBindings) => {
            bash!("cargo test -p spacetimedb-codegen")?;
            bash!("cargo update")?;
            bash!("cargo run -p spacetimedb-cli -- build --project-path modules/module-test")?;
        }

        Some(CiCmd::Smoketests {
            start_server,
            docker,
            parallel,
            args,
        }) => {
            // TODO: does this work on windows?
            let py3_available = cmd!("bash", "-lc", "command -v python3 >/dev/null 2>&1")
                .run()
                .map(|s| s.status.success())
                .unwrap_or(false);
            let python = if py3_available { "python3" } else { "python" };

            if !parallel {
                run_smoketests_serial(python, start_server, &docker, &args)?;
            } else {
                // Parallel mode: run each top-level smoketest suite in its own process.
                let suites = list_smoketest_suites()?;
                if suites.is_empty() {
                    bail!("No smoketest suites found in smoketests/tests");
                }

                let mut handles = Vec::new();
                for suite in suites {
                    let suite_name = suite.clone();
                    let docker = docker.clone();
                    let args = args.clone();
                    let python = python.to_string();
                    let start_server = start_server;

                    let handle = thread::spawn(move || {
                        let (output, ok) =
                            run_smoketests_inner(&python, Some(&suite_name), start_server, &docker, true, &args);
                        (suite_name, output, ok)
                    });

                    handles.push(handle);
                }

                let mut all_ok = true;
                let mut results = Vec::new();
                for handle in handles {
                    match handle.join() {
                        Ok((suite, output, ok)) => {
                            results.push((suite, output, ok));
                        }
                        Err(_) => {
                            results.push(("<thread-panic>".to_string(), "thread panicked".to_string(), false));
                        }
                    }
                }

                // Print outputs in a stable order.
                results.sort_by(|a, b| a.0.cmp(&b.0));
                for (suite, output, ok) in &results {
                    println!("===== smoketests suite: {} =====", suite);
                    print!("{}", output);
                    println!(
                        "===== end suite: {} (status: {}) =====",
                        suite,
                        if *ok { "ok" } else { "FAILED" }
                    );
                    if !ok {
                        all_ok = false;
                    }
                }

                if !all_ok {
                    bail!("One or more smoketest suites failed");
                }
            }
        }

        Some(CiCmd::UpdateFlow {
            target,
            github_token_auth,
        }) => {
            let target = target.map(|t| format!("--target {t}")).unwrap_or_default();
            let github_token_auth_flag = if github_token_auth {
                "--features github-token-auth "
            } else {
                ""
            };

            bash!(&format!("echo 'checking update flow for target: {target}'"))?;
            bash!(&format!(
                "cargo build {github_token_auth_flag}{target} -p spacetimedb-update"
            ))?;
            // NOTE(bfops): We need the `github-token-auth` feature because we otherwise tend to get ratelimited when we try to fetch `/releases/latest`.
            // My best guess is that, on the GitHub runners, the "anonymous" ratelimit is shared by *all* users of that runner (I think this because it
            // happens very frequently on the `macos-runner`, but we haven't seen it on any others).
            bash!(&format!(
                r#"
ROOT_DIR="$(mktemp -d)"
cargo run {github_token_auth_flag}{target} -p spacetimedb-update -- self-install --root-dir="${{ROOT_DIR}}" --yes
"${{ROOT_DIR}}"/spacetime --root-dir="${{ROOT_DIR}}" help
        "#
            ))?;
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

            bash!("pnpm install --recursive")?;
            bash!("cargo run --features markdown-docs -p spacetimedb-cli > docs/docs/cli-reference.md")?;
            bash!("pnpm format")?;
            bash!("git status")?;
            bash!(
                r#"
if git diff --exit-code HEAD; then
  echo "No docs changes detected"
else
  echo "It looks like the CLI docs have changed:"
  exit 1
fi
                "#
            )?;
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
