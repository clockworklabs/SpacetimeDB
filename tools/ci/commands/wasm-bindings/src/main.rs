#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use ci_common::pnpm;
use clap::Parser;
use duct::cmd;
use std::path::PathBuf;

/// Tests Wasm bindings
///
/// Runs tests for the codegen crate and builds a test module with the wasm bindings.
#[derive(Parser)]
struct Cli {}

fn cargo_target_dir() -> PathBuf {
    let repo_root = ci_common::repo_root();
    match std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from) {
        Some(path) if path.is_absolute() => path,
        Some(path) => repo_root.join(path),
        None => repo_root.join("target"),
    }
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
    cmd!("cargo", "test", "-vv", "-p", "spacetimedb-codegen").run()?;
    // Pre-build the CLI so that it _doesn't_ get `cargo update`d, since that may break the build.
    cmd!("cargo", "build", "-vv", "-p", "spacetimedb-cli").run()?;
    // Make sure the `Cargo.lock` file reflects the latest available versions.
    // This is what users would end up with on a fresh module, so we want to
    // catch any compile errors arising from a different transitive closure
    // of dependencies than what is in the workspace lock file.
    //
    // For context see also: https://github.com/clockworklabs/SpacetimeDB/pull/2714
    cmd!("cargo", "update", "-vv").run()?;
    let cli_path = cargo_target_dir()
        .join("debug/spacetimedb-cli")
        .with_extension(std::env::consts::EXE_EXTENSION);
    cmd!(cli_path, "build", "--module-path", "modules/module-test",).run()?;

    Ok(())
}
