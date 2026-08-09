#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use ci_args::test::Args;
use ci_common::pnpm;
use clap::Parser;
use duct::cmd;

/// Runs tests
///
/// Runs rust tests, codegens csharp sdk and runs csharp tests.
/// This does not include Unreal tests.
/// This expects to run in a clean git state.
#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    args: Args,
}

fn main() -> Result<()> {
    Cli::parse();

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
    // The SDK test harness uses the same child-process server guard as smoketests,
    // which expects release CLI/standalone binaries to already exist.
    cmd!(
        "cargo",
        "build",
        "--release",
        "-p",
        "spacetimedb-cli",
        "-p",
        "spacetimedb-standalone",
        "--features",
        "spacetimedb-standalone/allow_loopback_http_for_tests",
    )
    .run()?;
    // SDK procedure tests intentionally make localhost HTTP requests.
    cmd!(
        "cargo",
        "test",
        "-p",
        "spacetimedb-sdk",
        "--features",
        "allow_loopback_http_for_tests",
        "--",
        "--test-threads=2",
        "--skip",
        "unreal"
    )
    .run()?;
    // Run the same SDK suite against wasm/browser test clients.
    cmd!(
        "cargo",
        "test",
        "-p",
        "spacetimedb-sdk",
        "--features",
        "allow_loopback_http_for_tests,browser",
        "--",
        "--test-threads=2",
        "--skip",
        "unreal"
    )
    .run()?;
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

    Ok(())
}
