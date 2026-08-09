use anyhow::Result;
use ci_common::ensure_repo_root;
use duct::cmd;

fn main() -> Result<()> {
    ensure_repo_root()?;
    cmd("cargo-machete", std::env::args_os().skip(1)).run()?;
    Ok(())
}
