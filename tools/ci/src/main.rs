use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use duct::cmd;
use std::collections::HashMap;
use std::path::Path;
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

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum StartServer {
    /// Do not start a server; assume one is already running or remote
    No,
    /// Start a local server using the spacetimedb CLI
    Bare,
    /// Start services using docker compose
    Docker,
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
            value_enum,
            default_value_t = StartServer::Bare,
            long_help = "How to start SpacetimeDB before running smoketests: no | bare | docker"
        )]
        start_server: StartServer,
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

macro_rules! run {
    ($cmdline:expr) => {
        run_command($cmdline, &Vec::new())
    };
    ($cmdline:expr, $envs:expr) => {
        run_command($cmdline, $envs)
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
        run!(&format!("cargo ci {subcmd}"))?;
    }

    Ok(())
}

fn run_command(cmdline: &str, additional_env: &[(&str, &str)]) -> Result<()> {
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
            run!("cargo test --all -- --skip unreal")?;
            // The fallocate tests have been flakely when running in parallel
            run!("cargo test -p spacetimedb-durability --features fallocate -- --test-threads=1")?;
            run!("bash tools/check-diff.sh")?;
            run!("cargo run -p spacetimedb-codegen --example regen-csharp-moduledef && bash tools/check-diff.sh crates/bindings-csharp")?;
            run!("(cd crates/bindings-csharp && dotnet test -warnaserror)")?;
        }

        Some(CiCmd::Lint) => {
            run!("cargo fmt --all -- --check")?;
            run!("cargo clippy --all --tests --benches -- -D warnings")?;
            run!("(cd crates/bindings-csharp && dotnet tool restore && dotnet csharpier --check .)")?;
            // `bindings` is the only crate we care strongly about documenting,
            // since we link to its docs.rs from our website.
            // We won't pass `--no-deps`, though,
            // since we want everything reachable through it to also work.
            // This includes `sats` and `lib`.
            run!(
                "cd crates/bindings && cargo doc",
                // Make `cargo doc` exit with error on warnings, most notably broken links
                &[("RUSTDOCFLAGS", "--deny warnings")]
            )?;
        }

        Some(CiCmd::WasmBindings) => {
            run!("cargo test -p spacetimedb-codegen")?;
            run!("cargo update")?;
            run!("cargo run -p spacetimedb-cli -- build --project-path modules/module-test")?;
        }

        Some(CiCmd::Smoketests { start_server, args }) => {
            let mut started_pid: Option<i32> = None;
            match start_server {
                StartServer::Docker => {
                    // This means we ignore `.dockerignore`, beacuse it omits `target`, which our CI Dockerfile needs.
                    run!("cd .github && docker compose -f docker-compose.yml up -d")?;
                }
                StartServer::Bare => {
                    let pid_str;
                    if cfg!(target_os = "windows") {
                        pid_str = cmd!(
                            "powershell",
                            "-NoProfile",
                            "-Command",
                            "$p = Start-Process cargo -ArgumentList 'run -p spacetimedb-cli -- start --pg-port 5432' -PassThru; $p.Id"
                        )
                        .read()
                        .unwrap_or_default();
                    } else {
                        pid_str = cmd!(
                            "bash",
                            "-lc",
                            "cargo run -p spacetimedb-cli -- start --pg-port 5432 & echo $!"
                        )
                        .read()
                        .unwrap_or_default();
                    }
                    started_pid = Some(
                        pid_str
                            .trim()
                            .parse::<i32>()
                            .expect("Failed to get PID of started process"),
                    )
                }
                StartServer::No => {}
            }

            let py3_available = cmd!("bash", "-lc", "command -v python3 >/dev/null 2>&1")
                .run()
                .map(|s| s.status.success())
                .unwrap_or(false);
            let python = if py3_available { "python3" } else { "python" };

            let test_result = run!(&format!("{python} -m smoketests {}", args.join(" ")));

            match start_server {
                StartServer::Docker => {
                    let _ = run!("docker compose -f .github/docker-compose.yml down");
                }
                StartServer::Bare => {
                    let pid = if let Some(pid) = started_pid {
                        pid
                    } else {
                        // We constructed a `Some` above if `StartServer::Bare`.
                        unreachable!();
                    };
                    if cfg!(target_os = "windows") {
                        let _ = run!(&format!("powershell -NoProfile -Command \"Stop-Process -Id {} -Force -ErrorAction SilentlyContinue\"", pid));
                    } else {
                        let _ = run!(&format!("bash -lc 'kill {} 2>/dev/null || true'", pid));
                    }
                }
                StartServer::No => {}
            }

            test_result?;
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

            run!(&format!("echo 'checking update flow for target: {target}'"))?;
            run!(&format!(
                "cargo build {github_token_auth_flag}{target} -p spacetimedb-update"
            ))?;
            // NOTE(bfops): We need the `github-token-auth` feature because we otherwise tend to get ratelimited when we try to fetch `/releases/latest`.
            // My best guess is that, on the GitHub runners, the "anonymous" ratelimit is shared by *all* users of that runner (I think this because it
            // happens very frequently on the `macos-runner`, but we haven't seen it on any others).
            run!(&format!(
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

            run!("pnpm install --recursive")?;
            run!("cargo run --features markdown-docs -p spacetimedb-cli > docs/docs/cli-reference.md")?;
            run!("pnpm format")?;
            run!("git status")?;
            run!(
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
