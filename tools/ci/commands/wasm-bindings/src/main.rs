#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use ci_args::wasm_bindings::Args;
use ci_common::pnpm;
use clap::Parser;
use duct::cmd;

/// Tests Wasm bindings
///
/// Runs tests for the codegen crate and builds a test module with the wasm bindings.
#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    args: Args,
}

fn main() -> Result<()> {
    Cli::parse();

    pnpm([
        "install",
        "--filter",
        "./crates/bindings-typescript...",
        "--filter",
        "./modules/module-test-ts...",
    ])
    .run()?;
    pnpm(["build"]).dir("crates/bindings-typescript").run()?;
    cmd!("cargo", "test", "-p", "spacetimedb-codegen").run()?;
    // Pre-build the CLI so that it _doesn't_ get `cargo update`d, since that may break the build.
    cmd!("cargo", "build", "-p", "spacetimedb-cli").run()?;
    // Make sure the `Cargo.lock` file reflects the latest available versions.
    // This is what users would end up with on a fresh module, so we want to
    // catch any compile errors arising from a different transitive closure
    // of dependencies than what is in the workspace lock file.
    //
    // For context see also: https://github.com/clockworklabs/SpacetimeDB/pull/2714
    cmd!("cargo", "update").run()?;
    let cli_path = ci_common::repo_root()
        .join("target/debug/spacetimedb-cli")
        .with_extension(std::env::consts::EXE_EXTENSION);
    cmd!(cli_path, "build", "--module-path", "modules/module-test",).run()?;

    Ok(())
}
