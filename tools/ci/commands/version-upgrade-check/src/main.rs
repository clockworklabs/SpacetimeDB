#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use duct::cmd;
fn run_version_upgrade_check() -> Result<()> {
    cmd!(
        "cargo",
        "bump-versions",
        "123.456.789",
        "--rust-and-cli",
        "--csharp",
        "--typescript",
        "--cpp",
        "--accept-snapshots"
    )
    .run()?;
    Ok(())
}

fn main() -> Result<()> {
    run_version_upgrade_check()
}
