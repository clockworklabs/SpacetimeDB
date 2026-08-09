#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use ci_common::pnpm;
fn run_docs_build() -> Result<()> {
    pnpm(["install"]).dir("docs").run()?;
    pnpm(["build"]).dir("docs").run()?;
    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();
    run_docs_build()
}
