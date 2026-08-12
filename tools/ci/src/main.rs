#![allow(clippy::disallowed_macros)]

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use duct::cmd;

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
    #[arg(long, default_value = "other-workflows")]
    skip: Vec<String>,
}

#[derive(Subcommand)]
enum CiCmd {
    /// Runs tests
    ///
    /// Runs rust tests, codegens csharp sdk and runs csharp tests.
    /// This does not include Unreal tests.
    /// This expects to run in a clean git state.
    Test(ci_args::test::Args),
    /// Lints the codebase
    ///
    /// Runs rustfmt, clippy, csharpier, TypeScript lint, and generates rust docs to ensure there
    /// are no warnings.
    Lint(ci_args::lint::Args),
    /// Tests Wasm bindings
    ///
    /// Runs tests for the codegen crate and builds a test module with the wasm bindings.
    WasmBindings(ci_args::wasm_bindings::Args),
    /// Deprecated; use `cargo regen csharp dlls`.
    Dlls,
    /// Runs smoketests
    ///
    /// Executes the smoketests suite with some default exclusions.
    Smoketests(ci_args::smoketests::SmoketestsArgs),
    /// Runs the keynote benchmark as a CI performance regression gate.
    ///
    /// Assumes release SpacetimeDB binaries and the TypeScript SDK are already built, runs the
    /// keynote SpacetimeDB benchmark for 60 seconds against the TypeScript and Rust modules, and
    /// fails if throughput is below 275K TPS for TypeScript or 300K TPS for Rust.
    KeynoteBench(ci_args::keynote_bench::Args),
    /// Tests the update flow
    ///
    /// Tests the self-update flow by building the spacetimedb-update binary for the specified
    /// target, by default the current target, and performing a self-install into a temporary
    /// directory.
    UpdateFlow(ci_args::update_flow::Args),
    /// Generates CLI documentation and checks for changes
    CliDocs(ci_args::cli_docs::Args),
    /// Verify that any non-root global.json files are symlinks to the root global.json.
    GlobalJsonPolicy(ci_args::global_json_policy::Args),
    /// Checks that publishable crates satisfy publish constraints.
    PublishChecks(ci_args::publish_checks::Args),
    /// Runs TypeScript workspace tests and template build checks.
    TypescriptTest(ci_args::typescript_test::Args),
    /// Verifies that the repository version upgrade tool still works.
    VersionUpgradeCheck(ci_args::version_upgrade_check::Args),
    /// Builds the docs site.
    Docs(ci_args::docs::Args),
    OtherWorkflows {
        #[command(subcommand)]
        cmd: OtherWorkflowsCmd,
    },
}

#[derive(Subcommand)]
enum OtherWorkflowsCmd {
    /// Selects or starts the private workflow for a public Internal Tests run.
    CoordinateInternalTests(ci_args::coordinate_internal_tests::Args),
    /// Checks that sensitive CODEOWNERS-controlled files have the required approvals.
    CodeownersCheck(ci_args::codeowners_check::Args),
    /// Interacts with CLA Assistant.
    ClaAssistant(ci_args::cla_assistant::Args),
    /// Waits for a GitHub Actions workflow run to complete.
    Watch(ci_args::workflow_watch::Args),
}

fn run_package(package: &str, args: &[String]) -> Result<()> {
    let mut cargo_args = vec!["run", "--package", package, "--"];
    cargo_args.extend(args.iter().map(String::as_str));
    cmd("cargo", cargo_args).run()?;
    Ok(())
}

fn forwarded_args(raw_args: &[String], path: &[&str]) -> Vec<String> {
    let args = &raw_args[1..];
    let mut start = None;
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--skip" => idx += 2,
            arg if arg.starts_with("--skip=") => idx += 1,
            _ => {
                if args[idx] == path[0]
                    && args
                        .get(idx..idx + path.len())
                        .is_some_and(|window| window.iter().map(String::as_str).eq(path.iter().copied()))
                {
                    start = Some(idx);
                    break;
                }
                idx += 1;
            }
        }
    }

    let start = start.unwrap_or_else(|| panic!("missing command path `{}` in raw argv", path.join(" ")));
    args[start + path.len()..].to_vec()
}

fn run_dlls() -> Result<()> {
    eprintln!("warning: `cargo ci dlls` is deprecated; use `cargo regen csharp dlls` instead");
    cmd!("cargo", "regen", "csharp", "dlls").run()?;
    Ok(())
}

fn run_command(cmd: CiCmd, raw_args: &[String]) -> Result<()> {
    match cmd {
        CiCmd::Test(_) => run_package("ci-test", &forwarded_args(raw_args, &["test"])),
        CiCmd::Lint(_) => run_package("ci-lint", &forwarded_args(raw_args, &["lint"])),
        CiCmd::WasmBindings(_) => run_package("ci-wasm-bindings", &forwarded_args(raw_args, &["wasm-bindings"])),
        CiCmd::Dlls => run_dlls(),
        CiCmd::Smoketests(_) => run_package("ci-smoketests", &forwarded_args(raw_args, &["smoketests"])),
        CiCmd::KeynoteBench(_) => run_package("ci-keynote-bench", &forwarded_args(raw_args, &["keynote-bench"])),
        CiCmd::UpdateFlow(_) => run_package("ci-update-flow", &forwarded_args(raw_args, &["update-flow"])),
        CiCmd::CliDocs(_) => run_package("ci-cli-docs", &forwarded_args(raw_args, &["cli-docs"])),
        CiCmd::GlobalJsonPolicy(_) => run_package(
            "ci-global-json-policy",
            &forwarded_args(raw_args, &["global-json-policy"]),
        ),
        CiCmd::PublishChecks(_) => run_package("ci-publish-checks", &forwarded_args(raw_args, &["publish-checks"])),
        CiCmd::TypescriptTest(_) => run_package("ci-typescript-test", &forwarded_args(raw_args, &["typescript-test"])),
        CiCmd::VersionUpgradeCheck(_) => run_package(
            "ci-version-upgrade-check",
            &forwarded_args(raw_args, &["version-upgrade-check"]),
        ),
        CiCmd::Docs(_) => run_package("ci-docs-build", &forwarded_args(raw_args, &["docs"])),
        CiCmd::OtherWorkflows { cmd } => match cmd {
            OtherWorkflowsCmd::CoordinateInternalTests(_) => run_package(
                "ci-coordinate-internal-tests",
                &forwarded_args(raw_args, &["other-workflows", "coordinate-internal-tests"]),
            ),
            OtherWorkflowsCmd::CodeownersCheck(_) => run_package(
                "ci-codeowners-check",
                &forwarded_args(raw_args, &["other-workflows", "codeowners-check"]),
            ),
            OtherWorkflowsCmd::ClaAssistant(_) => run_package(
                "ci-cla-assistant",
                &forwarded_args(raw_args, &["other-workflows", "cla-assistant"]),
            ),
            OtherWorkflowsCmd::Watch(_) => run_package(
                "ci-workflow-watch",
                &forwarded_args(raw_args, &["other-workflows", "watch"]),
            ),
        },
    }
}

fn run_all_clap_subcommands(skip: &[String]) -> Result<()> {
    for subcommand in Cli::command().get_subcommands() {
        let name = subcommand.get_name();
        if skip.iter().any(|skip| skip == name) {
            continue;
        }
        cmd!("cargo", "ci", name).run()?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let raw_args = std::env::args().collect::<Vec<_>>();
    let cli = Cli::parse_from(&raw_args);

    match cli.cmd {
        Some(cmd) => run_command(cmd, &raw_args),
        None => run_all_clap_subcommands(&cli.skip),
    }
}
