#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use ci_common::pnpm;
use clap::{Parser, Subcommand, ValueEnum};
use duct::cmd;

/// Runs tests
///
/// Runs rust tests, codegens csharp sdk and runs csharp tests.
/// This does not include Unreal tests.
/// This expects to run in a clean git state.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<TestCommand>,
}

#[derive(Subcommand)]
enum TestCommand {
    /// Run the Rust workspace and feature tests.
    Rust,
    /// Regenerate the C# module definition and check that it is committed.
    CsharpCodegen,
    /// Run the C# bindings tests.
    Csharp,
    /// Run a C++ compile-test suite.
    Cpp {
        #[arg(long, value_enum)]
        suite: CppSuite,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum CppSuite {
    HttpHandlers,
    Indexes,
}

impl CppSuite {
    fn as_str(self) -> &'static str {
        match self {
            Self::HttpHandlers => "http-handlers",
            Self::Indexes => "indexes",
        }
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Some(TestCommand::Rust) => rust_tests(),
        Some(TestCommand::CsharpCodegen) => csharp_codegen(),
        Some(TestCommand::Csharp) => csharp_tests(),
        Some(TestCommand::Cpp { suite }) => cpp_tests(suite),
        None => {
            rust_tests()?;
            csharp_codegen()?;
            csharp_tests()?;
            cpp_tests(CppSuite::HttpHandlers)?;
            cpp_tests(CppSuite::Indexes)
        }
    }
}

fn rust_tests() -> Result<()> {
    pnpm(["build"]).dir("crates/bindings-typescript").run()?;

    // TODO: This doesn't work on at least user Linux machines, because something here apparently uses `sudo`?

    // Exclude smoketests from `cargo test --all` since they require pre-built binaries.
    // Smoketests have their own dedicated command: `cargo ci smoketests`
    cmd!(
        "cargo",
        "test",
        "--all",
        "--exclude",
        "spacetimedb-smoketests",
        "--exclude",
        "spacetimedb-sdk",
        "--exclude",
        "spacetimedb",
        "--",
        "--test-threads=2",
        "--skip",
        "unreal"
    )
    .run()?;
    // Bindings snapshot tests rely on the unstable feature,
    // as they compile and test APIs which are gated behind that feature,
    // e.g. procedures, HTTP handlers.
    cmd!(
        "cargo",
        "test",
        "-p",
        "spacetimedb",
        "--features",
        "unstable",
        "--",
        "--test-threads=2",
    )
    .run()?;
    // SDK tests have their own dedicated, sharded command: `cargo ci sdk-tests`.
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
    Ok(())
}

fn csharp_codegen() -> Result<()> {
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
    Ok(())
}

fn csharp_tests() -> Result<()> {
    cmd!("dotnet", "test", "-warnaserror")
        .dir("crates/bindings-csharp")
        .run()?;
    Ok(())
}

fn cpp_tests(suite: CppSuite) -> Result<()> {
    cmd!(
        "bash",
        "crates/bindings-cpp/tests/compile/run-compile-tests.sh",
        "--suite",
        suite.as_str(),
    )
    .run()?;

    Ok(())
}
