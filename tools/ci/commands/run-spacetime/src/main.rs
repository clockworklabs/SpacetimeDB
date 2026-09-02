#![allow(clippy::disallowed_macros)]

use anyhow::{bail, ensure, Result};
use duct::cmd;
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

const CLI_NAME: &str = "spacetimedb-cli";
const STANDALONE_NAME: &str = "spacetimedb-standalone";

fn executable_name(name: &str) -> OsString {
    if env::consts::EXE_EXTENSION.is_empty() {
        name.into()
    } else {
        format!("{name}.{}", env::consts::EXE_EXTENSION).into()
    }
}

fn validate_runtime(cli: &Path) -> Result<()> {
    ensure!(
        cli.is_file(),
        "SpacetimeDB CLI binary does not exist at {}",
        cli.display()
    );

    let standalone = cli.with_file_name(executable_name(STANDALONE_NAME));
    ensure!(
        standalone.is_file(),
        "SpacetimeDB standalone binary does not exist beside the CLI at {}",
        standalone.display()
    );

    Ok(())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("run-spacetime package should be nested beneath the workspace root")
        .to_owned()
}

fn local_cli() -> Result<PathBuf> {
    cmd!("cargo", "build", "-p", CLI_NAME, "-p", STANDALONE_NAME).run()?;

    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join("target"));
    Ok(target_dir.join("debug").join(executable_name(CLI_NAME)))
}

fn main() -> Result<()> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        bail!("Usage: cargo spacetime <command> [args...]");
    }

    let cli = match env::var_os("SPACETIME_BIN") {
        Some(path) => {
            let path = PathBuf::from(path);
            ensure!(
                path.is_absolute(),
                "SPACETIME_BIN must be an absolute path, got {}",
                path.display()
            );
            path
        }
        None => local_cli()?,
    };

    validate_runtime(&cli)?;
    cmd(cli, args).run()?;
    Ok(())
}
