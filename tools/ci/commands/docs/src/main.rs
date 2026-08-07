#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use duct::{cmd, Expression};
use std::ffi::OsStr;
fn pnpm<I, S>(args: I) -> Expression
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<std::ffi::OsString> = args.into_iter().map(|a| a.as_ref().to_os_string()).collect();
    if cfg!(windows) {
        let mut full: Vec<std::ffi::OsString> = vec!["/c".into(), "pnpm".into()];
        full.extend(args);
        cmd("cmd", full)
    } else {
        cmd("pnpm", args)
    }
}
fn run_docs_build() -> Result<()> {
    pnpm(["install"]).dir("docs").run()?;
    pnpm(["build"]).dir("docs").run()?;
    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();
    run_docs_build()
}
