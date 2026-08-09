#![allow(clippy::disallowed_macros)]
use anyhow::{bail, Result};
use ci_common::pnpm;

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "-h" || arg == "--help") {
        println!("Usage: cargo ci docs");
        return Ok(());
    }
    if !args.is_empty() {
        bail!("cargo ci docs does not accept arguments");
    }

    pnpm(["install"]).dir("docs").run()?;
    pnpm(["build"]).dir("docs").run()?;
    Ok(())
}
