use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, Subcommand};
use duct::cmd;
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

fn infer_python() -> String {
    let py3_available = cmd!("python3", "--version").run().is_ok();
    if py3_available {
        "python3".to_string()
    } else {
        "python".to_string()
    }
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

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

        Some(CiCmd::Smoketests { args: smoketest_args }) => {
            let python = infer_python();
            cmd(
                python,
                ["-m", "smoketests"]
                    .into_iter()
                    .map(|s| s.to_string())
                    .chain(smoketest_args)
                    .chain(vec!["spacetimedb_init".to_string()]),
            )
            .run()?;
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
            cmd!("pnpm", "generate-cli-docs").dir("docs").run()?;
            let out = cmd!("git", "status", "--porcelain", "--", "docs").read()?;
            if out.is_empty() {
                log::info!("No docs changes detected");
            } else {
                anyhow::bail!("CLI docs are out of date:\n{out}");
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
