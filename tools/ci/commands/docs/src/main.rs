#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use ci_common::pnpm;

fn main() -> Result<()> {
    pnpm(["install"]).dir("docs").run()?;
    pnpm(["build"]).dir("docs").run()?;
    Ok(())
}
