#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use clap::{Args, CommandFactory, Parser, Subcommand};
use duct::cmd;
use std::fs;
use std::path::Path;

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
    #[arg(long, default_value = "other-workflows")]
    skip: Vec<String>,
}

#[derive(Args)]
struct ForwardedArgs {
    /// Arguments forwarded to the split CI command package.
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
    Dlls,
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

pub(crate) fn split_command_help_package(path: &[&str]) -> Option<&'static str> {
    match path {
        ["smoketests"] => Some("ci-smoketests"),
        ["update-flow"] => Some("ci-update-flow"),
        ["cli-docs"] => Some("ci-cli-docs"),
        ["other-workflows", "coordinate-internal-tests"] => Some("ci-coordinate-internal-tests"),
        ["other-workflows", "codeowners-check"] => Some("ci-codeowners-check"),
        ["other-workflows", "cla-assistant"] => Some("ci-cla-assistant"),
        _ => None,
    }
}

fn split_help_command(args: &[String]) -> Option<(&'static str, Vec<String>)> {
    if let Some(help_path) = args.strip_prefix(&["help".to_string()]) {
        for path_len in [2, 1] {
            if help_path.len() >= path_len {
                let path = help_path[..path_len].iter().map(String::as_str).collect::<Vec<_>>();
                if let Some(package) = split_command_help_package(&path) {
                    return Some((package, vec!["--help".to_string()]));
                }
            }
        }
    }

    if !args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return None;
    }

    for path_len in [2, 1] {
        if args.len() >= path_len {
            let path = args[..path_len].iter().map(String::as_str).collect::<Vec<_>>();
            if let Some(package) = split_command_help_package(&path) {
                return Some((package, args[path_len..].to_vec()));
            }
        }
    }

    None
}

fn run_smoketests(args: &[String]) -> Result<()> {
    let mut args = args.to_vec();
    if args.first().is_some_and(|arg| arg.starts_with("--test-")) {
        args.insert(0, "--".to_string());
    }
    run_package("ci-smoketests", &args)
}

fn run_self_docs(check: bool) -> Result<()> {
    let readme_content = ci_docs::generate_cli_docs()?;
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
        CiCmd::Smoketests(args) => run_smoketests(&args.args),
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
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Some((package, args)) = split_help_command(&args) {
        return run_package(package, &args);
    }

    let cli = Cli::parse();

    match cli.cmd {
        Some(cmd) => run_command(cmd),
        None => run_all_clap_subcommands(&cli.skip),
    }
}
