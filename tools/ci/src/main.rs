use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use duct::cmd;
use log::warn;
use serde_json;
use std::collections::{HashMap, HashSet};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
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
            long_help = "Run smoketests in parallel batches grouped by test suite"
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

#[derive(Debug, Clone)]
pub enum StartServer {
    No,
    Yes { random_port: bool },
    Docker { compose_file: PathBuf, random_port: bool },
}

#[derive(Debug, Clone)]
pub enum ServerState {
    None,
    Yes { pid: i32 },
    Docker { compose_file: PathBuf, project: String },
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

fn run_smoketests_batch(server_mode: StartServer, args: &[String], python: &str) -> Result<()> {
    let server_state = match server_mode {
        StartServer::No => ServerState::None,
        StartServer::Docker {
            compose_file,
            random_port,
        } => {
            println!("Starting server..");
            let env_string;
            let project;
            if random_port {
                let server_port = find_free_port()?;
                let pg_port = find_free_port()?;
                let tracy_port = find_free_port()?;
                env_string = format!("STDB_PORT={server_port} STDB_PG_PORT={pg_port} STDB_TRACY_PORT={tracy_port}");
                project = format!("spacetimedb-smoketests-{server_port}");
            } else {
                env_string = String::new();
                project = "spacetimedb-smoketests".to_string();
            };
            let compose_str = compose_file.to_string_lossy();
            bash!(&format!(
                "{env_string} docker compose -f {compose_str} --project {project} up -d"
            ))?;
            ServerState::Docker { compose_file, project }
        }
        StartServer::Yes { random_port } => {
            // Pre-build so that `cargo run -p spacetimedb-cli` will immediately start. Otherwise we risk starting the tests
            // before the server is up.
            bash!("cargo build -p spacetimedb-cli -p spacetimedb-standalone")?;

            // TODO: Make sure that this isn't brittle / multiple parallel batches don't grab the same port
            let arg_string = if random_port {
                let server_port = find_free_port()?;
                let pg_port = find_free_port()?;
                &format!("--listen-addr 0.0.0.0:{server_port} --pg-port {pg_port}")
            } else {
                "--pg-port 5432"
            };
            println!("Starting server..");
            let pid_str;
            if cfg!(target_os = "windows") {
                pid_str = cmd!(
                        "powershell",
                        "-NoProfile",
                        "-Command",
                        &format!(
                            "$p = Start-Process cargo -ArgumentList 'run -p spacetimedb-cli -- start {arg_string}' -PassThru; $p.Id"
                        )
                    )
                    .read()
                    .unwrap_or_default();
            } else {
                pid_str = cmd!(
                    "bash",
                    "-lc",
                    &format!("nohup cargo run -p spacetimedb-cli -- start {arg_string} >/dev/null 2>&1 & echo $!")
                )
                .read()
                .unwrap_or_default();
            }
            ServerState::Yes {
                pid: pid_str
                    .trim()
                    .parse::<i32>()
                    .expect("Failed to get PID of started process"),
            }
        }
    };

    println!("Running smoketests..");
    let test_result = bash!(&format!("{python} -m smoketests {}", args.join(" ")));

    // TODO: Make an effort to run the wind-down behavior if we ctrl-C this process
    match server_state {
        ServerState::None => {}
        ServerState::Docker { compose_file, project } => {
            println!("Shutting down server..");
            let compose_str = compose_file.to_string_lossy();
            let _ = bash!(&format!("docker compose -f {compose_str} --project {project} down"));
        }
        ServerState::Yes { pid } => {
            println!("Shutting down server..");
            if cfg!(target_os = "windows") {
                let _ = bash!(&format!(
                    "powershell -NoProfile -Command \"Stop-Process -Id {} -Force -ErrorAction SilentlyContinue\"",
                    pid
                ));
            } else {
                let _ = bash!(&format!("kill {}", pid));
            }
        }
    }

    test_result
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
            let start_server = match (start_server, docker.as_ref()) {
                (start_server, Some(compose_file)) => {
                    if !start_server {
                        warn!("--docker implies --start-server=true");
                    }
                    StartServer::Docker {
                        random_port: parallel,
                        compose_file: compose_file.into(),
                    }
                }
                (true, None) => StartServer::Yes { random_port: parallel },
                (false, None) => StartServer::No,
            };
            let mut args = args.to_vec();
            if let Some(compose_file) = docker.as_ref() {
                // Note that we do not assume that the user wants to pass --docker to the tests. We leave them the power to
                // run the server in docker while still retaining full control over what tests they want.
                args.push("--compose-file".to_string());
                args.push(compose_file.to_string());
            }

            // TODO: does this work on windows?
            let py3_available = cmd!("bash", "-lc", "command -v python3 >/dev/null 2>&1")
                .run()
                .map(|s| s.status.success())
                .unwrap_or(false);
            let python = if py3_available { "python3" } else { "python" };

            if parallel {
                println!("Listing smoketests for parallel execution..");

                let mut list_args: Vec<String> = args.to_vec();
                list_args.push("--list=json".to_string());
                let list_cmdline = format!("{python} -m smoketests {}", list_args.join(" "));

                // TODO: do actually check the return code here. and make --list=json not return non-zero if there are errors.
                let list_output = cmd!("bash", "-lc", list_cmdline)
                    .stderr_to_stdout()
                    .unchecked()
                    .read()?;

                let parsed: serde_json::Value = serde_json::from_str(&list_output)?;
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
                    return Err(anyhow::anyhow!(
                        "Errors encountered while constructing smoketests; aborting parallel run"
                    ));
                }

                let batches: HashSet<String> = tests
                    .into_iter()
                    .map(|t| {
                        let name = t.as_str().unwrap();
                        let parts = name.split('.').collect::<Vec<&str>>();
                        parts[2].to_string()
                    })
                    .collect();

                let mut any_failed_batch = false;
                for batch in batches {
                    println!("Running smoketests batch {batch}..");
                    // TODO: this doesn't work properly if the user passed multiple batches as input.
                    let mut batch_args: Vec<String> = Vec::new();
                    batch_args.push(batch.clone());
                    batch_args.extend(args.iter().cloned());

                    // TODO: capture output and print it only in contiguous blocks
                    let result = run_smoketests_batch(start_server.clone(), &batch_args, python);

                    if result.is_err() {
                        any_failed_batch = true;
                    }
                }

                if any_failed_batch {
                    anyhow::bail!("One or more smoketest batches failed");
                }
            } else {
                run_smoketests_batch(start_server, &args, python)?;
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
