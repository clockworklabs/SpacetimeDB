#![allow(clippy::disallowed_macros)]

use anyhow::Result;
use clap::Parser;
use duct::cmd;

/// Checks the portable datastore wasm boundary.
///
/// Ensures the table, datastore portable feature, portable datastore crate, and
/// wasm adapter compile for wasm32-unknown-unknown, then runs the portable
/// datastore unit tests natively.
#[derive(Parser)]
struct Cli {}

fn main() -> Result<()> {
    Cli::parse();

    cmd!(
        "cargo",
        "check",
        "-p",
        "spacetimedb-table",
        "--target",
        "wasm32-unknown-unknown"
    )
    .run()?;
    cmd!(
        "cargo",
        "check",
        "-p",
        "spacetimedb-datastore",
        "--no-default-features",
        "--features",
        "portable",
        "--target",
        "wasm32-unknown-unknown"
    )
    .run()?;
    cmd!(
        "cargo",
        "clippy",
        "-p",
        "spacetimedb-datastore",
        "--no-default-features",
        "--features",
        "portable",
        "--target",
        "wasm32-unknown-unknown",
        "--",
        "-D",
        "warnings",
    )
    .run()?;
    cmd!(
        "cargo",
        "check",
        "-p",
        "spacetimedb-portable-datastore",
        "--target",
        "wasm32-unknown-unknown"
    )
    .run()?;
    cmd!(
        "cargo",
        "check",
        "-p",
        "spacetimedb-portable-datastore-wasm",
        "--target",
        "wasm32-unknown-unknown"
    )
    .run()?;
    cmd!("cargo", "test", "-p", "spacetimedb-portable-datastore").run()?;

    Ok(())
}
