#![allow(clippy::disallowed_macros)]

use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand};
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
    Test(ForwardedArgs),
    /// Lints the codebase
    ///
    /// Runs rustfmt, clippy, csharpier, TypeScript lint, and generates rust docs to ensure there
    /// are no warnings.
    Lint(ForwardedArgs),
    /// Tests Wasm bindings
    ///
    /// Runs tests for the codegen crate and builds a test module with the wasm bindings.
    WasmBindings(ForwardedArgs),
    /// Deprecated; use `cargo regen csharp dlls`.
    Dlls,
    /// Runs smoketests
    ///
    /// Executes the smoketests suite with some default exclusions.
    Smoketests(ForwardedArgs),
    /// Runs the keynote benchmark as a CI performance regression gate.
    ///
    /// Assumes release SpacetimeDB binaries and the TypeScript SDK are already built, runs the
    /// keynote SpacetimeDB benchmark for 60 seconds against the TypeScript and Rust modules, and
    /// fails if throughput is below 275K TPS for TypeScript or 300K TPS for Rust.
    KeynoteBench(ForwardedArgs),
    /// Tests the update flow
    ///
    /// Tests the self-update flow by building the spacetimedb-update binary for the specified
    /// target, by default the current target, and performing a self-install into a temporary
    /// directory.
    UpdateFlow(ForwardedArgs),
    /// Generates CLI documentation and checks for changes
    CliDocs(ForwardedArgs),
    /// Verify that any non-root global.json files are symlinks to the root global.json.
    GlobalJsonPolicy(ForwardedArgs),
    /// Checks that publishable crates satisfy publish constraints.
    PublishChecks(ForwardedArgs),
    /// Runs TypeScript workspace tests and template build checks.
    TypescriptTest(ForwardedArgs),
    /// Verifies that the repository version upgrade tool still works.
    VersionUpgradeCheck(ForwardedArgs),
    /// Builds the docs site.
    Docs(ForwardedArgs),
    OtherWorkflows {
        #[command(subcommand)]
        cmd: OtherWorkflowsCmd,
    },
}

#[derive(Subcommand)]
enum OtherWorkflowsCmd {
    /// Selects or starts the private workflow for a public Internal Tests run.
    CoordinateInternalTests(ForwardedArgs),
    /// Checks that sensitive CODEOWNERS-controlled files have the required approvals.
    CodeownersCheck(ForwardedArgs),
    /// Interacts with CLA Assistant.
    ClaAssistant(ForwardedArgs),
    /// Waits for a GitHub Actions workflow run to complete.
    Watch(ForwardedArgs),
}

#[derive(Args)]
#[command(disable_help_flag = true)]
struct ForwardedArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

fn run_package(package: &str, args: &[String]) -> Result<()> {
    let mut cargo_args = vec!["run", "--package", package, "--"];
    cargo_args.extend(args.iter().map(String::as_str));
    cmd("cargo", cargo_args).run()?;
    Ok(())
}

fn run_dlls() -> Result<()> {
    eprintln!("warning: `cargo ci dlls` is deprecated; use `cargo regen csharp dlls` instead");
    cmd!("cargo", "regen", "csharp", "dlls").run()?;
    Ok(())
}

fn run_command(cmd: CiCmd) -> Result<()> {
    match cmd {
        CiCmd::Test(args) => run_package("ci-test", &args.args),
        CiCmd::Lint(args) => run_package("ci-lint", &args.args),
        CiCmd::WasmBindings(args) => run_package("ci-wasm-bindings", &args.args),
        CiCmd::Dlls => run_dlls(),
        CiCmd::Smoketests(args) => run_package("ci-smoketests", &args.args),
        CiCmd::KeynoteBench(args) => run_package("ci-keynote-bench", &args.args),
        CiCmd::UpdateFlow(args) => run_package("ci-update-flow", &args.args),
        CiCmd::CliDocs(args) => run_package("ci-cli-docs", &args.args),
        CiCmd::GlobalJsonPolicy(args) => run_package("ci-global-json-policy", &args.args),
        CiCmd::PublishChecks(args) => run_package("ci-publish-checks", &args.args),
        CiCmd::TypescriptTest(args) => run_package("ci-typescript-test", &args.args),
        CiCmd::VersionUpgradeCheck(args) => run_package("ci-version-upgrade-check", &args.args),
        CiCmd::Docs(args) => run_package("ci-docs-build", &args.args),
        CiCmd::OtherWorkflows { cmd } => match cmd {
            OtherWorkflowsCmd::CoordinateInternalTests(args) => run_package("ci-coordinate-internal-tests", &args.args),
            OtherWorkflowsCmd::CodeownersCheck(args) => run_package("ci-codeowners-check", &args.args),
            OtherWorkflowsCmd::ClaAssistant(args) => run_package("ci-cla-assistant", &args.args),
            OtherWorkflowsCmd::Watch(args) => run_package("ci-workflow-watch", &args.args),
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
    let cli = Cli::parse();

    match cli.cmd {
        Some(cmd) => run_command(cmd),
        None => run_all_clap_subcommands(&cli.skip),
    }
}
