#![allow(clippy::disallowed_macros)]

use anyhow::{Context, Result};
use clap::{Args, Command, CommandFactory, FromArgMatches, Parser, Subcommand};
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

#[derive(Args)]
struct ForwardedArgs {
    /// Arguments forwarded to the split CI command package.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Subcommand)]
enum CiCmd {
    /// Runs tests
    ///
    /// Runs rust tests, codegens csharp sdk and runs csharp tests.
    /// This does not include Unreal tests.
    /// This expects to run in a clean git state.
    #[command(override_help = "")]
    Test(ForwardedArgs),
    /// Lints the codebase
    ///
    /// Runs rustfmt, clippy, csharpier, TypeScript lint, and generates rust docs to ensure there
    /// are no warnings.
    #[command(override_help = "")]
    Lint(ForwardedArgs),
    /// Tests Wasm bindings
    ///
    /// Runs tests for the codegen crate and builds a test module with the wasm bindings.
    #[command(override_help = "")]
    WasmBindings(ForwardedArgs),
    /// Deprecated; use `cargo regen csharp dlls`.
    Dlls,
    /// Runs smoketests
    ///
    /// Executes the smoketests suite with some default exclusions.
    #[command(override_help = "")]
    Smoketests(ForwardedArgs),
    /// Runs the keynote benchmark as a CI performance regression gate.
    ///
    /// Assumes release SpacetimeDB binaries and the TypeScript SDK are already built, runs the
    /// keynote SpacetimeDB benchmark for 60 seconds against the TypeScript and Rust modules, and
    /// fails if throughput is below 275K TPS for TypeScript or 300K TPS for Rust.
    #[command(override_help = "")]
    KeynoteBench(ForwardedArgs),
    /// Tests the update flow
    ///
    /// Tests the self-update flow by building the spacetimedb-update binary for the specified
    /// target, by default the current target, and performing a self-install into a temporary
    /// directory.
    #[command(override_help = "")]
    UpdateFlow(ForwardedArgs),
    /// Generates CLI documentation and checks for changes
    #[command(override_help = "")]
    CliDocs(ForwardedArgs),
    /// Verify that any non-root global.json files are symlinks to the root global.json.
    #[command(override_help = "")]
    GlobalJsonPolicy(ForwardedArgs),
    /// Checks that publishable crates satisfy publish constraints.
    #[command(override_help = "")]
    PublishChecks(ForwardedArgs),
    /// Runs TypeScript workspace tests and template build checks.
    #[command(override_help = "")]
    TypescriptTest(ForwardedArgs),
    /// Verifies that the repository version upgrade tool still works.
    #[command(override_help = "")]
    VersionUpgradeCheck(ForwardedArgs),
    /// Builds the docs site.
    #[command(override_help = "")]
    Docs(ForwardedArgs),
    OtherWorkflows {
        #[command(subcommand)]
        cmd: OtherWorkflowsCmd,
    },
}

#[derive(Subcommand)]
enum OtherWorkflowsCmd {
    /// Selects or starts the private workflow for a public Internal Tests run.
    #[command(override_help = "")]
    CoordinateInternalTests(ForwardedArgs),
    /// Checks that sensitive CODEOWNERS-controlled files have the required approvals.
    #[command(override_help = "")]
    CodeownersCheck(ForwardedArgs),
    /// Interacts with CLA Assistant.
    #[command(override_help = "")]
    ClaAssistant(ForwardedArgs),
}

fn run_package(package: &str, args: &[String]) -> Result<()> {
    let mut cargo_args = vec!["run", "--package", package, "--"];
    cargo_args.extend(args.iter().map(String::as_str));
    cmd("cargo", cargo_args).run()?;
    Ok(())
}

fn split_command_help(package: &str) -> Result<String> {
    let help = cmd!("cargo", "run", "--quiet", "--package", package, "--", "--help")
        .env("COLUMNS", "1000")
        .read()
        .with_context(|| format!("failed to render help for `{package}`"))?;
    Ok(help.lines().map(str::trim_end).collect::<Vec<_>>().join("\n"))
}

fn set_split_help_override(command: &mut Command, path: &[&str], help: String) {
    let (name, rest) = path.split_first().expect("split command path is never empty");
    let subcommand = command
        .find_subcommand_mut(name)
        .unwrap_or_else(|| panic!("missing split subcommand `{}`", path.join(" ")));

    if rest.is_empty() {
        *subcommand = subcommand.clone().override_help(help);
    } else {
        set_split_help_override(subcommand, rest, help);
    }
}

pub(crate) fn command_with_split_help_overrides() -> Result<Command> {
    let mut command = Cli::command();

    for (path, package) in [
        (&["test"][..], "ci-test"),
        (&["lint"], "ci-lint"),
        (&["wasm-bindings"], "ci-wasm-bindings"),
        (&["smoketests"], "ci-smoketests"),
        (&["keynote-bench"], "ci-keynote-bench"),
        (&["update-flow"], "ci-update-flow"),
        (&["cli-docs"], "ci-cli-docs"),
        (&["global-json-policy"], "ci-global-json-policy"),
        (&["publish-checks"], "ci-publish-checks"),
        (&["typescript-test"], "ci-typescript-test"),
        (&["version-upgrade-check"], "ci-version-upgrade-check"),
        (&["docs"], "ci-docs-build"),
        (
            &["other-workflows", "coordinate-internal-tests"],
            "ci-coordinate-internal-tests",
        ),
        (&["other-workflows", "codeowners-check"], "ci-codeowners-check"),
        (&["other-workflows", "cla-assistant"], "ci-cla-assistant"),
    ] {
        let help = split_command_help(package)?;
        set_split_help_override(&mut command, path, help);
    }

    Ok(command)
}

fn should_load_split_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h" || arg == "help")
}

fn parse_cli() -> Result<Cli> {
    let args = std::env::args().collect::<Vec<_>>();
    if should_load_split_help(&args) {
        let matches = command_with_split_help_overrides()?.get_matches_from(args);
        Ok(Cli::from_arg_matches(&matches)?)
    } else {
        Ok(Cli::parse_from(args))
    }
}

fn run_smoketests(args: &[String]) -> Result<()> {
    let mut args = args.to_vec();
    if args.first().is_some_and(|arg| arg.starts_with("--test-")) {
        args.insert(0, "--".to_string());
    }
    run_package("ci-smoketests", &args)
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
    let cli = parse_cli()?;

    match cli.cmd {
        Some(cmd) => run_command(cmd),
        None => run_all_clap_subcommands(&cli.skip),
    }
}
