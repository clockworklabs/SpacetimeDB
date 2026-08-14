use anyhow::Result;
use ci_common::ensure_repo_root;
use duct::cmd;
use std::ffi::OsString;

fn main() -> Result<()> {
    ensure_repo_root()?;
    let mut args = vec![OsString::from("+1.95.0"), OsString::from("shear")];
    args.extend(std::env::args_os().skip(1));
    cmd("cargo", args).run()?;
    Ok(())
}
