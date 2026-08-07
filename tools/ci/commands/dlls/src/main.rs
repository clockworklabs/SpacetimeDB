#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use duct::cmd;
fn main() -> Result<()> {
    env_logger::init();
    eprintln!("warning: `cargo ci dlls` is deprecated; use `cargo regen csharp dlls` instead");
    cmd!("cargo", "regen", "csharp", "dlls").run()?;

    Ok(())
}
