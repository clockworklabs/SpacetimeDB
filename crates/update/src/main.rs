use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;

mod cli;
mod proxy;

fn main() -> anyhow::Result<ExitCode> {
    let mut args = std::env::args_os();
    let argv0: PathBuf = args.next().unwrap().into();
    let env_cmd = std::env::var_os("SPACETIMEDB_UPDATE_MULTICALL_APPLET");
    let cmd = if let Some(cmd) = &env_cmd {
        cmd
    } else {
        argv0.file_stem().context("argv0 must have a filename")?
    };
    if cmd == "spacetimedb-update" {
        spacetimedb_update_main()
    } else if cmd == "spacetime" {
        proxy::run_cli(None, Some(argv0.as_os_str()), args.collect())
    } else {
        anyhow::bail!(
            "unknown command name for spacetimedb-update multicall binary: {}",
            Path::new(cmd).display()
        )
    }
}

fn spacetimedb_update_main() -> anyhow::Result<ExitCode> {
    let args = cli::Args::parse();
    args.exec()
}