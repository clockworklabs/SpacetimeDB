#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use clap::{Args, CommandFactory, Parser, Subcommand};
use duct::cmd;
use std::fs;
use std::path::Path;

const README_PATH: &str = "tools/ci/README.md";

/// SpacetimeDB CI tasks
///
/// This tool provides several subcommands for automating CI workflows in SpacetimeDB.
///
/// It may be invoked via `cargo ci <subcommand>`, or simply `cargo ci` to run all subcommands in
/// sequence. It is mostly designed to be run in CI environments via the github workflows, but can
/// also be run locally.
#[derive(Parser)]
#[command(name = "cargo ci", subcommand_required = false, arg_required_else_help = false)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<CiCmd>,

    /// Skip specified subcommands when running all.
    ///
    /// When no subcommand is specified, all subcommands are run in sequence. This option allows
    /// specifying subcommands to skip when running all. For example, to skip the `unreal-tests`
    /// subcommand, use `--skip unreal-tests`.
    #[arg(long, default_value = "other-workflows")]
    skip: Vec<String>,
}

#[derive(Args)]
struct ForwardedArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Subcommand)]
enum CiCmd {
    /// Runs tests.
    Test(ForwardedArgs),
    /// Lints the codebase.
    Lint(ForwardedArgs),
    /// Tests Wasm bindings.
    WasmBindings(ForwardedArgs),
    /// Deprecated; use `cargo regen csharp dlls`.
    Dlls(ForwardedArgs),
    /// Runs smoketests.
    Smoketests(ForwardedArgs),
    /// Runs the keynote benchmark as a CI performance regression gate.
    KeynoteBench(ForwardedArgs),
    /// Tests the update flow.
    UpdateFlow(ForwardedArgs),
    CliDocs(ForwardedArgs),
    SelfDocs {
        /// Only check for changes, do not generate the docs.
        #[arg(long)]
        check: bool,
    },
    GlobalJsonPolicy(ForwardedArgs),
    PublishChecks(ForwardedArgs),
    TypescriptTest(ForwardedArgs),
    VersionUpgradeCheck(ForwardedArgs),
    Docs(ForwardedArgs),
    OtherWorkflows {
        #[command(subcommand)]
        cmd: OtherWorkflowsCmd,
    },
}

#[derive(Subcommand)]
enum OtherWorkflowsCmd {
    CoordinateInternalTests(ForwardedArgs),
    CodeownersCheck(ForwardedArgs),
    ClaAssistant(ForwardedArgs),
}

fn run_package(package: &str, args: &[String]) -> Result<()> {
    let mut cargo_args = vec!["run", "--package", package, "--"];
    cargo_args.extend(args.iter().map(String::as_str));
    cmd("cargo", cargo_args).run()?;
    Ok(())
}

fn run_self_docs(check: bool) -> Result<()> {
    let readme_content = include_str!("../README.md");
    let path = Path::new(README_PATH);

    if check {
        let existing = fs::read_to_string(path).unwrap_or_default();
        if existing != readme_content {
            bail!("README.md is out of date. Please run `cargo ci self-docs` to update it.");
        }
    } else {
        fs::write(path, readme_content)?;
    }
    Ok(())
}

fn run_dlls(args: &[String]) -> Result<()> {
    if !args.is_empty() {
        bail!("cargo ci dlls does not accept arguments");
    }
    eprintln!("warning: `cargo ci dlls` is deprecated; use `cargo regen csharp dlls` instead");
    cmd!("cargo", "regen", "csharp", "dlls").run()?;
    Ok(())
}

fn run_command(cmd: CiCmd) -> Result<()> {
    match cmd {
        CiCmd::Test(args) => run_package("ci-test", &args.args),
        CiCmd::Lint(args) => run_package("ci-lint", &args.args),
        CiCmd::WasmBindings(args) => run_package("ci-wasm-bindings", &args.args),
        CiCmd::Dlls(args) => run_dlls(&args.args),
        CiCmd::Smoketests(args) => run_package("ci-smoketests", &args.args),
        CiCmd::KeynoteBench(args) => run_package("ci-keynote-bench", &args.args),
        CiCmd::UpdateFlow(args) => run_package("ci-update-flow", &args.args),
        CiCmd::CliDocs(args) => run_package("ci-cli-docs", &args.args),
        CiCmd::SelfDocs { check } => run_self_docs(check),
        CiCmd::GlobalJsonPolicy(args) => run_package("ci-global-json-policy", &args.args),
        CiCmd::PublishChecks(args) => run_package("ci-publish-checks", &args.args),
        CiCmd::TypescriptTest(args) => run_package("ci-typescript-test", &args.args),
        CiCmd::VersionUpgradeCheck(args) => run_package("ci-version-upgrade-check", &args.args),
        CiCmd::Docs(args) => run_package("ci-docs-build", &args.args),
        CiCmd::OtherWorkflows { cmd } => match cmd {
            OtherWorkflowsCmd::CoordinateInternalTests(args) => run_package("ci-coordinate-internal-tests", &args.args),
            OtherWorkflowsCmd::CodeownersCheck(args) => run_package("ci-codeowners-check", &args.args),
            OtherWorkflowsCmd::ClaAssistant(args) => run_package("ci-cla-assistant", &args.args),
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
